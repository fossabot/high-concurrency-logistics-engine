import ws from "k6/ws";
import { check, sleep } from "k6";
import { Counter, Trend } from "k6/metrics";
import { SharedArray } from "k6/data";

// ─── Pre-generated Ed25519 tokens from token-gen ──────────────────────────────
const tokens = new SharedArray("driver_tokens", function () {
  return open("./token-output.txt").trim().split("\n");
});

// ─── Custom Metrics ───────────────────────────────────────────────────────────
const wsErrors = new Counter("ws_errors");
const locationUpdatesSent = new Counter("location_updates_sent");
const connectionDuration = new Trend("ws_connection_duration_ms");
const locationUpdatesReceived = new Counter("location_updates_received");

// ─── Config ───────────────────────────────────────────────────────────────────
const BASE_URL = __ENV.BASE_URL;

// Bangalore bounding box
const START_LAT = 12.9716;
const START_LNG = 77.5946;
const PARCEL_COUNT = 10000;
// ─── Stages: ramp to 10000 VU ──────────────────────────────────────────────────
export const options = {
  scenarios: {
    drivers: {
      executor: "ramping-vus",
      startVUs: 0,
      stages: [
        { duration: "4m", target: 10000 }, // gentle start
        { duration: "10m", target: 10000 }, // soak
        { duration: "4m", target: 0 }, // ramp down
      ], // cool down
      gracefulStop: "245s", // Higher than your 240s iteration time
      gracefulRampDown: "245s",
      exec: "driver_logic",
    },
    customers: {
      executor: "ramping-vus",
      startVUs: 0,
      startTime: "245s",
      stages: [
        { duration: "4m", target: 10000 },
        { duration: "2m", target: 10000 },
        { duration: "4m", target: 0 },
      ], // cool down
      gracefulStop: "245s", // Higher than your 240s iteration time
      gracefulRampDown: "245s",
      exec: "customer_logic",
    },
  },
  thresholds: {
    "ws_errors{scenario:drivers}": ["count<100"],
    "location_updates_sent{scenario:drivers}": ["count>10000"],

    "ws_errors{scenario:customers}": ["count<100"],
    "location_updates_received{scenario:customers}": ["count>10000"],

    ws_connecting: ["p(95)<30", "p(99)<100"],
  },
};

function getStartPosition(vuId) {
  // Spread drivers across ~5km radius around Bangalore center
  const latOffset = (vuId % 100) * 0.0005;
  const lngOffset = Math.floor(vuId / 100) * 0.0005;
  return {
    lat: Number((START_LAT + latOffset).toFixed(6)),
    lng: Number((START_LNG + lngOffset).toFixed(6)),
  };
  summaryTrendStats: [
    'avg',
    'min',
    'med',
    'max',
    'p(90)',
    'p(95)',
    'p(99)'
  ],
}
// ─── Lat/lng drift — smooth curved path per driver ────────────────────────────

function nextPosition(lat, lng, tick) {
  // sin/cos gives smooth direction change — simulates driving a curved route
  const speed = 0.00005; // ~5m per 2s tick
  const angle = tick * 0.1; // direction rotates slowly
  return {
    lat: Number((lat + speed * Math.sin(angle)).toFixed(6)),
    lng: Number((lng + speed * Math.cos(angle)).toFixed(6)),
  };
}

