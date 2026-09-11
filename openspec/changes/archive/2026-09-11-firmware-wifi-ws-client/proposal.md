# Proposal — firmware-wifi-ws-client

> SDD Phase 1 slice: the ESP32 firmware connects to WiFi as a **station** and opens a
> **WebSocket client** to the hub at `/ws/robot?robot_id=<id>`, with automatic
> reconnection (bounded backoff) and explicit protocol keepalive.
>
> OUT of scope: motor control, camera/video, telemetry publishing, SNTP, TLS/WSS, auth.

Status: proposal (ready for `sdd-spec`).

---

## 1. Why

Today the firmware is a hello-world: it initializes the logger and exits. The robot
cannot reach the hub, so the web panel has no robot to talk to and the rest of the
roadmap (motors, camera, telemetry, AI) has no transport to build on.

This change gives the robot its network life: it joins the LAN as a station, dials the
hub as a WebSocket client, and stays connected across WiFi drops and hub restarts. It
is the smallest slice that makes the robot *present* on the network — nothing more.

The existing C++ reference (`ejverat/esp-cam-robot-car`) hardcodes credentials and
runs the **old architecture** (robot = HTTP server, no hub). We intentionally do **not**
port that: this slice implements the hub-client model from `docs/architecture.md`.

## 2. What changes

| Area | Change |
|------|--------|
| `firmware/Cargo.toml` | Add `serde` + `serde_json`; add `[[package.metadata.esp-idf-sys.extra_components]]` for `espressif/esp_websocket_client` (v1.1.0) — **required** to compile `esp_idf_svc::ws::client` on ESP-IDF ≥ 5. |
| `firmware/src/config.rs` (new) | Runtime config resolved in order: **NVS → compiled defaults** (`config.rs` consts). If NVS is empty, compiled defaults are used and **persisted to NVS on first boot**. Keys: `wifi.ssid`, `wifi.password`, `hub.url` (host + port), `hub.robot_id`. Default `robot_id = "a1"` (matches server tests). |
| `firmware/src/wifi.rs` (new) | `BlockingWifi` station connect with a **bounded retry + backoff** loop (e.g. 1s → 2s → 4s, cap 15s) and per-attempt logging. Handles `STA_DISCONNECTED` by re-running the connect loop. |
| `firmware/src/ws_client.rs` (new) | `EspWebSocketClient` dialing `ws://<host>:<port>/ws/robot?robot_id=<id>` (URL-encode `robot_id`). Explicit `EspWebSocketClientConfig` (see §4 gotcha). Callback-style client; `Connected`/`Disconnected` events logged. |
| `firmware/src/main.rs` | Wire modules together: `link_patches()` → `EspLogger` → NVS init → config → WiFi connect loop → WS client → `loop { rx.recv() }` over an `std::sync::mpsc` channel fed by the WS callback. |
| `firmware/sdkconfig.defaults` | **No change required** (validated): the default ESP-IDF single-app partition table already includes an `nvs` partition; `esp_wifi`/`nvs_flash`/`esp_timer`/`esp_event` are on by default for esp32. The WS component is injected via Cargo metadata, not Kconfig. |
| `docs/` / `README.md` | Add a short note in the firmware section: credentials live in NVS (with compiled fallback), how to clear NVS (`erase-flash`), and the brownout/power caveat. |

No `server/` changes. No new Cargo features for WiFi (the `wifi` module is cfg-gated on
the default `esp_wifi` component, not a Cargo feature).

## 3. Impact

- **Firmware binary grows** (serde_json + `esp_websocket_client` + its transports). The
  dev/release profiles already use `opt-level = "z"`/`"s"`; expected well within flash
  budget. Verify by checking `.bin` size during apply.
- **NVS keys introduced** under the default partition. First boot writes defaults; later
  boots read them. No schema/migration to manage.
- **No server behavior change.** The hub already auto-registers robots from the query
  string and logs reconnects; the firmware simply becomes a real client.
- **Robot appears in the hub** but emits no application messages yet (no register
  envelope, no telemetry — see non-goals). Clients will see the connection only via hub
  logs / future robots list.

## 4. Key design decisions (resolved, not to be re-opened)

1. **Provisioning = NVS runtime + fallback.** SSID/password, hub URL, and `robot_id` are
   read from NVS at runtime; empty NVS falls back to compiled defaults in `config.rs` and
   persists them on first boot. (Pre-proposal decision, user-selected.)
2. **No `register` envelope.** The hub auto-registers via `?robot_id=`. The firmware
   sends no `register` message.
3. **No server greeting / no JSON ping.** The server sends nothing on connect today; the
   firmware must not block waiting for a greeting. Keepalive is **protocol-level WS ping**
   (explicit config) — no application JSON heartbeat in this slice.
4. **`ts` = omitted / 0.** No SNTP in this change; telemetry with timestamps arrives in
   phase 3.
5. **Transport = `ws://`** (`TransportOverTCP`), LAN phase 1. `wss://` + auth are phase 4.
6. **`EspWebSocketClientConfig` all-zeros gotcha (must be honored in design).**
   `#[derive(Default)]` yields **all-zeros**, which is **NOT** ESP-IDF's
   `WEBSOCKET_CONFIG_DEFAULT()` macro. The config must explicitly set at least:
   `task_prio=5`, `task_stack=4096`, `buffer_size=1024`, `ping_interval_sec=10`,
   `pingpong_timeout_sec=60–120`, `reconnect_timeout_ms=3000–5000`,
   `network_timeout_ms=10000`, `transport=TransportOverTCP`,
   `disable_auto_reconnect=false`. Otherwise keepalive and auto-reconnect silently do
   nothing.
