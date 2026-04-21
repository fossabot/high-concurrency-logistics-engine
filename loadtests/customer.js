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
const locationUpdatesReceived = new Counter("location_updates_received");
const connectionDuration = new Trend("ws_connection_duration_ms");

// ─── Config ───────────────────────────────────────────────────────────────────
const BASE_URL = __ENV.BASE_URL || "ws://localhost:8080";

// Bangalore bounding box
const START_LAT = 12.9716;
const START_LNG = 77.5946;

// ─── Stages: ramp to 5000 VU ──────────────────────────────────────────────────
export const options = {
  stages: [
    { duration: "30s", target: 1000 }, // warm up
    { duration: "60s", target: 2000 }, // ramp
    { duration: "60s", target: 3000 }, // build
    { duration: "60s", target: 3000 }, // push
    { duration: "60s", target: 3000 }, // slow final push
    { duration: "60s", target: 1500 }, // hold at peak
    { duration: "30s", target: 0 }, // cool down
  ],
  thresholds: {
    ws_errors: ["count<100"],
    location_updates_received: ["count>10000"],
  },
};

// ─── Main VU ──────────────────────────────────────────────────────────────────
export default function () {
  const vuId = __VU;
  const parcelId = `parcel-${(vuId % 1000) + 1}`;
  let tick = 0;
  // Pick pre-generated Ed25519 token — signed with same key as axum API
  const token = tokens[(vuId - 1) % tokens.length];

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
        // 1. Randomize the first ping (JITTER)
        // This prevents 10,000 users from pinging at the exact same millisecond
        const initialDelay = Math.random() * 20;
        console.log(`Initial delay: ${initialDelay}`);
        socket.setInterval(
          function () {
            socket.send(JSON.stringify({ type: "ping" }));
          },
          25000 + Math.random() * 5000,
        ); // Ping every 25-30 seconds
      });
      socket.on("message", function (data) {
        console.log(`Raw message from server: ${data}`); // <--- ADD THIS
        try {
          const msg = JSON.parse(data);
          check(msg, {
            "is valid location update": (m) =>
              m.latitude !== undefined && m.longitude !== undefined,
            "correct driver id": (m) => m.driver_id !== undefined,
          });
          locationUpdatesReceived.add(1);
        } catch (e) {
          console.error(`Failed to parse JSON. Data was: ${data}`);
        }
      });
      socket.on("error", function (e) {
        wsErrors.add(1);
        console.error(`VU ${vuId} error: ${e.error()}`);
      });
      socket.on("close", function () {
        connectionDuration.add(Date.now() - startTime);
      });
      socket.setTimeout(function () {
        socket.close();
      }, 120000);
    },
  );

  check(res, {
    "WebSocket connected (101)": (r) => r && r.status === 101,
  });
}
