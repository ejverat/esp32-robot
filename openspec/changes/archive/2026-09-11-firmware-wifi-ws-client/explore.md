# Explore — firmware-wifi-ws-client

> SDD Phase 1 slice: ESP32 firmware connects to WiFi as a station, then opens a
> WebSocket **client** to the hub at `ws://<server>/ws/robot?robot_id=<id>`, with
> automatic reconnection and ping/pong handling.
> OUT of scope: motor control, camera, telemetry publishing.

Status: exploration complete. This is context/notes only; no implementation.

---

## 1. Firmware state (today vs. required)

### 1.1 Current state

- `firmware/Cargo.toml`: `esp-idf-svc 0.52.1` with features `critical-section`,
  `embassy-time-driver`, `embassy-sync`; `embassy-time 0.5` (feature
  `generic-queue-8`); `log 0.4`; build-dep `embuild 0.33`.
- `firmware/src/main.rs`: hello-world (`link_patches()` +
  `EspLogger::initialize_default()` + `log::info!`).
- `firmware/sdkconfig.defaults`: only stack sizes (main task 8192, system event
  task 4096, idle 4096, pthread 4096). No WiFi / NVS / WS options.
- `firmware/rust-toolchain.toml`: channel `esp`.
- `firmware/.cargo/config.toml`: `MCU=esp32`, `ESP_IDF_VERSION=v5.5.3`, target
  `xtensa-esp32-espidf`, `build-std = ["std","panic_abort"]`, `espidf_time64`.
- `firmware/build.rs`: `embuild::espidf::sysenv::output()`.
- `Cargo.lock` (relevant): `esp-idf-svc 0.52.1`, `esp-idf-sys 0.37.2`,
  `esp-idf-hal 0.46.2`, `embedded-svc 0.29.0`, `embedded-io 0.6.1/0.7.1`,
  `embassy-sync 0.7.2`, `embassy-time 0.5.1`.

### 1.2 Feature gating (verified against the vendored esp-idf-svc 0.52.1 source)

- **There is no `wifi` Cargo feature.** The `wifi` module is gated by compile-time
  cfgs (see `src/lib.rs:110-120`):
  `feature = "alloc"` (implied by default `std`) AND
  `esp_idf_comp_esp_wifi_enabled` (the `esp_wifi` component, enabled by default for
  the esp32 target) AND `esp_idf_comp_esp_event_enabled` (default).
  ⇒ **WiFi requires no new Cargo feature**; it is available once `esp_wifi` is in
  the build (default for `MCU=esp32`).
- **The `ws::client` module is gated** (see `src/ws.rs`) on: `feature = "alloc"` +
  `esp_idf_comp_tcp_transport_enabled` + `esp_idf_comp_esp_tls_enabled` +, for
  ESP-IDF ≥ 5, `esp_idf_comp_espressif__esp_websocket_client_enabled`.
  The last cfg is **not enabled by default**: on ESP-IDF 5.x the WebSocket client
  is an *external managed component*. The `http_ws_client` example states it
  explicitly: the example **panics at runtime on ESP-IDF ≥ 5** unless you add to
  `Cargo.toml`:
  ```toml
  [[package.metadata.esp-idf-sys.extra_components]]
  remote_component = { name = "espressif/esp_websocket_client", version = "1.1.0" }
  ```
  ⇒ **This extra-component entry is the single required dependency change.**
  (Confirmed the `extra_components.remote_component` mechanism exists in
  esp-idf-sys 0.37.2 `BUILD-OPTIONS.md`.)
- The `ws.rs` module doc "add `CONFIG_HTTPD_WS_SUPPORT=y`" is for the **httpd
  server** WebSocket, **not** the client. It is not needed for this change.
- Missing direct deps to add to `firmware/Cargo.toml`:
  - `serde` + `serde_json` (build/parse the JSON envelope). Firmware is `std`, so
    stock `serde_json` is fine. (Phase 1 could hand-roll the two tiny strings
    `register`/`pong`, but serde is needed for telemetry/commands later — adding it
    now is reasonable.)
  - Optional: `anyhow` (esp-idf-svc examples use it for `main() -> anyhow::Result`).

### 1.3 sdkconfig.defaults

