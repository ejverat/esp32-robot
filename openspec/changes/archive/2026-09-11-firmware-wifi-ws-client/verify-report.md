```yaml
schema: gentle-ai.verify-result/v1
evidence_revision: sha256:452851fce0d7d96d549cd22fcb7e11e9d37cd58a7b916793af57f340018f1090
verdict: pass
blockers: 0
critical_findings: 0
requirements: 10/10
scenarios: 25/25
test_command: N/A (firmware-only change; no host tests per openspec/config.yaml testing.firmware)
test_exit_code: 0
test_output_hash: sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
build_command: /nix/store/3r9ar16m779lj6j2rxcmp87iq6kvddwc-esp32-robot-firmware-fhs/bin/esp32-robot-firmware-fhs -c 'cd firmware && cargo build'
build_exit_code: 0
build_output_hash: sha256:b1ecfdeab05729bb4d9f1bac4fc358d0f42162e44a2dc7d0a1735b4024771fe5
```

# Verify Report — firmware-wifi-ws-client

Revision 2 (regenerated after on-device completion). Revision 1 covered static verification only
(`verdict: fail`, `blockers: 4` — the four hardware-gated checks). All four are now complete on a
real ESP32-CAM and persisted as `[x]` in `tasks.md`; the envelope is now `verdict: pass`,
`blockers: 0`.

## Overall verdict

**verified**. Static verification (code inspection + independent cross-compilation) is clean for
all 10 requirements and all 25 scenarios, and the four on-device checks completed with hardware.
No FAIL, no unresolved mismatch, no critical code defect.

### Post-verify delta (this revision)

During on-device work the maintainer temporarily compiled real network credentials into
`config.rs` defaults to exercise the target network, and removed the placeholder warning. The
sync phase flagged the credential defaults as a spec violation (R1 "secrets out of source") and
the warning removal as a doc/behavior inconsistency. Final action taken:

- `config.rs` defaults reverted to the placeholder values (`CHANGE_ME_*`, `192.168.1.10:8080`)
  and the placeholder warning restored — the source state is now **byte-identical to the settled
  apply candidate** (`evidence_revision` above matches the apply settle hash).
- Real credentials continue to live only in device NVS (persisted; flash preserves NVS), so the
  physical device keeps operating while the repository holds no secrets.
- Rebuilt (`build_exit_code: 0`) and re-flashed; device verified connected after the re-flash.

## Independent cross-compilation (not trusting apply's claim)

Command (FHS wrapper invoked directly because the flake `shellHook` `exec`s the wrapper and drops
`-c` from `nix develop -c`):

```
/nix/store/3r9ar16m779lj6j2rxcmp87iq6kvddwc-esp32-robot-firmware-fhs/bin/esp32-robot-firmware-fhs -c 'cd firmware && cargo build'
```

Result (revision 2; full output hashed as `sha256:b1ecfdea…1fe5`):

```
warning: `esp32-robot-firmware` (bin "esp32-robot-firmware") generated 1 warning
    Finished `dev` profile [optimized + debuginfo] target(s) in 0.33s
```

- Exit code **0**. The single warning is the benign `linker_messages` notice (`ldproxy` running).

## Requirement-by-requirement verdict

| # | Requirement | Verdict (static) | On-device | Evidence |
|---|-------------|------------------|-----------|----------|
| R1 | WiFi station + NVS-resolved credentials | PASS | **PASS** | `config.rs` `resolve()`/`read_or_default()`; on-device: `config resolved: ssid=…, hub_url=192.168.1.166:8080, robot_id=a1` on a provisioned device; post-`erase-flash` the empty NVS was repopulated from defaults and the robot connected (write-back proven end-to-end) |
| R2 | Layered configuration resolution (NVS → defaults) | PASS | n/a | `config.rs` key consts + resolution order; NVS value wins, default consulted only when absent/empty |
| R3 | WebSocket client to hub (dial URI, URL-encode, no greeting wait) | PASS | **PASS** | `ws_client.rs` `build_uri()`/`percent_encode()`; on-device: hub `/robots` lists `a1` after `WebSocket connected` (`ws://192.168.1.166:8080/ws/robot?robot_id=a1`) |
| R4 | Bounded backoff + STA re-entry + WS auto-reconnect | PASS | **PASS** | `wifi.rs`/`main.rs` backoff loop; on-device: (a) with TCP 8080 firewalled, `Reconnect after 3000 ms` retries for minutes then reconnect unaided after unblock, no device reset; (b) after a hub restart, `a1` re-registered within seconds (6 polls × 24 s); no TWDT reset observed |
| R5 | Explicit non-zero WS client config | PASS | n/a | `ws_client.rs` `client_config()` explicit fields (prio 5, stack 4096, buffer 1024, ping 10 s, pong 60 s, reconnect 3 s, network timeout 10 s, auto-reconnect on, pingpong discon on) |
| R6 | Connection state transitions logged | PASS | **PASS** | `ws_client.rs` `log_event()`; on-device serial: `WiFi started (STA)` → `wifi:connected with …, aid=19/21, channel 5` → `WiFi connected (netif up)` → `WebSocket connected` (~80 ms after netif up) |
| R7 | Dependency + build contract | PASS | n/a | `Cargo.toml` serde/serde_json + `esp_websocket_client` 1.1.0 extra component; build exit 0 |
| R8 | Main stays alive without busy-spinning | PASS | n/a | `main.rs` `loop { match rx.recv() … }` — blocks on channel receive |
| R9 | No register envelope / heartbeat / `ts` | PASS | n/a | zero `EspWebSocketClient::send(...)` sites; only channel `tx.send(NetEvent::…)`; no `register`, no JSON ping, no `ts` |
| R10 | Non-goals + build-config stability | PASS | n/a | `sdkconfig.defaults` byte-identical; no motor/camera/telemetry/SNTP/TLS-WSS/auth/`embassy-executor` |

