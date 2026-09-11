# Apply Progress — firmware-wifi-ws-client

> Backend: `openspec`. Executor: SDD apply.

## Status

`applyState: ready` (authoritative, `isNonAuthoritative: false`). All Tasks 1–7, the
keep-out checklist, and the 4 on-device verification steps completed with hardware
(ESP32-CAM + MB adapter, CH340, ESP32 rev v3.1, 4MB flash).

## Completed tasks (persisted checkboxes updated to `- [x]`)

- Task 1 — Cargo.toml dependency + `esp_websocket_client` 1.1.0 extra component (3/3)
- Task 2 — `net.rs` shared `NetEvent` enum + `EventSender` alias (3/3)
- Task 3 — `config.rs` NVS → defaults resolution (3/3)
- Task 4 — `wifi.rs` STA init + bounded-backoff connect (4/4)
- Task 5 — `ws_client.rs` dial URI + explicit config + callback client (4/4)
- Task 6 — `main.rs` rewrite (module wiring + keep-alive loop) (3/3)
- Task 7 — README Spanish firmware note (2/2)
- Keep-out checklist (5/5, verified via diff review)

On-device (all 4 PASS, evidence from live serial capture + hub API):

- [x] Flash + observe: normal boot (`boot:0x13 SPI_FAST_FLASH_BOOT`), `config resolved: ssid=FamVeraCen, hub_url=192.168.1.166:8080, robot_id=a1`, `wifi:connected with FamVeraCen, aid=19/21, channel 5`, `sta ip: 192.168.1.130`, **`WebSocket connected`** (~80 ms after netif up). Robot listed in hub `/robots`. <!-- sdd-owner: implementation -->
- [x] Auto-reconnect: (a) with the hub port firewalled, the client retried for minutes (`Reconnect after 3000 ms`, no TWDT reset) and reconnected on its own the moment the firewall rule was added — no device reset; (b) after a hub restart, robot re-registered within seconds (polled `/robots` 6×24 s, `a1` present). <!-- sdd-owner: implementation -->
- [x] First-boot clean: `espflash erase-flash` + re-flash; empty NVS was repopulated from defaults (robot connected with the real SSID/hub/robot_id — only possible via write-back; functional proof; the log line was not captured due to the MB adapter strap quirk, see findings). <!-- sdd-owner: implementation -->
- [x] `.bin` size: 1,273,312 bytes (30.84%) pre-fix; **1,273,008 bytes (30.83%)** after the warning fix — well within the 4 MB budget. <!-- sdd-owner: implementation -->

## On-device findings

1. **Credential handling corrected (spec compliance)**: during on-device work the compiled
defaults temporarily held real network credentials and the placeholder warning was removed; the
sync phase flagged this against spec R1 ("secrets out of source"). Final state: defaults reverted
to `CHANGE_ME_*` placeholders and the warning **restored** as the unprovisioned signal — source is
byte-identical to the settled apply candidate. Real credentials live only in device NVS (flash
preserves NVS), so the physical unit keeps operating with no secrets in the repository.
2. **NixOS firewall blocked the hub** from LAN clients (default `networking.firewall`); inbound
TCP 8080 must be allowed (`iptables` rule added; `allowedTCPPorts` recommended in config). Not a
firmware issue.
3. **MB/CH340 strap quirk** (documented, not a defect): opening the serial port can toggle
DTR/RTS and strap GPIO0 low → chip lands in ROM download mode (`waiting for download` loop with
`rst:0xb TGWDT_CPU_RESET` — ROM idle timeout, not an app watchdog). Reliable capture pattern:
open port → release lines (`DTR=0, RTS=0`) → RTS pulse to reset. The physical RST button and a
USB power cycle boot the app cleanly.

## Files changed (authorized surfaces)

| File | Kind | + / − |
|------|------|-------|
| `firmware/Cargo.toml` | edit | +5 / −0 |
| `firmware/src/net.rs` | new | +13 / −0 |
| `firmware/src/config.rs` | new | +74 / −0 |
| `firmware/src/wifi.rs` | new | +92 / −0 |
| `firmware/src/ws_client.rs` | new | +99 / −0 |
| `firmware/src/main.rs` | rewrite | +54 / −6 |
| `README.md` | edit | +10 / −0 |

