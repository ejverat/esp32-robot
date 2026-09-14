# Tasks — firmware-motors

> Backend: `openspec`. Binding design: `openspec/changes/firmware-motors/design.md`.
> Firmware has no host tests (`openspec/config.yaml` → `testing.firmware`): verification is
> cross-compilation inside the FHS wrapper (`$(ls -d /nix/store/*-esp32-robot-firmware-fhs/bin/*firmware-fhs | head -1) -c 'cd firmware && cargo build'`),
> code inspection, and on-device checks. Strict TDD (RED/GREEN/TRIANGULATE/REFACTOR) does **not**
> apply to firmware-only code; every task is implementation + cross-compile/inspection verification.
>
> **Compilation note:** a new Rust module only type-checks once declared with `mod <name>;` in
> `main.rs`. Tasks 1–2 each declare their module in `main.rs` so the cross-compile is meaningful;
> those two one-line declarations are consolidated into Task 3's `main.rs` wiring and are counted
> there, not in the module line totals. `command.rs` is host-inspectable pure code (no hardware,
> no I/O) — a host-side `cargo check` on the unit-testable parts is NOT available (firmware has no
> host runner), so the pure-function verification below is by inspection of the frozen formula
> (§5 worked examples) plus cross-compilation.

## Review Workload Forecast

| Field | Value |
|-------|-------|
| Estimated changed lines | 257 (255 additions + 2 deletions) |
| 400-line budget risk | Low |
| Chained PRs recommended | No |
| Suggested split | single PR (4 work-unit commits) |
| Delivery strategy | ask-on-risk |
| Chain strategy | pending |

```text
Decision needed before apply: No
Chained PRs recommended: No
Chain strategy: pending
400-line budget risk: Low
```

### Exact line accounting (additions + deletions)

| # | Task | Files | + | − | Changed |
|---|------|-------|---|---|---------|
| 1 | `command.rs` parse + mixing | `firmware/src/command.rs` | 70 | 0 | 70 |
| 2 | `motors.rs` LEDC driver + motor task | `firmware/src/motors.rs` | 140 | 0 | 140 |
| 3 | `ws_client.rs` + `main.rs` wiring | `firmware/src/ws_client.rs`, `firmware/src/main.rs` | 33 | 2 | 35 |
| 4 | README note (es) | `README.md` | 12 | 0 | 12 |
| | **Total** | | **255** | **2** | **257** |

- `openspec/` artifacts are **excluded** from the review budget.
- `firmware/sdkconfig.defaults` is **unchanged (0 lines)**; no `Cargo.toml` change (serde/serde_json
  were pre-declared in the prior slice and now actually link).
- Code-only subtotal (excluding the `README.md` note): 243 additions + 2 deletions = **245**.
- This resolves design §12's forecast (≈255, range 220–300) to a committed **257 changed lines**,
  which is **under** the 400-line budget with **no overflow**. Single-PR delivery, `ask-on-risk`,
  **no chaining**, no `size:exception`. At apply time, if the actual diff exceeds 400, pause per
  `ask-on-risk` (do not auto-chain or infer an exception).

---

## Task 1 — `command.rs` pure parse + mixing math

Touches: `firmware/src/command.rs` (new). Changed lines: **+70 / −0 = 70**.

Covers acceptance criteria: AC4 (mixing math), AC9 (envelope parse/validate). Spec: R1, R2
(mixing/deadband/normalization steps 1–4). No hardware, no I/O, no `use crate::motors`.

Adds (exact public items from design §2): `pub const DEADBAND: f32 = 0.05`,
`pub struct SideSpeeds { left, right }`, `pub enum Cmd { Move { v, omega }, Stop }`,
`pub enum CommandError { MalformedJson, WrongShape, UnknownType, InvalidValue }`,
`pub fn parse(s: &str) -> Result<Cmd, CommandError>`, `pub fn mix(v: f32, omega: f32) -> SideSpeeds`.
Private serde shapes (§7.1): `Envelope { r#type: String, payload: Option<serde_json::Value> }` and
`MovePayload { v: f32, omega: f32 }` (no `deny_unknown_fields`; `id`/`ts`/`robot_id` tolerated).