Static coverage: **10/10 requirements, 25/25 scenarios**. On-device runtime confirmation: **PASS**
for R1, R3, R4, R6.

## On-device verification (completed)

Hardware: ESP32-CAM AI-Thinker + MB adapter (CH340), ESP32 rev v3.1, 4 MB flash, on WiFi
`FamVeraCen` (channel 5), hub at `192.168.1.166:8080`.

| Check | Result | Evidence |
| --- | --- | --- |
| Flash + observe | PASS | Normal boot (`boot:0x13 SPI_FAST_FLASH_BOOT`, ESP-IDF v5.5.3); app size 1,273,312 bytes pre-fix / 1,273,008 bytes post-fix (30.83 % of 4 MB); serial chain `config resolved` → `wifi:connected` → `sta ip: 192.168.1.130` → `WebSocket connected`; hub `/robots` → `["a1"]` |
| Auto-reconnect | PASS | (a) firewall-blocked port: minutes of `Reconnect after 3000 ms`, unaided reconnect after unblock, no reset; (b) hub restart: `a1` re-registered within seconds |
| First boot after erase | PASS (functional) | `erase-flash` + re-flash; empty NVS repopulated; robot connected with the persisted real credentials — write-back proven. The literal `first boot: persisted defaults to NVS` line was not captured on this rig (MB/CH340 strap quirk documented in `apply-progress.md`); behavior proven by system state |
| `.bin` size | PASS | 1,273,008 bytes (30.83 %) — within the 4 MB budget |

## Task completion status

**31/31** — all 7 tasks, the 5 keep-out checklist items, and the 4 on-device checks are `[x]` in
`tasks.md` (evidence lines added inline). No unchecked implementation tasks remain.

## Structured status and actionContext findings

- Dependencies after this revision: `verify: ready`, `sync: unblocked`, `archive: unblocked`
  (zero unchecked implementation tasks; zero blockers).
- `actionContext.mode: repo-local`, workspace root `/home/ejverat/Projects/esp32-robot`.

## Test/validation commands

- Build (independent): wrapper `cd firmware && cargo build` → exit 0, `Finished dev profile`.
- Host tests: none exist for `firmware/` (`openspec/config.yaml` → `testing.firmware`); server
  tests are out of scope (no `server/` files touched).
- On-device: `espflash flash` / `erase-flash` / serial capture via the project skill
  `.pi/skills/esp32-ondevice-workflow/` (strap-safe capture pattern).

## Strict TDD compliance

`openspec/config.yaml` sets `strict_tdd: true` with `tdd_scope` limited to server-side and
host-testable code; `testing.firmware` states TDD does not apply to firmware-only code. No
host-testable code changed; no TDD cycle table is required or asserted.

## Review workload / PR boundary findings

- Committed forecast: 382 changed lines; actual authored diff **353** (347 + / 6 −) — under both
  the forecast and the 400-line budget. No `size:exception`, no chaining.
- Build side effects (not authored): `Cargo.lock` +2, `components_esp32.lock` (new).
- Scope creep: none — only the 7 authorized surfaces plus the two build-generated lockfiles.

## Confirmed apply deviations (behavior-neutral)

1. `main.rs` uses `.map_err(|e| e.0)?` for `ws_client::start` (no `From<EspIOError> for EspError`
   in esp-idf-svc 0.52.1).
2. `ws_client.rs` matches the actual `WebSocketEventType` enum name (design wrote `WsEventType`).

## Exact blockers

None.
