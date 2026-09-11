# Tasks — firmware-wifi-ws-client

> Backend: `openspec`. Binding design: `openspec/changes/firmware-wifi-ws-client/design.md`.
> Firmware has no host tests (`openspec/config.yaml` → `testing.firmware`): verification is
> cross-compilation (`nix develop .#firmware-fhs` + `cd firmware && cargo build`), code
> inspection, and on-device checks. Strict TDD (RED/GREEN/TRIANGULATE/REFACTOR) does **not**
> apply to firmware-only code; every task is implementation + cross-compile/inspection verification.
>
> **Compilation note:** a new Rust module only type-checks once declared with `mod <name>;` in
> `main.rs`. Tasks 2–5 each declare their module in `main.rs` so the cross-compile is meaningful;
> those one-line declarations are consolidated into Task 6's `main.rs` rewrite and are counted
> there, not in the module line totals.

## Review Workload Forecast

| Field | Value |
|-------|-------|
| Estimated changed lines | 382 (372 additions + 10 deletions) |
| 400-line budget risk | Medium |
| Chained PRs recommended | No |
| Suggested split | single PR (7 work-unit commits) |
| Delivery strategy | ask-on-risk |
| Chain strategy | pending |

```text
Decision needed before apply: No
Chained PRs recommended: No
Chain strategy: pending
400-line budget risk: Medium
```

### Exact line accounting (additions + deletions)

| # | Task | Files | + | − | Changed |
|---|------|-------|---|---|---------|
| 1 | Cargo.toml wiring | `firmware/Cargo.toml` | 8 | 0 | 8 |
| 2 | Shared event enum | `firmware/src/net.rs` | 14 | 0 | 14 |
| 3 | Config resolution | `firmware/src/config.rs` | 80 | 0 | 80 |
| 4 | WiFi STA + backoff | `firmware/src/wifi.rs` | 100 | 0 | 100 |
| 5 | WS client | `firmware/src/ws_client.rs` | 100 | 0 | 100 |
| 6 | `main.rs` rewrite | `firmware/src/main.rs` | 58 | 10 | 68 |
| 7 | README note (es) | `README.md` | 12 | 0 | 12 |
| | **Total** | | **372** | **10** | **382** |

- `openspec/` artifacts are **excluded** from the review budget.
- `firmware/sdkconfig.defaults` is **unchanged (0 lines)**.
- Code-only subtotal (excluding the `README.md` note): 360 additions + 10 deletions = **370**.
- This resolves design §10's forecast (≈375, range 360–420) to a committed **382 changed lines**,
  which is **under** the 400-line budget. No `size:exception` and no chaining are required; at
  apply time, if the actual diff exceeds 400, pause per `ask-on-risk` (do not auto-chain or infer
  an exception).

---

## Task 1 — Cargo.toml dependency + component wiring

Touches: `firmware/Cargo.toml` (append only; no other lines change). Changed lines: **+8**.

Adds:
- Under `[dependencies]`: `serde = { version = "1", features = ["derive"] }` and `serde_json = "1"`.
- At end of file: `[[package.metadata.esp-idf-sys.extra_components]]` +
  `remote_component = { name = "espressif/esp_websocket_client", version = "1.1.0" }`.

Keep-out: **no new Cargo feature**; `esp-idf-svc` features stay exactly
`["critical-section", "embassy-time-driver", "embassy-sync"]`. `serde`/`serde_json` are
declared-but-unused this slice (binding spec) — do not remove them in later tasks.

- [x] Add the two dependency lines and the `extra_components` block (exact version `1.1.0`). <!-- sdd-owner: implementation -->
- [x] Verify cross-compile: `nix develop .#firmware-fhs` then `cd firmware && cargo build` — the unchanged hello-world `main.rs` still compiles, proving the managed component is fetched and the `esp_idf_comp_espressif__esp_websocket_client_enabled` gate is satisfied. <!-- sdd-owner: implementation -->
- [x] Inspect `Cargo.toml`: confirm `esp-idf-svc` features are unchanged and no new Cargo feature was added. <!-- sdd-owner: implementation -->