- [x] Add `mod command;` to `firmware/src/main.rs` (temporary one-liner, consolidated in Task 3) and create `firmware/src/command.rs`. <!-- sdd-owner: implementation -->
- [x] Implement `mix()` in design §5 order: (1) deadband on inputs (both `|·| < DEADBAND` → coast `{0.0, 0.0}`; else zero the near-zero axis), (2) `left = v - omega`, `right = v + omega`, (3) ratio-preserving normalize by `max(|left|, |right|)` when `> 1.0`, (4) clamp each side to `[-1.0, 1.0]`. <!-- sdd-owner: implementation -->
- [x] Implement `parse()` per design §7.2: `from_str::<Value>` fail → `MalformedJson`; `from_value::<Envelope>` fail → `WrongShape`; `"cmd:move"` → `payload.ok_or(WrongShape)?` + `from_value::<MovePayload>` fail → `WrongShape`, then `!is_finite() || |v| > 1.0 || |omega| > 1.0` → `InvalidValue`; `"cmd:stop"` → `Ok(Cmd::Stop)`; anything else → `UnknownType`. <!-- sdd-owner: implementation -->
- [x] Verify cross-compile: `$(ls -d /nix/store/*-esp32-robot-firmware-fhs/bin/*firmware-fhs | head -1) -c 'cd firmware && cargo build'` — serde/serde_json link for `xtensa-esp32-espidf` (AC1). <!-- sdd-owner: implementation -->
- [x] Inspect: no `unwrap`/`expect`/`panic!` on wire-derived bytes; no `use crate::motors`; worked examples in design §5 (straight forward, spin left, normalization `0.333/1.0`, both-deadband coast, near-zero-omega zeroing) hold by inspection of the step order. <!-- sdd-owner: implementation -->

## Task 2 — `motors.rs` LEDC driver + motor task (six stop triggers)

Touches: `firmware/src/motors.rs` (new). Changed lines: **+140 / −0 = 140**.

Covers acceptance criteria: AC2 (LEDC resource plan), AC3 (`get_max_duty()`), AC5 (direction map +
invert consts), AC6 (`MAX_SPEED = 0.7`), AC7 (six stop triggers). Spec: R3, R4, R5, R6 (single
owner of LEDC/PWM state).

Adds (exact public items from design §2): `pub enum MotorSignal { Text(String), ConnectionLost }`,
`pub type MotorSender = std::sync::mpsc::Sender<MotorSignal>`,
`pub const MAX_SPEED: f32 = 0.7;`, `pub const INVERT_LEFT: bool = false;`,
`pub const INVERT_RIGHT: bool = false;`, `pub const DEADMAN: Duration = Duration::from_millis(1000);`,
`pub struct Motors { … }`, and
`pub fn init(ledc: LEDC, gpio12: Gpio12<'static>, gpio13: Gpio13<'static>, gpio14: Gpio14<'static>, gpio15: Gpio15<'static>) -> Result<Motors, EspError>`,
`pub fn run(rx: Receiver<MotorSignal>, motors: Motors)`. `Motors` holds
`in1_left`/`in2_left`/`in1_right`/`in2_right` `LedcDriver` fields plus `_timer` declared **last**
(design §4.2 drop-order: drivers then timer). `motors.rs` imports `crate::command` (calls `mix`).

