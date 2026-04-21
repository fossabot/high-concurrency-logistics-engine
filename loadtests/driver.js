import ws from "k6/ws";
import { check, sleep } from "k6";
import { Counter, Trend } from "k6/metrics";
import { SharedArray } from "k6/data";

// ─── Pre-generated Ed25519 tokens from token-gen ──────────────────────────────
const tokens = new SharedArray("driver_tokens", function () {
  return open("/loadtests/token-output.txt").trim().split("\n");
});

// ─── Custom Metrics ───────────────────────────────────────────────────────────
const wsErrors = new Counter("ws_errors");
const locationUpdatesSent = new Counter("location_updates_sent");
const connectionDuration = new Trend("ws_connection_duration_ms");

// ─── Config ───────────────────────────────────────────────────────────────────
const BASE_URL = __ENV.BASE_URL || "ws://host.docker.internal:8080";

// Bangalore bounding box
const START_LAT = 12.9716;
const START_LNG = 77.5946;

// ─── Stages: ramp to 5000 VU ──────────────────────────────────────────────────
export const options = {
  stages: [
    { duration: "2m", target: 2000 }, // Slow start
    { duration: "5m", target: 10000 }, // Gentle climb (33 conn/sec)
    { duration: "5m", target: 10000 }, // Soak test (the real stability check)
    { duration: "3m", target: 0 }, // Slow ramp down to avoid a "disconnect storm"
  ], // cool down

  thresholds: {
    ws_errors: ["count<100"],
    location_updates_sent: ["count>10000"],
  },
};

// ─── Lat/lng drift — smooth curved path per driver ────────────────────────────
function getStartPosition(vuId) {
  // Spread drivers across ~5km radius around Bangalore center
  const latOffset = (vuId % 100) * 0.0005;
  const lngOffset = Math.floor(vuId / 100) * 0.0005;
  return {
    lat: START_LAT + latOffset,
    lng: START_LNG + lngOffset,
  };
}

function nextPosition(lat, lng, tick) {
  // sin/cos gives smooth direction change — simulates driving a curved route
  const speed = 0.00005; // ~5m per 2s tick
  const angle = tick * 0.1; // direction rotates slowly
  return {
    lat: lat + speed * Math.sin(angle),
    lng: lng + speed * Math.cos(angle),
  };
}

// ─── Main VU ──────────────────────────────────────────────────────────────────
export default function () {
  const vuId = __VU;
  const driverId = `driver-${vuId}`;
  const parcelId = `parcel-${(vuId % 1000) + 1}`;

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
      }, 110000); // 110 seconds (slightly less than your 120s server timeout)

      socket.on("close", function () {
        connectionDuration.add(Date.now() - startTime);
      });
    },
  );
  sleep(115);
  check(res, {
    "WebSocket connected (101)": (r) => r.status === 101,
  });
}
