# Architecture Decisions

## Ed25519 over RSA for jwt signing
RSA required higher CPU processing than Ed25519 with more time. At 
high Websocket Connection requests every verification adds up to 
more CPU power. So I used Ed25519 as it provides same security 
with lower data footprint.

## Redis Lua script for deduplication
Needed atomic compare and write to the stream only if the location 
was changed. Also, reduces time needed for round trips for the 
operation if not used Lua scripts and also prevent data race 
condition at high load. Lua scripts directly run at the nodes of 
the server thereby reducing 2 round trips to a single round trip.

## At Fixed Interval or under Heavy Load Requests, making batch
## windows for postgres writes
Writing every Websocket data to postgres killed my backend 
performance immediately as too many round trips causing heavy 
network IO and causing postgres connection time to increase.
But by batching at most 1000 entries or waiting 20 second, 
the batch of entries can be carried out in a single connection 
through UNNEST while keeping location data fresh enough
for parcel tracking.
Maximum acceptable data loss window: 30 seconds by design.

## FuturesUnordered for Redis cluster
Needed to poll multiple Redis nodes concurrently without spawning 
a task per node. FuturesUnordered drives all futures concurrently 
and yields results as they complete. If one node fails, others 
continue — no blocking, no cascading failure which doesnt make 
other users data to be stored without interuption.

## MPSC channel for batch collection
All WebSocket handlers connections send location updates to a 
single receiver via MPSC which helps in maintaining the
active connections that need to send data to the Postgres.
This decouples ingestion from persistence — handlers never 
block waiting on Postgres,and flushes on interval of 20s or size 
limit exceeds 1000.
