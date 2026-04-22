# Real-Time Parcel Tracking System (Rust)

STATUS: WORK IN PROGRESS


> **Current Milestone:** Single-node optimization (10k Concurrent VUs achieved).
> **Next Milestone:** Horizontal scaling with Nginx Load Balancer and Redis Cluster.

> A production-grade distributed backend for live courier tracking, built in Rust. Handles **10,000 concurrent WebSocket connections** with **100% success rate**, **6.35ms average connection time**, on **under1 CPU cores** and **under1GB RAM** — entire stack included.

**Built as a case study** of how a real parcel delivery platform handles thousands of drivers simultaneously sending location updates while customers receive live tracking in real time.

---

## The Problem

Parcel delivery platforms have a hard real-time problem:

- Thousands of drivers sending GPS coordinates every 2 seconds
- Customers expecting live location updates with no perceptible lag
- Systems that must not lose a position event or process one twice
- Infrastructure that must scale horizontally without duplicate processing

This system solves all four end to end.

---

## Architecture

```
Driver Device (GPS update every 2s)
        │
        ▼
┌─────────────────────────────────┐
│   Axum WebSocket Handler        │
│   Ed25519 JWT Authentication    │
│   parcel_id + driver_id +       │
│   lat/lng + timestamp (u64)     │
└──────────────┬──────────────────┘
               │
               ▼
┌─────────────────────────────────┐
│   Redis Lua Atomic Script       │
│   HGET → last known position    │
│   Compare → deduplicate         │
│   HSET → update position        │
│   XADD → Redis Stream           │
└──────────────┬──────────────────┘
               │
               ▼
┌─────────────────────────────────┐
│   Redis Stream Consumer Group   │
│   Unique worker ID via OnceLock │
│   Each container gets own ID    │
│   No duplicate message processing│
└──────────────┬──────────────────┘
               │
        ┌──────┴──────┐
        ▼             ▼
┌──────────────┐  ┌────────────────────┐
│  Postgres    │  │  Customer WebSocket │
│  Batch Writer│  │  Live position push │
│  1000 entries│  │  or last known pos  │
│  SQLx unnest │  │  from Redis cache   │
└──────────────┘  └────────────────────┘
```




---

## Key Engineering Decisions

### Ed25519 JWT Authentication
I went with Ed25519 for the JWTs instead of the usual HS256. I wanted to separate the signing responsibility, and since it’s faster/smaller than RSA, it didn't kill my latency during the handshake.

### Atomic Deduplication with Redis Lua Scripts
Before writing to the stream, the system checks the driver's last known position via `HGET`. This check and the subsequent `XADD` are wrapped in a **Lua script executed atomically server-side in Redis**. No race condition where two simultaneous updates both pass the check and both write to the stream. Exactly-once semantics at the ingestion layer.

### Horizontal Scaling via Redis Consumer Groups with OnceLock
The system scales across multiple containers from day one. Each container participates in a Redis Stream consumer group with a **unique worker identity generated at startup using OnceLock** — initialized exactly once per process, thread-safely, without Mutex overhead on every request. Dead containers are detected automatically and their pending entries requeued to active consumers.

### Postgres Batch Writing with SQLx Unnest
A background Tokio task collects stream entries and writes them to Postgres using **SQLx `unnest` batching — 3000 entries per transaction**. This protects the database from write amplification during peak load. At 10,000 concurrent drivers Postgres used only **4% CPU and 42MB RAM** — proof the batching strategy works correctly.

### Customer Live Tracking with Graceful Fallback
Customers connect via WebSocket and receive live driver position updates pushed from the Redis Stream. If no live update is available, the server returns the **last known position from Redis cache** — so customers always see something meaningful even during brief network gaps. Inactive customer connections are detected via Ping/Pong heartbeat and removed cleanly.

### Tokio Pin with Interval Ticks
Stream publishing uses `tokio::time::interval` with `Pin` for precise async timing control — ensuring interval ticks fire at the correct cadence without drifting under load. Critical when position updates need 2-second resolution across 15,000 simultaneous connections.