## Task 2 — `net.rs` shared event enum

Touches: `firmware/src/net.rs` (new). Changed lines: **+14**.

Adds: `enum NetEvent { WifiStaDisconnected, WsEvent }` and
`type EventSender = std::sync::mpsc::Sender<NetEvent>` (unbounded channel — the upstream
`http_ws_client` example uses this pattern). No other modules depend on each other; `net.rs`
depends on nothing.

- [x] Add `mod net;` to `main.rs` and create `firmware/src/net.rs` with the `NetEvent` enum and `EventSender` alias. <!-- sdd-owner: implementation -->
- [x] Verify cross-compile: `nix develop .#firmware-fhs` + `cd firmware && cargo build` (the enum and alias type-check). <!-- sdd-owner: implementation -->
- [x] Inspect: `net.rs` has no `use crate::wifi` / `use crate::ws_client` imports (no cyclic dependency). <!-- sdd-owner: implementation -->

## Task 3 — `config.rs` NVS → defaults resolution

Touches: `firmware/src/config.rs` (new). Changed lines: **+80**.

Adds: `NVS_NAMESPACE = "robot"`, key consts (`wifi.ssid`, `wifi.password`, `hub.url`,
`hub.robot_id`), default consts (`CHANGE_ME_SSID` / `CHANGE_ME_PASSWORD` /
`192.168.1.10:8080` / `a1`), `Config { ssid, password, hub_url, robot_id }`, and
`resolve(&EspNvs<NvsDefault>) -> Result<Config, EspError>` implementing: `get_str` → non-empty
wins → else `set_str(default)` write-back, `wrote_any` → first-boot log, placeholder-in-use
`warn!`. **Never logs the password.**

- [x] Add `mod config;` to `main.rs` and create `firmware/src/config.rs` with the key/default consts, `Config` struct, and `resolve()` (read → fallback → write-back order per design §3.1). <!-- sdd-owner: implementation -->
- [x] Verify cross-compile: `nix develop .#firmware-fhs` + `cd firmware && cargo build` (NVS `get_str`/`set_str` APIs resolve against esp-idf-svc 0.52.1). <!-- sdd-owner: implementation -->
- [x] Inspect: all key names ≤ 15 chars; read buffers `[u8; 33]`/`[u8; 65]`/`[u8; 64]`/`[u8; 33]`; defaults match design §3.1; no `log::*!` call site references `password`. <!-- sdd-owner: implementation -->

## Task 4 — `wifi.rs` STA init + bounded-backoff connect

Touches: `firmware/src/wifi.rs` (new). Changed lines: **+100**.

Adds: `struct Wifi { blocking: BlockingWifi<EspWifi<'static>>, _sub: EspSubscription<'static, System> }`;
`init(modem, &sysloop, nvs, &config, tx)` building `EspWifi::new` → `BlockingWifi::wrap` →
`set_configuration(&Client(...))` (SSID/password via `try_into::<heapless::String<32/64>>()`,
`AuthMethod::WPA2Personal` as a named `const`) → `start()` → `subscribe::<WifiEvent>` sending
`NetEvent::WifiStaDisconnected`; `connect(&mut Wifi)` bounded loop (`is_up()` early-out →
`connect()` → `wait_netif_up()`, on error `warn!` + `sleep(backoff)` with 1s→2s→4s→8s→15s cap).
`set_configuration`/`start` are **not** repeated on re-entry.