- [x] Add `mod motors;` to `firmware/src/main.rs` (temporary one-liner, consolidated in Task 3) and create `firmware/src/motors.rs`. <!-- sdd-owner: implementation -->
- [x] Implement `init()`: one `LedcTimerDriver::new(ledc.timer0, &TimerConfig::new())?` (1 kHz/8-bit, per design §4.1 — no units import, no builder call), then four `LedcDriver::new(ledc.channelN, &timer, gpioM)?` with the frozen map: LEFT IN1=ch0→GPIO12, LEFT IN2=ch1→GPIO13, RIGHT IN1=ch2→GPIO15, RIGHT IN2=ch3→GPIO14 (all duty 0 = coast). Pass `&timer` (blanket `Borrow`, no `Rc`). <!-- sdd-owner: implementation -->
- [x] Implement `apply(v, omega) -> bool`: `command::mix` → clamp each side to `[-MAX_SPEED, MAX_SPEED]` (after normalize, before duty) → apply `INVERT_LEFT`/`INVERT_RIGHT` (negate sign last) → duty `((speed.abs() * get_max_duty() as f32).round() as u32).min(get_max_duty())` → direction encode (design §6: `s > 0` IN1=0/IN2=duty; `s < 0` IN1=duty/IN2=0; `s = 0` both 0) → return `left != 0.0 || right != 0.0`. <!-- sdd-owner: implementation -->
- [x] Implement `stop()`: `set_duty(0)` on all four drivers (coast, idempotent); `set_duty` errors swallowed with `warn!` (thread cannot propagate `EspError`). <!-- sdd-owner: implementation -->
- [x] Implement `run(rx, motors)`: `moving = false`; loop on `rx.recv_timeout(DEADMAN)` — `Ok(Text)` → `parse` → `Move`→`apply`, `Stop`→`stop`+`moving=false`, `Err`→`warn!`+`stop`+`moving=false`; `Ok(ConnectionLost)`→`stop`; `Err(Timeout)`→ stop only `if moving`; `Err(Disconnected)`→`stop`+`warn!`+`thread::park()` (design §8.1). All six triggers converge on one `stop()`. <!-- sdd-owner: implementation -->
- [x] Verify cross-compile: `$(ls -d /nix/store/*-esp32-robot-firmware-fhs/bin/*firmware-fhs | head -1) -c 'cd firmware && cargo build'` — `esp_idf_hal::ledc` types resolve; `Motors` is `Send + 'static` and moves into `thread::spawn`. <!-- sdd-owner: implementation -->
- [x] Inspect: duty computed from `get_max_duty()` (never a literal 255/256); `_timer` field declared last; no `Rc<LedcTimerDriver>`; `&timer` shared across all four channels; no `embassy-executor`; no `unwrap`/`expect`/`panic!` in the task loop. <!-- sdd-owner: implementation -->

## Task 3 — `ws_client.rs` forwarding + `main.rs` wiring

Touches: `firmware/src/ws_client.rs` (edit, +15/−1) and `firmware/src/main.rs` (edit, +20/−1 —
consolidates the two `mod` declarations). Changed lines: **+33 / −2 = 35**.

Covers acceptance criteria: AC8 (no outbound send, non-blocking callback), AC1 (full cross-compile).
Spec: R6 (callback forwarding, single owner of LEDC), R5 trigger 2 wiring.

