# Load Test Results

All tests run on a Windows laptop via Docker Desktop.  
Stack: Axum backend + Redis Cluster + PostgreSQL — entire stack in containers.  
Tool: [k6](https://k6.io/)

---

## Test Conditions

| Parameter | Value |
|---|---|
| Max VUs | 10,000 |
| Duration | 15 minutes |
| Driver update interval | Every 2 seconds |
| Thresholds | location_updates_sent > 10,000 AND ws_errors < 100 |
| Hardware | Windows laptop, Docker Desktop, under 1 CPU core, under 2GB RAM |

---

## Test 1 — Baseline (21 April 2026)

![Baseline Load Test](../assets/singlenode_k6_results_c10k.png)

First successful 10k VU run with redis node not cluster. Used to verify the system held under sustained load.

```
THRESHOLDS
location_updates_sent  ✓ count=1638827
ws_errors              ✓ count=0

TOTAL RESULTS
checks_total.......: 1945770   2108.000785/s
checks_succeeded...: 100.00%   1945770 out of 1945770
checks_failed......: 0.00%     0 out of 1945770

✓ ack ok
✓ WebSocket connected (101)

CUSTOM
location_updates_sent: 1638827   1775.466063/s

WEBSOCKET
ws_connecting......: avg=3.63ms   min=1.87ms   med=2.83ms   max=175.2ms   p(90)=4.12ms   p(95)=5.12ms
ws_errors..........: 0            0/s
ws_msgs_received...: 1923764      2084.16001/s
ws_msgs_sent.......: 1638827      1775.466063/s
ws_sessions........: 30540        33.086307/s
```

**Result: 100% checks passed. Zero errors. Clean baseline.**



---

## Test 2 — Memory Stability Check (26 April 2026)

Run immediately after Test 1 to check if RAM climbed or leaked under consecutive load.  
If memory was leaking, this test would show degraded performance or errors.

![Second Redis Cluster Load Test](../assets/second_test_rediscluster_back_to_back.png)

```
THRESHOLDS
location_updates_sent  ✓ count=1639246
ws_errors              ✓ count=0

TOTAL RESULTS
checks_total.......: 1943042   2105.09341/s
checks_succeeded...: 99.99%    1943041 out of 1943042
checks_failed......: 0.00%     1 out of 1943042

✓ ack ok
✗ WebSocket connected (101)  — 99% → 22005 / X 1

CUSTOM
location_updates_sent: 1639246   1775.960557/s

WEBSOCKET
ws_connecting......: avg=3.28ms   min=1.81ms   med=2.77ms   max=104.63ms   p(90)=4.14ms   p(95)=5.14ms
ws_errors..........: 0            0/s
ws_msgs_received...: 1921036      2081.25209/s
ws_msgs_sent.......: 1639246      1775.960557/s
ws_sessions........: 30540        33.087063/s
```

**Result: 1 failed check out of 1,943,042. ws_errors still zero.  
No memory leak. Performance identical to Test 1.**

---

## Test 3 — Churn Load (26 April 2026)

Same 10k VUs but with connection churn — clients disconnecting and reconnecting  
throughout the test to simulate real-world driver app behaviour.

![First Redis Clusster Load Test](../assets/first_test_redis_cluster_k6_results.png)

```
THRESHOLDS
location_updates_sent  ✓ count=1639369
ws_errors              ✓ count=0

TOTAL RESULTS
checks_total.......: 1943098   2105.135958/s
checks_succeeded...: 99.99%    1943097 out of 1943098
checks_failed......: 0.00%     1 out of 1943098

✓ ack ok
✗ WebSocket connected (101)  — 99% → 22004 / X 1

CUSTOM
location_updates_sent: 1639369   1776.078527/s

WEBSOCKET
ws_connecting......: avg=4.34ms   min=1.83ms   med=2.83ms   max=30s   p(90)=4.37ms   p(95)=5.49ms
ws_errors..........: 0            0/s
ws_msgs_received...: 1921093      2081.295927/s
ws_msgs_sent.......: 1639369      1776.078527/s
ws_sessions........: 30540        33.086778/s
```

**Result: Zero ws_errors under churn. 1 failed check out of 1.9M.  
System handles reconnection storms without degradation.**

---

## Test 4 — Chaos Test: Redis Node Killed Mid-Test (26 April 2026)

Redis cluster node manually stopped during a live 10k VU run.  
Purpose: verify FuturesUnordered fault tolerance and cluster failover behaviour.

![Manual Failure Load Test](../assets/test_with_redis_node_failure_k6_result.png)

```
THRESHOLDS
location_updates_sent  ✓ count=1632385
ws_errors              ✓ count=0

TOTAL RESULTS
checks_total.......: 1935573   2081.347589/s
checks_succeeded...: 99.99%    1935405 out of 1935573
checks_failed......: 0.00%     168 out of 1935573

✗ ack ok                      — 99% → 1913430 / X 17
✗ WebSocket connected (101)   — 99% → 21975 / X 151

CUSTOM
location_updates_sent: 1632385   1755.325469/s

WEBSOCKET
ws_connecting......: avg=340.1ms   min=1.85ms   med=2.93ms   max=30s   p(90)=5.21ms   p(95)=7.99ms
ws_errors..........: 0             0/s
ws_msgs_received...: 1913447       2057.555204/s
ws_msgs_sent.......: 1632385       1755.325469/s
ws_sessions........: 30570         32.872331/s
```

**What happened:**
- Redis node killed mid-test
- ws_connecting spiked from avg 4ms → 340ms during failover window
- 168 checks failed out of 1,935,573 — all during the failover moment
- ws_errors remained zero throughout
- System recovered and continued processing without restart

**Conclusion: Maximum data loss window during node failure: ~30 seconds by design.  
FuturesUnordered continued polling remaining nodes. No cascading failure.**

---

## Summary

| Test | VUs | ws_errors | Checks Passed | Avg Connect |
|---|---|---|---|---|
| Baseline | 10,000 | 0 | 100.00% | 3.63ms |
| Memory Stability | 10,000 | 0 | 99.99% | 3.28ms |
| Churn Load | 10,000 | 0 | 99.99% | 4.34ms |
| Chaos (node killed) | 10,000 | 0 | 99.99% | 340.1ms → recovered |

Zero WebSocket errors across all four tests.  
System survived a Redis node failure mid-test with no ws_errors and automatic recovery.


## Test 5 — Full End-to-End (Drivers + Customers simultaneous)

14,000 VUs across two scenarios. Validates both throughput 
and correctness — customers verified to receive updates 
from their correct driver only.

![Customer and Driver test](../assets/customer_and_driver_load_test.png)


 █ THRESHOLDS

    location_updates_received{scenario:customers}
    ✓ 'count>10000' count=36114

    location_updates_sent{scenario:drivers}
    ✓ 'count>10000' count=1178210

    ws_connecting
    ✓ 'p(95)<30' p(95)=9.7ms

    ws_errors{scenario:customers}
    ✓ 'count<100' count=0

    ws_errors{scenario:drivers}
    ✓ 'count<100' count=0


  █ TOTAL RESULTS

    checks_total.......: 1498616 1627.195415/s
    checks_succeeded...: 99.99%  1498605 out of 1498616
    checks_failed......: 0.00%   11 out of 1498616

    ✓ ack ok
    ✓ is valid location update
    ✓ correct driver id
    ✗ WebSocket connected (101)
      ↳  99% — ✓ 45670 / ✗ 11

    CUSTOM
    location_updates_received...: 36114   39.212537/s
      { scenario:customers }....: 36114   39.212537/s
    location_updates_sent.......: 1178210 1279.298973/s
      { scenario:drivers }......: 1178210 1279.298973/s

    EXECUTION
    iteration_duration..........: avg=2m36s         min=1.06ms   med=2m0s   max=3m45s  p(90)=3m45s  p(95)=3m45s
    iterations..................: 45681   49.600374/s
    vus.........................: 42      min=0         max=14000
    vus_max.....................: 14000   min=10936     max=14000

    NETWORK
    data_received...............: 41 MB   44 kB/s
    data_sent...................: 235 MB  255 kB/s

    WEBSOCKET
    ws_connecting...............: avg=10.46ms       min=868.96µs med=3.89ms max=30s    p(90)=7.1ms  p(95)=9.7ms
    ws_connection_duration_ms...: avg=115984.830757 min=109993   med=119996 max=135638 p(90)=120000 p(95)=120001
    ws_errors...................: 0       0/s
      { scenario:customers }....: 0       0/s
      { scenario:drivers }......: 0       0/s
    ws_msgs_received............: 1416822 1538.38359/s
    ws_msgs_sent................: 1308976 1421.284536/s
    ws_session_duration.........: avg=1m55s         min=913.52µs med=2m0s   max=2m15s  p(90)=2m0s   p(95)=2m0s
    ws_sessions.................: 56882   61.762406/s




running (15m21.0s), 00000/14000 VUs, 45680 complete and 11202 interrupted iterations
drivers   ✓ [======================================] 0000/7000 VUs  15m0s
customers ✓ [======================================] 0000/7000 VUs  14m0s