- [x] Add `mod wifi;` to `main.rs` and create `firmware/src/wifi.rs` with `Wifi`, `init()`, and `connect()`. <!-- sdd-owner: implementation -->
- [x] Verify cross-compile: `nix develop .#firmware-fhs` + `cd firmware && cargo build` (BlockingWifi/EspWifi/subscription APIs resolve). <!-- sdd-owner: implementation -->
- [x] Inspect: backoff cap is `15s`, factor `2`, reset per `connect()` invocation; `_sub` keeps the subscription alive; `AUTH_METHOD` is a `const`; no `set_configuration`/`start` call inside `connect()`. <!-- sdd-owner: implementation -->
- [x] Inspect: `connect()` uses event-loop blocking + `thread::sleep` yields (no busy-spin, no `embassy-executor`). <!-- sdd-owner: implementation -->

## Task 5 — `ws_client.rs` dial URI + explicit config + callback client

Touches: `firmware/src/ws_client.rs` (new). Changed lines: **+100**.

Adds: `UriError`; `build_uri(&Config) -> Result<String, UriError>` (`hub.url.rsplit_once(':')`,
`port.parse::<u16>()`, host non-empty → `ws://<host>:<port>/ws/robot?robot_id=<encoded>`); private
`percent_encode` (RFC 3986, no new crate); `client_config() -> EspWebSocketClientConfig<'static>`
with the design §3.3 frozen values (`TransportOverTCP`, `task_prio=5`, `task_stack=4096`,
`buffer_size=1024`, `ping_interval_sec=10`, `pingpong_timeout_sec=60`, `reconnect_timeout_ms=3000`,
`network_timeout_ms=10000`, `disable_auto_reconnect=false`, `disable_pingpong_discon=false`,
`..Default::default()` only for optional TLS/header fields); `start(uri, &config, tx)` calling
`EspWebSocketClient::new(uri, &cfg, 5s, |event| { log; tx.send(NetEvent::WsEvent).ok() })` **once**;
`log_event` mapping every `WsEventType` variant to the design §5.4 log level.

- [x] Add `mod ws_client;` to `main.rs` and create `firmware/src/ws_client.rs` with `build_uri`, `percent_encode`, `client_config`, `start`, and `log_event`. <!-- sdd-owner: implementation -->
- [x] Verify cross-compile: `nix develop .#firmware-fhs` + `cd firmware && cargo build` (`esp_idf_svc::ws::client` types resolve with the extra component from Task 1). <!-- sdd-owner: implementation -->
- [x] Inspect `client_config()`: every required field is a non-zero literal; `transport` is explicit `TransportOverTCP`; no TLS/cert fields set; `disable_auto_reconnect` and `disable_pingpong_discon` are `false`. <!-- sdd-owner: implementation -->
- [x] Inspect: **zero** `client.send(...)` call sites in this module (no register envelope, no JSON heartbeat, no manual Ping/Pong). <!-- sdd-owner: implementation -->

## Task 6 — `main.rs` rewrite (module wiring + keep-alive loop)

Touches: `firmware/src/main.rs` (rewrite; consolidates the `mod` declarations). Changed lines: **+58 / −10 = 68**.

Adds: `fn main() -> Result<(), EspError>` wiring per design §5.1 — `link_patches()` →
`EspLogger::initialize_default()` → `Peripherals::take()` → `EspSystemEventLoop::take()` →
`EspDefaultNvsPartition::take()` (cloned: one clone to `EspNvs::new(..., "robot", true)`, other to
`EspWifi`) → `config::resolve(&nvs)?` + `drop(nvs)` → `mpsc::channel::<NetEvent>()` →
`wifi::init(...)` → `wifi::connect(...)` → `build_uri(&config)` (on `UriError`: `error!` + fall back
to `DEFAULT_HUB_URL`, do **not** overwrite NVS) → `ws_client::start(...)` bound to `_ws` (never
dropped) → `loop { rx.recv() }` (`WifiStaDisconnected` → `wifi::connect`, `WsEvent` → no-op,
`Err(_)` → `warn!` + `thread::park()`).