- [x] In `firmware/src/ws_client.rs`, change `start()` signature to `pub fn start(uri: &str, cfg: &EspWebSocketClientConfig<'static>, tx: EventSender, motor_tx: MotorSender) -> Result<EspWebSocketClient<'static>, EspIOError>`, importing `crate::motors::{MotorSender, MotorSignal}`. <!-- sdd-owner: implementation -->
- [x] In the callback, keep `log_event(event)` + `tx.send(NetEvent::WsEvent).ok()`, and **additionally**, only on `Ok(event)`: `Text(s)` → `motor_tx.send(MotorSignal::Text(s.to_string())).ok()`; `Disconnected`/`Close(_)`/`Closed` → `motor_tx.send(MotorSignal::ConnectionLost).ok()`; everything else → nothing (design §8.2). Both sends unbounded + `.ok()` (never block). <!-- sdd-owner: implementation -->
- [x] In `firmware/src/main.rs`, consolidate `mod command;`/`mod motors;`, then wire per design §8.3 boot order: `(motor_tx, motor_rx) = mpsc::channel::<MotorSignal>()` → `motors::init(peripherals.ledc, peripherals.pins.gpio12, gpio13, gpio14, gpio15)?` → `thread::spawn(move || motors::run(motor_rx, motors))` **before** `wifi::init`/`wifi::connect` → pass `motor_tx.clone()` into `ws_client::start`. `net.rs` and the `rx.recv()` loop stay byte-for-byte unchanged. <!-- sdd-owner: implementation -->
- [x] Verify cross-compile: `$(ls -d /nix/store/*-esp32-robot-firmware-fhs/bin/*firmware-fhs | head -1) -c 'cd firmware && cargo build'` — full crate compiles for `xtensa-esp32-espidf`. <!-- sdd-owner: implementation -->
- [x] Inspect: **zero** `EspWebSocketClient::send*` call sites anywhere in the diff; callback never blocks; parsing/stop decisions live only in `motors::run` (not the callback); `main` holds `motor_tx` for the program lifetime. <!-- sdd-owner: implementation -->

## Task 4 — README firmware note (Spanish)

Touches: `README.md` (firmware section, `user_facing_docs: es`). Changed lines: **+12 / −0 = 12**.

Covers acceptance criteria: AC13 (non-goals + README note), spec R7 (documented USB/battery rule).

Adds a short Spanish note: the **hard rule** "nunca USB y batería a la vez; flashea por USB →
desconecta USB → conecta batería"; battery-only has no serial console (verification via physical
observation + hub `/robots`); and the direction correction path via `INVERT_LEFT`/`INVERT_RIGHT`.

- [x] Add the Spanish note to the `README.md` firmware section covering the USB/battery hard rule, the battery-only no-serial-console caveat, and the `INVERT_*` direction-correction path. <!-- sdd-owner: implementation -->
- [x] Inspect: text is Spanish and consistent with `user_facing_docs: es`; no credentials introduced; no motor-driving-on-USB guidance that contradicts R7. <!-- sdd-owner: implementation -->

---

## Keep-out checklist (verify during apply diff review)

- [x] Zero outbound `send()` / `EspWebSocketClient::send*` call sites (receive-only holds: no `register`, `telemetry`, `pong`, heartbeat). <!-- sdd-owner: implementation -->
- [x] No camera code, no telemetry publishing, no SNTP, no TLS/WSS, no auth code. <!-- sdd-owner: implementation -->
- [x] No NVS/config extension for motor tuning (constants only: `MAX_SPEED`/`INVERT_*`/`DEADMAN`/`DEADBAND`). <!-- sdd-owner: implementation -->
- [x] No `server/`, web UI, partition-table, or brownout-detector change. <!-- sdd-owner: implementation -->
- [x] `firmware/sdkconfig.defaults` is byte-identical (0 diff). <!-- sdd-owner: implementation -->
- [x] No `embassy-executor` anywhere (blocking loop, like the prior slice); `firmware/Cargo.toml` unchanged. <!-- sdd-owner: implementation -->

---

## On-device verification sequence (final, once)

> FHS wrapper: `WRAPPER="$(ls -d /nix/store/*-esp32-robot-firmware-fhs/bin/*firmware-fhs | head -1)"`.
> **Hard rule: never USB + battery at the same time** (this rig has no protection). Flash via USB →
> disconnect USB → connect battery.

### Phase A — USB bench (motors **disconnected**, or L298N motor power off with wheels off the ground)

