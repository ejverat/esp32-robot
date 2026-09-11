# Project Context — esp32-robot

> SDD project context. Owned by the init phase; kept stable and updated only when
> architecture decisions actually change. Technical artifact (English), per project
> conventions. Complements `openspec/config.yaml`.

## Purpose

Monorepo in Rust to control a 4WD robot car (kit Keyestudio KS5024: ESP32-CAM
AI-Thinker + OV2640 camera, L298N motor driver) from a web panel: remote control,
MJPEG video streaming, and telemetry. Future goal: autonomous driving via a
server-side AI agent.

## Architecture (hub model)

- Central Rust server (`server/`) is the hub: Axum + Tokio, WebSocket relay keyed by
  `robot_id`.
- The ESP32 firmware (`firmware/`) connects to the server as a WebSocket **client**;
  the server never initiates connections to robots.
- Browsers (and the future AI agent) connect to the server as WebSocket **clients**.
- The future AI agent is just another hub client: it consumes video + telemetry and
  publishes movement commands.

## Stack

| Component | Technology |
|-----------|-----------|
| Server | axum 0.8 + tokio + tokio-tungstenite, serde/serde_json, rustls (later) |
| Firmware | esp-rs + esp-idf-svc 0.52.1, ESP-IDF v5.5.3, target `xtensa-esp32-espidf`, toolchain channel `esp` |
| Web (future) | Leptos → WASM |
| AI (future) | ort (ONNX) or candle, server-side |

## Repository layout

```
server/       central hub (functional: hub, message envelope, ws handlers, tests)
firmware/     ESP32 firmware (hello-world; cross-compiled)
docs/         architecture.md (protocols, envelope v0, roadmap 0-6)
web/          (future) web panel
ai/           (future) AI agent
```

## Message envelope (v0)

JSON envelope with `type`, `id`, `ts`, `robot_id`, `payload`. Types: `cmd:move`,
`cmd:stop`, `cmd:camera`, `telemetry`, `video` (binary JPEG), `config`, `ping`,
`pong`, `register`, `robots:list`, `error`. The hub relays by `robot_id` and does not
interpret `payload`. Full spec: `docs/architecture.md` and `server/src/message.rs`.

## Roadmap

| Phase | Scope | Status |
|-------|-------|--------|
| 0 | Validate hardware + open decisions | done (README §8) |
| 1 | Motors + WebSocket; hub; basic web control | MVP — in progress |
| 2 | MJPEG camera + web viewer | planned |
| 3 | Full telemetry + persistence | planned |
| 4 | Security (WSS + auth) | planned |
| 5 | AI object detection | planned |
| 6 | Autonomous planner | planned |

## Testing strategy

- **Strict TDD** for server-side and host-testable code: `cd server && cargo test`
  (unit tests in `hub.rs`/`message.rs` + e2e WebSocket test in `main.rs` via
  tokio-tungstenite). Requires `nix develop .#server`.
- Firmware is cross-compiled for Xtensa; host-side `cargo test` is **not** possible.
  Firmware changes are validated by `cd firmware && cargo build` (inside
  `nix develop .#firmware-fhs`) plus on-device flashing (`cargo espflash flash`).

## Conventions

- Technical artifacts (code, SDD/OpenSpec): English.
- User-facing docs (`README.md`, `docs/architecture.md`): currently Spanish.
- Commits: Spanish conventional style (e.g. "Docs:", "Infra:", "Implementar ...").
- Existing Rust code comments: Spanish.

## Closed hardware decisions

Kit Keyestudio KS5024 (4WD Camera Robot Car), validated with the reference C++
firmware in `ejverat/esp-cam-robot-car`. ESP32-CAM AI-Thinker + OV2640; L298N
on-board (2 PWM channels LEDC: GPIO 12/13 left, 14/15 right; LED flash GPIO 4). No
IMU/encoders on the kit → telemetry limited to battery (ADC), WiFi RSSI, and motor
state. Video target: MJPEG VGA 640×480, JPEG q10, ~10–15 FPS, `fb_count=2` with
PSRAM; WebRTC discarded for this hardware. Full details: `README.md` §8.
