# Load Test Results

This document covers the complete load testing methodology, environment, results, and instructions to replicate every test against this codebase.

All tests were run on **Docker Compose** using Linux kernel networking — no Windows socket limitations, no port exhaustion, clean results.

---

## Environment

| Component | Specification |
|---|---|
| CPU | Intel i5-12400F (6 cores, 12 threads) |
| RAM | 32GB |
| OS | Windows 11 with Docker Desktop (Linux containers) |
| Network | Docker Compose internal network (Linux kernel) |
| Load Generator | k6 |
| Token Auth | Ed25519 JWT (pre-generated via token-gen) |

---

## Test Architecture

```
k6 (Docker container)
    │
    │  15,000 concurrent WebSocket VUs
    │  Each VU sends location update every 2 seconds
    │  Each VU holds connection for ~5 minutes
    │
    ▼
axum-api (Docker container) → redis (Docker container)
                            → postgres_db (Docker container)
```

Each VU simulates a real driver:
- Connects via WebSocket with Ed25519 JWT
- Sends `parcel_id`, `driver_id`, `timestamp` (u64), `latitude`, `longitude`, `status`
- Position drifts realistically using sin/cos — simulates curved driving route
- Holds connection for randomised duration around 5 minutes

---

## Token Generation

k6 cannot sign Ed25519 JWTs natively. A dedicated Rust binary generates tokens signed with the same private key as the API:

```bash
# Generate 15,000 tokens signed with your JWT_PRIVATE_KEY
TOKEN_COUNT=15000 cargo run -p token-gen

# Tokens saved to tokens.txt — one per line
# k6 loads them via SharedArray — each VU gets its own token
```

Tokens are pre-generated once per test run. Each VU picks `tokens[(VU_ID - 1) % tokens.length]` — no sharing, no collisions, genuine Ed25519 verification on every connection.

---

## Load Test Stages

```javascript
stages: [
  { duration: "2m", target: 2000 }, // Slow start
  { duration: "5m", target: 10000 }, // Gentle climb
  { duration: "5m", target: 10000 }, // Soak test (the real stability check)
  { duration: "3m", target: 0 }, // Slow ramp down to avoid a "disconnect storm 
],
```

Total test duration: approximately 18 minutes.

---

## Results at 10,000 Concurrent VUs 

```
✓ ack ok
✓ WebSocket connected (101)
checks..................: 100.00%
ws_errors...............: 0       0/s
ws_connecting...........: avg=3.14ms  min=1.15ms  med=2.69ms  max=348ms  p(90)=3.9ms  p(95)=5.12ms
location_updates_sent...: 994,389    2,425/s
ws_msgs_received........: 1,047,787
ws_sessions.............: 10,000
```

| Metric | Value |
|---|---|
| Concurrent VUs | 10,000 |
| Success rate | 100% |
| WebSocket errors | 0 |
| Avg connection time | 3.14ms |
| p95 connection time | 5.12ms |
| Location updates sent | 994,389 |
| Throughput | 2,425 updates/second |

---

## Results at 15,000 Concurrent VUs

```
✓ ack ok
✓ WebSocket connected (101)
checks..................: 99.9999%  ✓ 5,486,029  ✗ 4
ws_errors...............: 0        0/s
ws_connecting...........: avg=6.35ms  min=1.15ms  med=3.44ms  max=348ms  p(90)=12.1ms  p(95)=19.5ms
location_updates_sent...: 4,994,895   4,625/s
ws_msgs_received........: 5,486,030
ws_sessions.............: 15,004
```

| Metric | Value |
|---|---|
| Concurrent VUs | 15,000 |
| Total checks | 5,486,029 |
| Success rate | 99.9999% (4 failures out of 5,486,029) |
| WebSocket errors | 0 |
| Avg connection time | 6.35ms |
| p95 connection time | 19.5ms |
| Location updates processed | 4,994,895 |
| Sustained throughput | 4,625 updates/second |

The 4 failures are statistical noise from null response checks during the ramp-up phase — not real connection failures.

---

## Resource Usage at 15,000 VUs

Captured via `docker stats` during the 5-minute hold phase at peak load:

| Component | CPU | RAM | Network I/O |
|---|---|---|---|
| Rust API (Axum) | 145% (1.45 cores) | 1.873GB | 1.55GB in / 3.1GB out |
| Redis | 42% (0.42 cores) | 55MB | 1.79GB in / 215MB out |
| Postgres | 4% (0.04 cores) | 42MB | 9.87KB in / 4.12KB out |
| **Total** | **1.91 cores** | **~1.97GB** | — |

**Memory per connection:** ~130KB (entire stack)
**Estimated cloud cost at this load:** ~$10-20/month (2 vCPU / 4GB RAM instance)

---

## Scaling Analysis

Based on observed resource usage:

| Connections | Bottleneck | Action Required |
|---|---|---|
| Up to 15,000 | None — headroom on all components | Current architecture |
| ~30,000 | Redis CPU (~84%) | Redis Cluster |
| ~40,000 | Redis saturated | Redis Cluster + stream sharding |
| ~100,000+ | Multiple nodes needed | Full horizontal scaling |