- **No mandatory change.** `esp_wifi`, `nvs_flash`, `esp_timer`, `esp_event` are
  enabled by default for esp32; the default ESP-IDF single-app partition table
  already includes an `nvs` partition (needed by `EspWifi::new(..., Some(nvs))` and
  optional credential storage).
- The `esp_websocket_client` component is injected via Cargo metadata, not Kconfig.
- Optional (robustness/legibility only): `CONFIG_ESP_WIFI_ENABLED=y` for
  explicitness; leave stacks as-is (main task already 8192).

### 1.4 Present vs. missing trait crates

- Present transitively: `embedded-svc 0.29.0` (wifi traits, `ws::Sender`/`FrameType`,
  `ws::Final`/`Fragmented`), `embedded-io`, `embassy-sync`, `embassy-time`.
- Missing direct deps: `serde`, `serde_json` (and optionally `anyhow`).
- `embassy-executor` is **not** present and is **not** needed for this slice (the
  ws client and `BlockingWifi` are blocking/callback-based, not async).

---

## 2. Hub contract requirements (verified in `server/src/*`)

### 2.1 Handshake

- `GET /ws/robot?robot_id=<id>` (`server/src/ws.rs`). Empty/missing `robot_id` →
  HTTP 400. Full URI the firmware must dial: `ws://<host>:8080/ws/robot?robot_id=<id>`.
- `GET /ws/client` is for browsers/AI; irrelevant to the firmware.

### 2.2 What the server does on robot connect

- `robot_session` sends `HubCommand::RegisterRobot { robot_id, tx }` **immediately**
  on upgrade (no greeting, no ack). Reconnection **replaces** the previous
  registration (hub logs "reconectado (reemplazado)").
- Robot→server **Text** frames are forwarded **verbatim** to all clients
  (`FromRobot` broadcast). **Binary** frames are passthrough for video (out of scope
  this change).
- Client→robot commands arrive at the robot as raw JSON **text** (server only
  extracts `robot_id` to route; it does not parse/rewrite the envelope).
- The server currently sends **nothing** to the robot on connect; the robot must
  not block waiting for a server greeting.

### 2.3 JSON envelope fields the firmware should emit

`Message` in `server/src/message.rs`:

| Field | Required? | Notes |
|-------|-----------|-------|
| `type` | yes | `MessageType` enum; relevant here: `register`, `pong` (later `telemetry`). |
| `robot_id` | no (optional) | Robot **should** set it so broadcast/clients can attribute messages. |
| `ts` | no | u64 **millis**. Server uses wall-clock; ESP32 has no RTC → omit or use monotonic for MVP (SNTP later). |
| `id` | no | Optional message id. |
| `payload` | default `null` | Free-form; for `register` could carry `{ name, fw_version }`. |

Minimal register message:
`{"type":"register","robot_id":"a1","payload":{"name":"esp32-robot","fw":"0.1.0"}}`.

Note: server auto-registers via the query string, so a `register` envelope is **not
required for routing** — it would be a broadcast so the web panel/AI learns robot
metadata. Whether to send it is an open question (below).

### 2.4 ping/pong semantics (important)

- **JSON-level**: `MessageType::Ping`/`Pong` (`"ping"`/`"pong"`) exist in the enum,
  but **no code in the server currently emits `ping`**. So JSON ping/pong is
  dormant. The firmware can cheaply reply `{"type":"pong"}` to a future
  `{"type":"ping"}` — future-proofing only.
- **Protocol-level**: handled **automatically by the C library**
  (`esp_websocket_client`), configurable via `EspWebSocketClientConfig`
  (`ping_interval_sec`, `pingpong_timeout_sec`, `disable_pingpong_discon`).
  **`EspWebSocketClient::send` PANICS on `FrameType::Ping`/`Pong`** — the firmware
  cannot send protocol pings manually and must rely on the C-lib config.

---

## 3. esp-idf-svc 0.52.x ws::client API shape (verified)

Path: `esp_idf_svc::ws::client::{EspWebSocketClient, EspWebSocketClientConfig,
FrameType, WebSocketEvent, WebSocketEventType}`.