7. **Blocking loop, not embassy async.** `BlockingWifi` + callback-style WS client are
   blocking/callback-based; `embassy-executor` is not present and not needed. Backoff uses
   plain delays.

## 5. Non-goals (explicit)

- Motor control / L298N / LEDC — unchanged, later phase.
- Camera / MJPEG video — later phase.
- Telemetry publishing (battery, RSSI, motor state) — later phase.
- SNTP / wall-clock time — later phase.
- `wss://` (TLS) and auth — phase 4.
- JSON `ping`/`pong` application heartbeat — dormant; only future-proofed, not emitted.
- `register` envelope emission.
- Modifying the brownout detector (see risks — documented, not touched).
- Static IP / mDNS — unneeded (the hub never dials the robot).

## 6. Risks (carried from explore.md) + mitigations

| Risk | Mitigation |
|------|------------|
| **Brownout reset** on ESP32-CAM WiFi TX spikes with weak USB power (the C++ reference disables the brownout detector). | **Documented, not modified** in this change (pre-proposal decision). README note: use adequate power / short cable before flashing. |
| **FreeRTOS task watchdog (TWDT)** tripping on long blocking WiFi connects. | Bounded connect loop with backoff and per-attempt yields; never block indefinitely. If TWDT still fires on-device, feed or disable the main-task watchdog as a documented, scoped follow-up (not silently). |
| **Binary size** growth from serde_json + WS component. | `opt-level` already `z`/`s`; check `.bin` size in apply; serde is needed for later phases anyway. |
| **Stale-connection detection** (half-open TCP when hub dies without FIN). | Protocol-level WS ping via explicit `ping_interval_sec`/`pingpong_timeout_sec`; `disable_pingpong_discon` stays false so dead peers are disconnected. |
| **All-zeros config silently disabling keepalive/reconnect.** | §4.6 is a hard design requirement; spec must assert each field is set explicitly. |
| **Secrets in repo.** | Credentials live in NVS with compiled fallback; `main.rs`/committed source contains no real secrets (fallback defaults are placeholders / `"a1"`). |

## 7. Rollback

- Revert the commit and re-flash previous firmware. No server, schema, or partition-table
  changes; NVS keys are additive and cleared with `erase-flash` if ever needed.

## 8. Delivery forecast

- Expected **well under the 400-line review budget** (new modules are small and
  focused; dependency change is one `Cargo.toml` block). Delivery strategy **ask-on-risk**;
  **no chaining** needed.

## 9. Acceptance criteria (inputs for `sdd-spec`)

A maintainer approves when all of the following hold:

1. `firmware/Cargo.toml` declares `serde`/`serde_json` and the
   `espressif/esp_websocket_client` (1.1.0) `extra_components` entry, and the firmware
   **cross-compiles** with `nix develop .#firmware-fhs` + `cd firmware && cargo build`.
2. `config.rs` resolves SSID/password/hub-URL/`robot_id` from NVS, falling back to
   compiled defaults, and **persists defaults to NVS on first boot** (empty NVS). No
   secrets in committed source beyond placeholder defaults.
3. WiFi connects as a station; on failure it retries with **bounded backoff** (documented
   cap, e.g. 15s) and logs each attempt; on `STA_DISCONNECTED` it re-enters the connect
   loop.
4. The WS client dials `ws://<host>:<port>/ws/robot?robot_id=<id>` with `robot_id`
   URL-encoded; transport is `TransportOverTCP` (no TLS fields configured).
5. `EspWebSocketClientConfig` sets **explicit, non-zero** values for
   `task_prio`, `task_stack`, `buffer_size`, `ping_interval_sec`,
   `pingpong_timeout_sec`, `reconnect_timeout_ms`, `network_timeout_ms`, and keeps
   `disable_auto_reconnect = false` (all-zeros default rejected).
6. The client is created **once** and auto-reconnects via the C library; `Connected` /
   `Disconnected` / `Close` events are logged via ESP log. The firmware never blocks
   waiting for a server greeting or JSON ping.
7. The firmware sends **no** `register` envelope and no application heartbeat; `ts` is
   omitted or 0.
8. `main()` stays alive (`loop` over the WS callback channel) and never busy-spins; the
   FreeRTOS task is not blocked indefinitely during connect/backoff.
9. `sdkconfig.defaults` remains unchanged (or only gains an optional explicit
   `CONFIG_ESP_WIFI_ENABLED=y` for legibility — no functional change); no new partition
   table is introduced.
10. A short README firmware note documents NVS-based credentials, the `erase-flash`
    clear path, and the brownout/power caveat.
11. **Non-goals hold:** no motor/camera/telemetry/SNTP/TLS/auth code is added.

## 10. Testing note

Firmware-only change: host-side `cargo test` is not possible (Xtensa target). Validation
is cross-compilation (criterion 1) plus on-device flashing/observation
(`cargo espflash flash`), logged WiFi/WS events as evidence. No server tests change.