**Total (authorized surfaces): 347 additions + 6 deletions = 353 changed lines.**
Under the 400-line budget and under the committed estimate (382).

Build side effects (NOT authorized edit surfaces, NOT counted in the review budget;
left in the working tree for the maintainer to decide):

- `firmware/Cargo.lock`: +2 lines (`serde`/`serde_json` recorded as direct deps of the bin).
- `firmware/components_esp32.lock`: new (ESP-IDF component-manager lock for
  `espressif/esp_websocket_client` 1.1.0).

## Verification evidence (cross-compilation, no host tests)

`strict_tdd` does NOT apply to firmware-only code (`openspec/config.yaml` → `testing.firmware`).

Environment note: `nix develop .#firmware-fhs -c bash -c '…'` from the instructions does not
work in this flake — the `firmware-fhs` shellHook `exec`s the FHS wrapper and drops the `-c`
command. I invoked the FHS wrapper binary directly with `-c` (same sandbox), e.g.
`/nix/store/<hash>-esp32-robot-firmware-fhs/bin/esp32-robot-firmware-fhs -c 'cd firmware && cargo build'`.
Additionally, `[[package.metadata.esp-idf-sys.extra_components]]` is read by the esp-idf-sys
build script from the root manifest but is not tracked for re-run, so the component was only
fetched after `cargo clean -p esp-idf-sys` (forced rebuild).

| Task | Command | Result |
|------|---------|--------|
| 1 | `cargo clean -p esp-idf-sys` then `cargo build` | Finished `dev` profile in 1m 19s; `esp_websocket_client` fetched into `managed_components`, `cargo:rustc-cfg=esp_idf_comp_espressif__esp_websocket_client_enabled` emitted |
| 2 | `cargo build` (after `mod net;`) | Finished in 4.45s |
| 3 | `cargo build` (after `mod config;`) | Finished in 2.73s |
| 4 | `cargo build` (after `mod wifi;`) | Finished in 2.78s |
| 5 | `cargo build` (after `mod ws_client;`) | Finished in 2.76s |
| 6 | `cargo build` (full crate) | Finished in 6.49s (after 2 compile errors fixed, see deviations) |

## Deviations from design

1. **`main()` error conversion for `ws_client::start`.** Design §2 declares
   `start(...) -> Result<EspWebSocketClient<'static>, EspIOError>` while `main()` returns
   `Result<(), EspError>`. esp-idf-svc 0.52.1 has no `From<EspIOError> for EspError`, so the
   design's bare `?` does not compile. Adapted at the call site with
   `ws_client::start(...).map_err(|e| e.0)?` (EspIOError is a newtype over EspError).
   Behavior unchanged.
2. **Actual WS event type name.** The vendored API names the enum `WebSocketEventType`
   (not `WsEventType`), and the callback receives `&Result<WebSocketEvent, EspIOError>`
   (the design §5.4 `Err(_)` arm maps to the `Err` case of that Result). Behavior unchanged.

No dependency versions were changed.

## Keep-out checklist (verified)

- `firmware/sdkconfig.defaults` byte-identical (0 diff); no `CONFIG_ESP_WIFI_ENABLED` added.
- Brownout detector untouched (documented risk only).
- No `embassy-executor`, no `static_cell`, no async runtime.
- No motor/camera/telemetry/SNTP/TLS-WSS/auth code paths.
- `serde`/`serde_json` remain declared in `Cargo.toml` (unused this slice).
- Only `tx.send(...)` channel sends exist; **zero** `EspWebSocketClient::send(...)` call sites.
- Never logs the WiFi password (only `ssid`, `hub_url`, `robot_id` are logged).

## Workload / PR boundary

Single PR (7 work-unit commits planned in tasks.md). Delivery strategy `ask-on-risk`; no
pause needed — actual 353 changed lines < 400 budget. No chaining, no `size:exception`.

## Structured status

Consumed authoritative `applyState: ready`, `actionContext.mode: repo-local`,
`allowedEditRoots: [/home/ejverat/Projects/esp32-robot]`, no warnings. Produced:
implementation tasks 27/31 complete (22 tasks + 5 keep-out), 4 on-device tasks remain pending.
`nextRecommended: sdd-verify` (apply implementation complete; on-device verification stays
documented as pending, not an apply blocker for non-hardware work).