// ─── Main VU ──────────────────────────────────────────────────────────────────
export function driver_logic() {
  const vuId = __VU;
  const driverId = `driver-${vuId}`;
  const parcelId = `parcel-${(vuId % PARCEL_COUNT) + 1}`;

  // Pick pre-generated Ed25519 token — signed with same key as axum API
  const token = tokens[(vuId - 1) % tokens.length];

  let position = getStartPosition(vuId);
  let tick = 0;
  const startTime = Date.now();

  const res = ws.connect(
    `${BASE_URL}/ws?parcel_id=${parcelId}&role=driver`,
    {
      headers: {
        Cookie: `token=${token}`,
        Authorization: `Bearer ${token}`,
      },
    },
    function (socket) {
      socket.on("open", function () {
        // Send initial position on connect
        socket.send(
          JSON.stringify({
            parcel_id: parcelId,
            driver_id: driverId,
            timestamp: Math.floor(Date.now() / 1000),
            latitude: position.lat,
            longitude: position.lng,
            status: "picked_up",
          }),
        );
        locationUpdatesSent.add(1);

        // Update every 2s — matches your Redis Stream 2s resolution
        socket.setInterval(function () {
          tick++;
          position = nextPosition(position.lat, position.lng, tick);
          socket.send(
            JSON.stringify({
              parcel_id: parcelId,
              driver_id: driverId,
              timestamp: Math.floor(Date.now() / 1000),
              latitude: position.lat,
              longitude: position.lng,
              status: "picked_up",
            }),
          );
          locationUpdatesSent.add(1);
        }, 2000);
      });

      socket.on("message", function (data) {
        try {
          const msg = JSON.parse(data);
          check(msg, {
            "ack ok": (m) => m.status === "ok" || m.status === "stream",
          });
        } catch (e) {
          console.error(`Failed to parse JSON. Data was: ${data}`);
        }
      });
      socket.on("error", function (e) {
        wsErrors.add(1);
        console.error(`VU ${vuId} error: ${e.error()}`);
      });
      socket.on("ping", function () {
        socket.pong(); // explicitly send pong back
      });
      // This ensures the VU doesn't die and the setInterval actually runs
      socket.setTimeout(function () {
        socket.close();
      }, 180000); // 110 seconds (slightly less than your 120s server timeout)

      socket.on("close", function () {
        connectionDuration.add(Date.now() - startTime);
      });
    },
  );
  sleep(60);
  check(res, {
    "WebSocket connected (101)": (r) => r.status === 101,
  });
}

// ─── Main Second VU ──────────────────────────────────────────────────────────────────
export function customer_logic() {
  const vuId = __VU;
  const parcelId = `parcel-${(vuId % PARCEL_COUNT) + 1}`;

  // Token logic remains same...
  const token = tokens[(vuId - 1) % tokens.length];
  const startTime = Date.now();

  const res = ws.connect(
    `${BASE_URL}/customer?parcel_id=${parcelId}&role=customer`,
    {
      headers: {
        Cookie: `token=${token}`,
        Authorization: `Bearer ${token}`,
      },
    },
    function (socket) {
      socket.on("open", function () {
        // 1. FIX: Actually use the Jitter
        // We use setTimeout to delay the FIRST ping, desynchronizing the fleet.
        const initialDelay = Math.random() * 20000; // 0-20 seconds delay

        socket.setTimeout(function () {
          // Send first ping
          socket.send(JSON.stringify({ type: "ping" }));

          // Start regular interval AFTER the delay
          socket.setInterval(function () {
            socket.send(JSON.stringify({ type: "ping" }));
          }, 30000); // Ping every 30s
        }, initialDelay);
      });

      socket.on("message", function (data) {
        // 2. FIX: REMOVED console.log
        // Only log errors, never success data in a load test.
        try {
          const msg = JSON.parse(data);

          // Lightweight checks
          if (msg.latitude && msg.longitude) {
            locationUpdatesReceived.add(1);
          }
          check(msg, {
            "correct parcel id": (m) => m.parcel_id === parcelId,
          });
          // Optional: detailed check (Costly at 10k users, use sparingly)
          // check(msg, { ... });
        } catch (e) {
          // Keep this error log, it's rare and important
          console.error(`Parse Error: ${e}`);
        }
      });
      socket.setTimeout(function () {
        socket.close();
      }, 120000);
      socket.on("error", function (e) {
        wsErrors.add(1);
        // Only log the first few errors to avoid console flood
        if (__ITER < 10) {
          console.error(`VU ${vuId} error: ${e.error()}`);
        }
      });
    },
  );

  check(res, {
    "WebSocket connected (101)": (r) => r && r.status === 101,
  });
}
