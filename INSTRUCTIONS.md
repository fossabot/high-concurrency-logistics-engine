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


## Environment Variables

Create a `.env` file at the workspace root:

```env
# Database
# Remember DATABASE_URL and postgres user details should match
# Format of the URL postgres://POSTGRES_USER:POSTGRES_PASSWORD@POSTGRES_HOST:5432/POSTGRES_DB
DATABASE_URL=postgres://prati:Source@host.docker.internal:5432/parcel
POSTGRES_USER= name
POSTGRES_PASS=your password
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
docker compose --rm  run k6-test
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