- Construction (callback style): `EspWebSocketClient::new(uri, &config, timeout,
  move |event: &Result<WebSocketEvent, EspIOError>| { ... })`. Callback is
  `Send + 'static`, runs on a hidden ESP-IDF task.
- Alternative (pull style): `EspWebSocketClient::new_with_conn(uri, &config,
  timeout) -> (client, EspWebSocketConnection)`; `connection.next()` blocks for the
  next event.
- `client.send(FrameType::Text(false), bytes)` sends text; `Binary(false)` for
  binary; Ping/Pong/Close/Continue **panic**.
- `client.is_connected() -> bool`.
- Events: `BeforeConnect`, `Connected`, `Disconnected`, `Close(reason)`, `Closed`,
  `Text(&str)`, `Binary(&[u8])`, `Ping`, `Pong`. `WEBSOCKET_EVENT_ERROR` surfaces as
  `Err` in the callback.
- `EspWebSocketClientConfig` fields (relevant): `disable_auto_reconnect` (**default
  false = auto-reconnect ON**), `reconnect_timeout_ms`, `network_timeout_ms`,
  `ping_interval_sec`, `pingpong_timeout_sec`, `disable_pingpong_discon`,
  `transport` (`TransportOverTCP` for `ws://`), `task_prio`, `task_stack`,
  `buffer_size`, `user_agent`, `headers`, TLS fields (unused for `ws://`).

**Gotcha (must fix in design):** `EspWebSocketClientConfig` is `#[derive(Default)]`,
so `..Default::default()` yields **all-zeros** — this is **NOT** the ESP-IDF
`WEBSOCKET_CONFIG_DEFAULT()` macro values (which set `ping_interval_sec=10`,
`pingpong_timeout_sec=120`, `network_timeout_ms=10000`, `task_prio=5`,
`task_stack=4096`, `buffer_size=1024`). The firmware must explicitly set:
`task_prio` (e.g. 5), `task_stack` (e.g. 4096), `buffer_size` (e.g. 1024),
`ping_interval_sec` (e.g. 10s), `pingpong_timeout_sec` (e.g. 60–120s),
`reconnect_timeout_ms` (e.g. 3000–5000), `network_timeout_ms` (e.g. 10000),
`transport = TransportOverTCP`, and keep `disable_auto_reconnect = false`.

---

## 4. Credentials strategy (SSID/password, robot_id, server URI)

Reference C++ (`src/main.cpp`) **hardcodes** SSID/password in `main.cpp` and uses a
blocking `while (WiFi.status() != WL_CONNECTED) { delay(1000); }`. It is also the
**old architecture** (robot runs an HTTP server + MJPEG on port 80, no hub), so
`WebServer.cpp` is **not** to be replicated.

Options:

1. **Hardcoded consts** — matches C++, zero effort, but commits secrets (bad).
2. **Compile-time `env!("WIFI_SSID")`** — esp-idf-svc `wifi.rs` example pattern;
   keeps secrets out of the repo but requires build-time env vars (awkward inside
   `nix develop .#firmware-fhs` + `cargo espflash`).
3. **sdkconfig (Kconfig)** — unusual, regenerated, not secret-friendly.
4. **NVS storage** — esp-idf-svc `nvs` module provides `EspNvsPartition` /
   `EspNvsStorage` with `get_str`/`set_str` (verified `src/nvs.rs:502/526`). Best
   long-term, but needs a one-time provisioning path (a "set credentials" command).

**Recommendation (dev-friendly MVP):** a small `config.rs` that resolves, in order:
(a) NVS if present, else (b) `option_env!`/`env!` build vars, else (c) a
placeholder default (`SSID="", password="", robot_id="a1"`), with an explicit
`log::warn!` when placeholders are used. This keeps secrets out of `main.rs` while
making on-device dev trivial. `robot_id` default `"a1"` (matches server tests).
Compose the URI in code from `HUB_HOST`/`HUB_PORT` + `robot_id` (URL-encode the id)
rather than a single hand-written URL string.

---

## 5. Reconnection strategy

- **WS-level reconnection is largely free:** the C lib auto-reconnects
  (`disable_auto_reconnect=false` default). Create `EspWebSocketClient` **once** and
  keep it alive; the callback fires `Disconnected` → (auto) → `Connected`. On each
  `Connected`, re-emit the `register` envelope (if adopted).
