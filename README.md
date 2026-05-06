# Real-Time Parcel Tracking System (Rust)

![GKE](https://img.shields.io/badge/gke-autopilot-4285F4?logo=google-cloud&logoColor=white)

STATUS: Horizontally Scaled Backend with Redis Cluster and Postgresql IN Google Kubernetes Engine

## Cloud Deployment (GKE)

K6 loadtest run on the real live VM Instance and Google Kubernete Engine (GKE)
![Baseline Load Test](./assets/gke_loadtest.png)
![Grafana Heatmap view from source Postgres](./assets/heatmap_grafana.png)

The system is architected to run on **GKE Autopilot**. 
Kubernetes manifests are available in the `/k8s` directory.

**Key Configuration:**
- **Ingress:** Nginx Controller configured for WebSocket upgrade headers.
- **HPA:** CPU-based autoscaling triggers at 70% utilization.
- **Secrets:** Ed25519 keys injected via Kubernetes Secrets.


> A production-grade distributed backend for live courier tracking, built in Rust. Handles **10,000 concurrent WebSocket connections** with **100% success rate**

**Built as a case study** of how a real parcel delivery platform handles thousands of drivers simultaneously sending location updates while customers receive live tracking in real time.

---

## The Problem

Parcel delivery platforms have a hard real-time problem:

- Thousands of drivers sending GPS coordinates every 2 seconds
- Customers expecting live location updates with no perceptible lag
- Systems that must not lose a position event or process one twice
- Infrastructure that must scale horizontally without duplicate processing

This system solves all four end to end.

## Architecture

The system uses an asynchronous, non-blocking architecture to decouple high-frequency ingestion from database persistence.  

DRIVER LOGIC
```mermaid
sequenceDiagram
    participant C as Driver (5k)
    participant N as Ingress-Nginx (LB)
    participant R as Rust (Axum)
    participant D as Redis (Hot Store)
    participant P as Postgres (Cold Store)

    C->>N: WebSocket Stream
    N->>R: Upgrade & Forward
    
    Note over R: High-Frequency Loop
    
    rect rgb(20, 20, 20)
        Note right of R: The "Hot Path" (< 3ms)
        R->>R: Ed25519 Verify
        R->>D: Atomic Lua Update
        D-->>R: Ack (New State)
    end

    par Async Persistence
        R-->>C: 200 OK (Ack)
        and
        R->>R: Send to MPSC Channel
        R->>P: Batch Insert (Background Task)
    end
```

CUSTOMER LOGIC
```mermaid
sequenceDiagram
    participant C as Customer (5k)
    participant N as Nginx (LB)
    participant R as Rust (Axum)
    participant D as Redis (Hot Store)

    C->>N: WebSocket Stream
    N->>R: Upgrade & Forward
    
    Note over R: High-Frequency Loop
    
    rect rgb(20, 20, 20)
        Note right of R: The "Hot Path" (< 3ms)
        R->>R: Ed25519 Verify
        R->>D: Subscribe Channel 
        D-->>R: LocationUpdates of Parcel
    end

    par Async Persistence
        R-->>C: Location Update
        and
        R->>R: DashMap Broadcast Channel
    end
```
    
![Architecture Diagram](assets/first_test_rediscluster_k6_results.png)

---

- [Architecture Overview](ARCHITECTURE.md)

- [Architecture Decisions](DECISIONS.md)

- [Mistakes](MISTAKES.md)




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
  "driver_id": "driver-123",
  "latitude": 12.9716,
  "longitude": 77.5946,
  "timestamp": 1714123456,
  "status": "picked_up"
}
```

---

## Load Test Results (Production Environment)

**Infrastructure:** Google Kubernetes Engine (GKE) Autopilot
**Cluster Region:** usa-central1 
**Resources:** 
- API Pods: 1.5 vCPU / 2GB RAM (Horizontal Pod Autoscaling Enabled)
- Redis: Cluster Mode (3 Primaries, 3 Replicas)
- Ingress: Nginx Ingress Controller with tuned `worker_connections`

| Metric | Result |
|---|---|
| **Concurrent Users** | **10,000** |
| **Environment** | **GKE Autopilot** |
| **p50 Latency** | **3ms** |
| **p99 Latency** | **19ms**  |



## Grafana Dashboard

For 14000 VUs, 7000 Driver VUs and 7000 Customer VUs

![Architecture Diagram](assets/customer_and_driver_load_test.png)

[![Grafana Dashboard](images/grafana_dashboard.png)](https://snapshots.raintank.io/dashboard/snapshot/cdbSuswQA77SlNUAsmZAqyyqTR0mqPXG)



### Scaling Analysis Locally

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
# Remember DATABASE_URL and postgres user details should match
# Format of the URL postgres://POSTGRES_USER:POSTGRES_PASSWORD@POSTGRES_HOST:5432/POSTGRES_DB
DATABASE_URL=postgres://prati:Source@host.docker.internal:5432/parcel
POSTGRES_USER=prati
POSTGRES_PASSWORD=Source
POSTGRES_HOST=host.docker.internal
POSTGRES_DB=parcel          

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

### Step 3 - Test

```bash
# Run Integration test of Backend to Ensure Everythings working
docker compose exec axum-api cargo test
```

### Step 4 — Full Run

```bash
docker compose up 
```

### Step 5 — Verify

```bash
# Health check
curl http://localhost:8080/health

# Prometheus targets — both should show UP
open http://localhost:9090/targets

# Grafana dashboards
open http://localhost:3001
# Login: admin / admin
```

### Step 6 — Run Load Tests

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
# Start Redis Cluster and Postgres via Docker
# NOTE: REMEMBER SETTING UP LOCALLY REQUIRES DEEP KNOWLEDGE OF ALL CONNECTIONS 
# with same credentials as in .env or it will not connect
# Remember to have configuration from deployment yaml or it wont works for ports and commands
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
│   ├── tests/auth_test.rs
│   └── src/
|       ├── middleware/auth  <- Jwt token verification
│       ├── main.rs         ← server startup, router, AppState
│       ├── handlers/
│       │   ├── ws.rs       ← driver WebSocket handler for driver
│       │   ├── auth.rs     ← register, login, verify
│       │   └── customer.rs ← customer live tracking handler
│       ├── bus/redis_bus/   ← Redis Stream, Lua scripts, consumer group
│       ├── models/          ← SQLx database models, custom error models and State models
│       └──components  
│              ├──password     ← Ed25519 JWT token generator for load testing
│              ├──background   ← Batch operations running for postgres
│              ├──batch_postgres  ← Sqlx Unnest and parse StreamId to location update
│              └──redis_read_background  ← Redis Subscriber Message Receiving  operations running for postgres
├── token-gen/                 
│   ├── Cargo.toml
|   ├──Dockerfile
│   └── src/
│       └── main.rs
└── loadtests/              ← k6 load test scripts
    ├── driver.js           ← driver WebSocket load test
    └── customer.js        ← customer tracking load test
         token-output.txt   TOKENS Stored here
         
```

## Load Test Results
## LOCALY RUN INITIAL TEST RESULTS 
> Full methodology, stages, and raw output with the cluster test in docker compose: [CLUSTERLOADTEST.md](./CLUSTERLOADTEST.md)

> Full methodology, stages, and raw output with the singleredis node test docker compose: [SINGLELOADTEST.md](./SINGLELOADTEST.md)

---

## Author
Note: Git history was reset during a major directory restructuring/refactor on 21/04/2026."
**Pramod S B**
Backend Engineer — Real-time distributed systems in Rust
Bengaluru, India
[github.com/Prati-source](https://github.com/Prati-source)