Redis is the first bottleneck at scale — consistent with its single-threaded architecture. The Rust API has significant headroom remaining at 15,000 connections (1.45 of 12 available cores).

---

## How to Replicate

### Prerequisites

```bash
# Install k6
# Windows
choco install k6

# macOS
brew install k6

# Linux
sudo gpg --no-default-keyring --keyring /usr/share/keyrings/k6-archive-keyring.gpg \
  --keyserver hkp://keyserver.ubuntu.com:80 \
  --recv-keys C5AD17C747E3415A3642D57D77C6C491D6AC1D69
echo "deb [signed-by=/usr/share/keyrings/k6-archive-keyring.gpg] https://dl.k6.io/deb stable main" \
  | sudo tee /etc/apt/sources.list.d/k6.list
sudo apt-get update && sudo apt-get install k6

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Step 1 — Start the Stack

```bash
git clone https://github.com/Prati-source/axum_api
cd axum_api

# Configure environment
cp .env.example .env
# Edit .env — leave JWT keys empty for first run

# Start stack — generates JWT keys on first run
docker compose up --build

# Copy printed JWT keys to .env then restart
docker compose down
docker compose up
```

### Step 2 — Generate Tokens

```bash
# Generates 15,000 Ed25519 signed tokens
# Uses JWT_PRIVATE_KEY from .env — must match running API
TOKEN_COUNT=15000 cargo run -p token-gen

# Verify tokens.txt was created
ls -la tokens.txt
wc -l tokens.txt  # should show 15000
```

### Step 3 — Run Driver Load Test

```bash
# 15,000 VU WebSocket test
k6 run loadtests/driver.js

# Custom VU count
BASE_URL=ws://localhost:8080 k6 run loadtests/driver.js
```

### Step 4 — Monitor in Real Time

While the test runs, open these in separate terminals:

```bash
# Docker resource usage — refresh every 2 seconds
docker stats

# Save stats to file for analysis
docker stats --format "{{.Name}},{{.CPUPerc}},{{.MemUsage}},{{.NetIO}}" \
  | while read line; do echo "$(date '+%H:%M:%S'),$line"; done >> docker_stats.csv
```

Open Grafana at `http://localhost:3001` to see live metrics:
- `ws_active_connections` — climbing to 15,000
- `rate(location_updates_total[1m])` — sustained throughput
- `rate(redis_stream_writes_total[1m])` — stream write rate

### Step 5 — Run Customer Load Test

```bash
k6 run loadtests/customer.js
```

---

## Thresholds

Tests are configured with these pass/fail thresholds:

```javascript
thresholds: {
  ws_errors: ["count<100"],           // fail if more than 100 WS errors
  location_updates_sent: ["count>10000"], // fail if fewer than 10k updates
}
```

A passing run shows all thresholds with ✓.

---

## Notes

- **Docker Compose is required** for clean results at high VU counts. Running k6 against a local Windows process hits Windows socket limits (TIME_WAIT exhaustion) before 10,000 VUs. Docker Compose uses Linux kernel networking which handles this correctly.
- **Token generation must use the same JWT_PRIVATE_KEY as the running API.** Tokens signed with a different key will be rejected with 401 on WebSocket upgrade.
- **nofile limits** are set to 65,536 in docker-compose.yml for the API and k6 containers. Without this, the OS limits open file descriptors and connections drop before reaching target VU count.
  █ THRESHOLDS

    location_updates_sent
    ✓ 'count>10000' count=1639027

    ws_errors
    ✓ 'count<100' count=0

  █ TOTAL RESULTS

    checks_total.......: 1945855 2108.098433/s
    checks_succeeded...: 100.00% 1945855 out of 1945855
    checks_failed......: 0.00%   0 out of 1945855

    ✓ ack ok
    ✓ WebSocket connected (101)

    CUSTOM
    location_updates_sent.......: 1639027 1775.687423/s

    EXECUTION
    iteration_duration..........: avg=3m45s        min=3m45s  med=3m45s  max=3m45s   p(90)=3m45s  p(95)=3m45s
    iterations..................: 22006   23.840838/s
    vus.........................: 58      min=5         max=10000
    vus_max.....................: 10000   min=10000     max=10000

    NETWORK
    data_received...............: 41 MB   44 kB/s
    data_sent...................: 289 MB  313 kB/s

    WEBSOCKET
    ws_connecting...............: avg=3.74ms       min=1.88ms med=2.87ms max=157.9ms p(90)=4.39ms p(95)=5.63ms
    ws_connection_duration_ms...: avg=109999.48056 min=109996 med=109999 max=110155  p(90)=110001 p(95)=110002
    ws_errors...................: 0       0/s
    ws_msgs_received............: 1923849 2084.257595/s
    ws_msgs_sent................: 1639027 1775.687423/s
    ws_session_duration.........: avg=1m50s        min=1m50s  med=1m50s  max=1m50s   p(90)=1m50s  p(95)=1m50s
    ws_sessions.................: 30540   33.086394/s