- **WiFi-level reconnection:** `BlockingWifi::connect()` is one-shot; wrap it in a
  bounded retry loop with backoff (e.g. 1s → 2s → 4s, cap 15s) and log each attempt
  (improves on the C++ unbounded `while`). After an initial connection, handle
  `STA_DISCONNECTED` by re-running the connect loop.
- **Blocking loop vs embassy async:** for phase 1, **blocking is simplest and
  sufficient**. The ws client is callback-based (not async) and `BlockingWifi` is
  blocking; `embassy-executor` adds no benefit here. `embassy-time-driver` is
  already enabled and can supply `embassy_time::Ticker`/`Delay` for backoff without
  a full executor. Keep `main()` alive with `loop { rx.recv() }` over an
  `std::sync::mpsc` channel from the ws callback (the idiomatic example pattern).
- **Watchdog / power:** long blocking WiFi connects can trip the FreeRTOS task
  watchdog (TWDT); use timeouts/backoff and consider feeding or disabling the main
  task watchdog. The reference C++ **disables the brownout detector**
  (`WRITE_PERI_REG(RTC_CNTL_BROWN_OUT_REG,0)`) because ESP32-CAM WiFi TX spikes can
  brownout-reset on weak USB power; Rust must consider the same (or ensure adequate
  power) — flag as a risk to resolve before flashing.

---

## 6. Replicate vs. improve (vs. reference C++)

| Aspect | C++ reference | Rust (this change) |
|--------|---------------|--------------------|
| Architecture | Robot = HTTP server + MJPEG (port 80), no hub | Robot = WebSocket **client** to hub (do NOT port `WebServer.cpp`) |
| Credentials | Hardcoded in `main.cpp` | config.rs + env!/NVS fallback, no secrets in main |
| WiFi connect | Unbounded blocking `while` | Bounded retry + backoff + logging |
| Reconnect | None after first connect | C-lib WS auto-reconnect + WiFi reconnect loop |
| Keepalive | None | C-lib protocol ping/pong (config-driven) |
| Identity | None | `robot_id` from config/query |
| Static IP / mDNS | None (and unneeded) | Not needed: hub never dials the robot |

---

## 7. Open questions (proposal must resolve)

1. **Credential provisioning UX for phase 1** — `env!` build vars vs. a `config.rs`
   with placeholder consts vs. NVS write path. (Recommend the layered config.rs
   fallback; NVS write as a follow-up.)
2. **`robot_id` default/uniqueness** — single-robot MVP hard default `"a1"`? How is
   a second robot distinguished later (NVS key)?
3. **Register envelope** — should the robot emit `{"type":"register",...}` on every
   (re)connect (it broadcasts to all clients), or is the server's auto-registration
   (query `robot_id`) sufficient? What `payload` (name/fw version)?
4. **Server greeting/ack** — server sends nothing on connect; confirm the robot
   must not wait for an ack before sending.
5. **`ts` source** — omit vs. monotonic (boot) vs. SNTP
   (`esp_idf_svc::sntp`). Recommend omit/monotonic for MVP; SNTP deferred.
6. **Liveness ownership** — rely solely on C-lib protocol ping for keepalive, or
   also add a JSON heartbeat? (Server emits no JSON ping today.) Recommend protocol
   ping now; JSON heartbeat in the telemetry change.
7. **Watchdog + brownout** — confirm whether the C++ brownout-disable is required in
   Rust (power supply adequacy) before flashing; decide TWDT handling for long
   WiFi/WS blocking calls.
8. **TLS/`ws://`** — phase 1 is LAN `ws://` (no auth, per roadmap phase 1/README
   §8); confirm `TransportOverTCP` and no cert handling in this slice.

---

## 8. Minimal Cargo.toml delta (for the proposal, not applied)

```toml
[dependencies]
esp-idf-svc = { version = "0.52.1", features = ["critical-section", "embassy-time-driver", "embassy-sync"] }  # unchanged
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"  # optional

[[package.metadata.esp-idf-sys.extra_components]]
remote_component = { name = "espressif/esp_websocket_client", version = "1.1.0" }
```