### Structured Tracing for Observability
The system uses `tracing` with structured spans rather than log lines. Every request can be followed through the full stack — WebSocket upgrade, position check, stream write, consumer processing, database batch. Production observability via **Prometheus + Grafana + Node Exporter**.

### Arc AppState for Safe Shared State
Application state — database pool, Redis connection manager, JWT keys — is wrapped in `Arc` and shared safely across async threads without cloning expensive resources on every request.

---

## Tech Stack

| Layer | Technology | Why |
|---|---|---|
| Language | Rust | Memory safety, zero GC, zero-cost abstractions |
| Async Runtime | Tokio | Industry standard, precise timer control with Pin |
| Web Framework | Axum | Ergonomic, Tokio-native, strong middleware support |
| Auth | JWT + Ed25519 | Asymmetric, fast, stateless across WebSocket and REST |
| Cache / Streams | Redis | Lua atomic ops, consumer groups, TTL session storage |
| Database | PostgreSQL + SQLx | Type-safe queries, unnest batch inserts |
| Observability | Prometheus + Grafana + Node Exporter | Full production metrics stack |
| Containerization | Docker Compose | Linux kernel networking, health checks |
| Load Testing | k6 + token-gen | Ed25519 signed tokens + WebSocket load testing |

---

## API Endpoints

| Method | Endpoint | Auth | Description |
|---|---|---|---|
| POST | `/register` | None | Register new user |
| POST | `/login` | None | Login, receive JWT |
| POST | `/verify` | None | Verify account |
| GET | `/ws?parcel_id=x&role=driver` | JWT | Driver WebSocket — send location updates |
| GET | `/customer?parcel_id=x&role=customer` | JWT | Customer WebSocket — receive live tracking |
| GET | `/health` | None | Health check |
| GET | `/metrics` | None | Prometheus metrics |

---

## WebSocket Message Format

**Driver → Server (every 2 seconds):**
```json
{
  "parcel_id": "parcel-123",
  "driver_id": "driver-456",
  "timestamp": 1714123456,
  "latitude": 12.9716,
  "longitude": 77.5946,
  "status": "picked_up"
}
```

**Server → Driver:**
```json
{ "status": "ok" }
```

**Server → Customer (live update):**
```json
{
  "parcel_id": "parcel-123",
  "latitude": 12.9716,
  "longitude": 77.5946,
  "timestamp": 1714123456
}
```

---

## Load Test Results

> Full methodology, stages, and raw output: [LOADTEST.md](./LOADTEST.md)

| Metric | Result |
|---|---|
| Concurrent WebSocket VUs | 15,000 |
| Success rate | 99.9999% |
| WebSocket errors | 0 |
| Avg connection time | 6.35ms |
| p95 connection time | 19.5ms |
| Location updates processed | 4,994,895 |
| Sustained throughput | 4,625 updates/second |

### Resource Usage at 15,000 Concurrent Connections

| Component | CPU | RAM | Network I/O |
|---|---|---|---|
| Rust API (Axum) | 1.45 cores | 1.873GB | 1.55GB in / 3.1GB out |
| Redis | 0.42 cores | 55MB | 1.79GB in / 215MB out |
| Postgres | 0.04 cores | 42MB | 8.22KB in / 3.97KB out |
| **Total Stack** | **1.91 cores** | **~1.97GB** | — |

### Scaling Analysis

| Connections | Status | Notes |
|---|---|---|
| 5,000 | ✓ Zero errors | Baseline proven |
| 10,000 | ✓ Zero errors | C10K solved |
| 15,000 | ✓ 99.9999% success | C15K proven |
| ~35,000 | Estimated ceiling | Redis CPU saturates |
| ~100,000 | Horizontal scaling needed | Redis Cluster + multiple nodes |

---

## Environment Variables

Create a `.env` file at the workspace root:

```env
# Database
DATABASE_URL=postgres://postgres:yourpassword@postgres_db:5432/postgres

# Redis
REDIS_URL=redis://redis:6379

# Server
PORT=8080
HOST=0.0.0.0
RUST_LOG=info

# JWT — leave empty on first run
# Server generates fresh Ed25519 keys and prints them on startup
# Copy printed values here for all subsequent runs
JWT_PRIVATE_KEY=
JWT_PUBLIC_KEY=

# Email verification
SMTP_USERNAME=your_email@gmail.com
SMTP_PASSWORD=your_smtp_app_password

# Load testing
TOKEN_COUNT=15000
TOKEN_OUTPUT=/loadtests/tokens.txt
```

**First run JWT key setup:**
```bash
# 1. Leave JWT_PRIVATE_KEY and JWT_PUBLIC_KEY empty
# 2. Start the server — it prints fresh keys:
#    SAVE THESE TO YOUR .ENV:
#    JWT_PRIVATE_KEY=abc123...
#    JWT_PUBLIC_KEY=def456...
# 3. Copy both values into .env
# 4. Restart — server loads existing keys
```

---

## Getting Started

### Prerequisites

- Docker and Docker Compose
- Rust 1.70+
- k6 (for load testing)

### Step 1 — Clone and Configure

```bash
git clone https://github.com/Prati-source/axum_api
cd axum_api
cp .env.example .env
# Edit .env — fill in SMTP credentials, leave JWT keys empty for now
```

### Step 2 — First Run (Generate JWT Keys)

```bash
docker compose up --build -d
# Wait for server to start
# Copy printed JWT_PRIVATE_KEY and JWT_PUBLIC_KEY into .env
docker compose down
```

### Step 3 — Full Run

```bash
docker compose up
```

### Step 4 — Verify

```bash
# Health check
curl http://localhost:8080/health

# Prometheus targets — both should show UP
open http://localhost:9090/targets

# Grafana dashboards
open http://localhost:3001
# Login: admin / admin
```

### Step 5 — Run Load Tests

```bash
# Generate 15,000 Ed25519 signed tokens
cargo run -p token-gen

# Run driver load test
k6 run loadtests/driver.js

# Run customer load test  
k6 run loadtests/customer.js

# Or run everything via Docker Compose
docker compose --profile --rm test run k6-test
```

### Local Development (without Docker)

```bash
# Start Redis and Postgres via Docker
docker compose up redis postgres_db

#Create a TABLE in PostgreSQL
sqlx migrate Run

#for temperory variable in UNNEST for Batching 
cargo sqlx prepare

# Run API locally
cargo run -p axum-api

# Lint and format
cargo clippy
cargo fmt
```

---

## Project Structure

```
axum_api/
├── Cargo.toml              ← workspace root (resolver = "2")
├── .env                    ← environment variables
├── .env.example            ← template for new contributors
├── docker-compose.yml      ← full stack orchestration
├── prometheus.yml          ← Prometheus scrape config
├── axum-api/               ← main API
│   ├── Cargo.toml
│   ├── Dockerfile
│   └── src/
|       |--middleware/auth  <- Jwt token verification
│       ├── main.rs         ← server startup, router, AppState
│       ├── handlers/
│       │   ├── ws.rs       ← driver WebSocket handler for driver
│       │   ├── auth.rs     ← register, login, verify
│       │   └── customer.rs ← customer live tracking handler
│       ├── bus/redis_bus/      ← Redis Stream, Lua scripts, consumer group
│       └── models/          ← SQLx database models and State models
        |___components/password  ← Ed25519 JWT token generator for load testing
├── token-gen/                 
│   ├── Cargo.toml
|   |--Dockerfile
│   └── src/
│       └── main.rs
└── loadtests/              ← k6 load test scripts
    ├── driver.js           ← driver WebSocket load test
    └── customer.js        ← customer tracking load test
         token-output.txt   TOKENS Stored here
```

---

## Author
Note: Git history was reset during a major directory restructuring/refactor on 21/04/2026."
**Pramod S B**
Backend Engineer — Real-time distributed systems in Rust
Bengaluru, India
[github.com/Prati-source](https://github.com/Prati-source)