- [x] Build + flash: `$WRAPPER -c 'cd firmware && cargo build'` then `$WRAPPER -c 'cd firmware && espflash flash --port /dev/ttyUSB0 --baud 115200 target/xtensa-esp32-espidf/debug/esp32-robot-firmware'`. Record `.bin` size (AC12; expect ~1.37–1.45 MB, ~34–36% of 4 MB). Chip ends in download mode — boot via RST button or a USB power cycle. <!-- sdd-owner: implementation -->
- [x] Capture serial strap-safely: `$WRAPPER -c 'python .pi/skills/esp32-ondevice-workflow/assets/serial_capture.py --reset'` (release DTR/RTS, RTS-pulse reset); expect `config resolved`, `wifi:connected`, `sta ip`, `WebSocket connected`. <!-- sdd-owner: implementation -->
- [x] Send one `cmd:move {v, omega}` via the hub (or a test client) and observe the logged computed left/right duty and sign (motors unloaded; no motors driven on USB power). <!-- sdd-owner: implementation -->
- [x] Verify the three software stops with motors unloaded: (a) **disconnect** — kill the hub / close the client → `ConnectionLost` → stop log; (b) **malformed frame** — send non-JSON → `warn!` + coast log; (c) **dead-man** — send one `cmd:move`, then silence → stop log after ~1 s. <!-- sdd-owner: implementation -->
- [x] Optional: scope GPIO12–15 (logic analyzer/multimeter) to confirm 1 kHz PWM and the forward/backward IN1/IN2 encoding. No battery connected. — **NO EJECUTADO (2026-09-14)**: sin instrumento disponible en el banco; disposición del mantenedor: la validación conductual on-device (sentidos L1/L2/R1/R2, dead-man ~1 s, corte de enlace, reconexión) cubre el comportamiento observable de este ítem opcional. <!-- sdd-owner: implementation -->

### Phase B — battery (maintainer, **no serial console**)

- [x] Maintainer: flash via USB → **disconnect USB** → **connect battery** (never both). <!-- sdd-owner: implementation -->
- [x] Observe physically (and via a LAN workstation, not the robot): forward / back / spin-in-place behave as expected, and direction sense matches `+v`=forward, `+omega`=left (CCW). <!-- sdd-owner: implementation -->
- [x] Observe **stop-on-disconnect** (close web client / kill hub → robot stops) and **dead-man** (release controls → stops in ~1 s). <!-- sdd-owner: implementation -->
- [x] Confirm the robot appears in hub **`/robots`** (`curl http://<hub-ip>:8080/robots`). <!-- sdd-owner: implementation -->
- [x] Confirm the board **boots with motors wired** — normal boot log (not silent): GPIO12 (MTDI) strapping check. <!-- sdd-owner: implementation -->
- [x] If physical forward drives the car backward (or one side is reversed): maintainer reports which side; implementer flips the affected `INVERT_LEFT`/`INVERT_RIGHT` const (`false → true`) and re-flashes (never change the frozen convention). <!-- sdd-owner: implementation -->
- [x] Maintainer **report-back** (required evidence): (a) forward/back/turn direction sense vs expectation (or which side is inverted); (b) stop-on-disconnect observed; (c) dead-man stop ~1 s observed; (d) robot listed in hub `/robots`; (e) normal boot log with motors wired; (f) which `INVERT_*` consts were flipped, if any. <!-- sdd-owner: implementation -->
\n> **Resultados on-device (2026-09-14, ejecutados):** ver `apply-progress.md` § "On-device verification (EJECUTADA)". Resumen: fase A OK (flash 1,385,088 bytes; boot; WS; rechazo `InvalidValue` con coast); fase B: L1/L2 invertidos → `INVERT_LEFT = true` + re-flash + re-validación OK; dead-man ~1 s cronometrado; corte de enlace con parada casi instantánea (marcador sonoro) y reconexión automática.

## Work-unit commit mapping (Spanish conventional style)

Each task maps to one work-unit commit so a reviewer can read the story in order:
`Implementar comando (parse + mezcla)`, `Implementar motores (LEDC + tarea)`,
`Cablear WS + main (motores)`, `Docs: regla USB/batería y verificación`. Rollback per task reverts
only that commit.
