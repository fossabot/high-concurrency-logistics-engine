# Architecture Overview

## System Design

Real-time parcel tracking system where drivers send GPS updates 
every 2 seconds and customers receive live location tracking.

## Data Flow

Driver App
↓ WebSocket (GPS update every 2s)
Axum Backend
↓
Redis Lua Script (deduplication)
→ Skip if location unchanged
→ Write to Redis Stream if changed
↓
MPSC Channel (batch collector)
→ Flush every 20s or 1000 entries
↓
Postgres (unnest bulk insert)

Customer App
↓ WebSocket (GPS update every 2s)
Axum Backend
↓
Subcribe to Redis
↓
DashMap
↓
LocationUpdate to Customer

## Failure Tolerance
- Redis node fails → FuturesUnordered continues with remaining nodes
- Maximum data loss window: 30 seconds by design
- No cascading failure between nodes
- limiting Subcriber limit to one for each unique ID 


## Observability
- Prometheus scraping custom metrics via /metrics
- Grafana dashboards for custom errors and request tracking
- k6 load testing for validation