- [x] Rewrite `firmware/src/main.rs` as above, keeping the `mod config; mod net; mod wifi; mod ws_client;` declarations and the boot→loop order from design §5.1. <!-- sdd-owner: implementation -->
- [x] Verify cross-compile: `nix develop .#firmware-fhs` + `cd firmware && cargo build` (full crate compiles for `xtensa-esp32-espidf`). <!-- sdd-owner: implementation -->
- [x] Inspect: the keep-alive loop blocks on `rx.recv()` (no busy-spin); the WS client is created once and held in `_ws`; no `send()`/`register`/heartbeat path; no password logged. <!-- sdd-owner: implementation -->

## Task 7 — README firmware note (Spanish)

Touches: `README.md` (firmware section, `user_facing_docs: es`). Changed lines: **+12**.

Adds: a short Spanish note that credentials live in NVS (with compiled-placeholder fallback),
first boot persists defaults, NVS is cleared with `erase-flash`, and the brownout/power caveat
(adequate power / short cable before flashing).

- [x] Add the Spanish note to the `README.md` firmware section covering NVS credentials, `erase-flash`, and the brownout/power caveat. <!-- sdd-owner: implementation -->
- [x] Inspect: text is Spanish and consistent with `user_facing_docs: es`; no real credentials introduced. <!-- sdd-owner: implementation -->

---

## Keep-out checklist (verify during apply diff review)

- [x] `firmware/sdkconfig.defaults` is byte-identical (0 diff); no `CONFIG_ESP_WIFI_ENABLED` line added. <!-- sdd-owner: implementation -->
- [x] Brownout detector is untouched (documented risk only, not modified). <!-- sdd-owner: implementation -->
- [x] No `embassy-executor` anywhere; no `static_cell`/async runtime introduced. <!-- sdd-owner: implementation -->
- [x] No motor, camera, telemetry, SNTP, TLS/WSS, or auth code paths. <!-- sdd-owner: implementation -->
- [x] `serde`/`serde_json` remain declared in `Cargo.toml` (unused this slice, per binding spec). <!-- sdd-owner: implementation -->

## On-device verification sequence (final, once)

- [x] Flash: `cd firmware && cargo espflash flash`; observe serial for WiFi started → connected/netif-up → `WebSocket connected`. <!-- sdd-owner: implementation -->
  - On-device PASS (ESP32-CAM, ESP32 rev v3.1, 4 MB): `config resolved: ssid=…, hub_url=192.168.1.166:8080, robot_id=a1` → `wifi:connected with FamVeraCen, aid=19/21, channel 5` → `sta ip: 192.168.1.130` → `WebSocket connected` (~80 ms after netif up); hub `/robots` lists `a1`.
- [x] Restart the hub; observe `WebSocket disconnected (auto-reconnect pending)` → `WebSocket connected` (C-library auto-reconnect, no TWDT reset). <!-- sdd-owner: implementation -->
  - On-device PASS: (a) with TCP 8080 firewalled, retried for minutes (`Reconnect after 3000 ms`) and reconnected unaided once the rule was added, no device reset; (b) after a hub restart, `a1` re-registered within seconds (6 polls × 24 s).
- [x] First-boot re-test: `cargo espflash erase-flash` + re-flash; observe `first boot: persisted defaults to NVS` and hub log registering `robot_id=a1`. <!-- sdd-owner: implementation -->
  - On-device PASS (functional): after `erase-flash` + re-flash, the empty NVS was repopulated and the robot connected — only possible via the first-boot write-back. The log line itself was not captured (MB/CH340 strap quirk); see `apply-progress.md` findings.
- [x] Check `.bin` size after build (serde_json + WS component growth within flash budget). <!-- sdd-owner: implementation -->
  - Measured: 1,273,312 bytes (30.84%) with real defaults; 1,273,008 bytes (30.83%) after the warning fix; final placeholder-defaults build re-measured at flash time. Well within 4 MB.

## Work-unit commit mapping (Spanish conventional style)

Each task maps to one work-unit commit so a reviewer can read the story in order, e.g.:
`Infra:`, `Implementar config NVS`, `Implementar wifi STA`, `Implementar cliente WS`,
`Implementar main`, `Docs:`. Rollback per task reverts only that commit.
