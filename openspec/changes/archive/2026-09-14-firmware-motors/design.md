# Design — firmware-motors

> SDD Phase 1 slice: the ESP32 firmware parses hub-relayed `cmd:move {v, omega}` /
> `cmd:stop` WebSocket `Text` frames and drives the 4WD car's L298N over LEDC PWM
> (timer0, 1 kHz, 8-bit), with differential mixing + deadband + `MAX_SPEED=0.7` and all
> six stop triggers centralized in one motor task (stop = coast).
>
> Backend: `openspec`. This document is the binding design for `sdd-tasks`.
> Proposal §4 decisions (the 7 pre-proposal decisions) and the spec `R1`–`R7` are binding
> and are NOT re-opened here. `explore.md` gotchas are frozen into this design.

Status: design (ready for `sdd-tasks`).

---

## 1. Overview

The robot has network life but no motion. This change adds two small modules — a pure
parse/math module (`command.rs`) and an LEDC driver + motor-task module (`motors.rs`) — and
wires a second channel into the existing WS callback and `main`. No `embassy-executor`
(blocking loop, like the prior slice); no new dependency (`serde`/`serde_json` were
pre-declared and now actually link).

```
boot → config::resolve → motors::init (LEDC built, all duties 0 = coast) → spawn motor task
     → wifi::init → wifi::connect → ws_client::start (callback forwards Text/loss → motor_tx)
     → main loop blocks on rx.recv() (NetEvent, unchanged)
```

Motors are initialized **stopped and before WiFi/WS start** (decision in §8.3). Data flow:
the WS callback forwards `Text(String)` / `ConnectionLost` over a **dedicated** `MotorSignal`
channel; the motor task owns every parse and every stop decision and all LEDC state. `main`'s
`NetEvent` loop is untouched.

---

## 2. Module layout and responsibilities

All under `firmware/src/`. `main.rs` gains `mod command;` and `mod motors;`.

| File | Responsibility | Public items (exact) |
|------|----------------|----------------------|
| `command.rs` (new) | Envelope v0 deserialization + validation + pure mixing math. No hardware, no I/O. Host-inspectable. | `pub const DEADBAND: f32 = 0.05;` `pub struct SideSpeeds { pub left: f32, pub right: f32 }` `pub enum Cmd { Move { v: f32, omega: f32 }, Stop }` `pub enum CommandError { MalformedJson, WrongShape, UnknownType, InvalidValue }` `pub fn parse(s: &str) -> Result<Cmd, CommandError>` `pub fn mix(v: f32, omega: f32) -> SideSpeeds` |
| `motors.rs` (new) | `MotorSignal` channel types, LEDC `Motors` struct (1 timer + 4 drivers), `init`, `apply`, `stop`, and `run` (the motor task owning every stop). | `pub enum MotorSignal { Text(String), ConnectionLost }` `pub type MotorSender = std::sync::mpsc::Sender<MotorSignal>` `pub const MAX_SPEED: f32 = 0.7;` `pub const INVERT_LEFT: bool = false;` `pub const INVERT_RIGHT: bool = false;` `pub const DEADMAN: Duration = Duration::from_millis(1000);` `pub struct Motors { … }` `pub fn init(ledc: LEDC, gpio12: Gpio12<'static>, gpio13: Gpio13<'static>, gpio14: Gpio14<'static>, gpio15: Gpio15<'static>) -> Result<Motors, EspError>` `pub fn run(rx: Receiver<MotorSignal>, motors: Motors)` |
| `ws_client.rs` (edit) | `start()` gains a `MotorSender` param; callback forwards `Text`/loss **in addition to** the existing `NetEvent::WsEvent` send. Still zero outbound sends. | `pub fn start(uri: &str, cfg: &EspWebSocketClientConfig<'static>, tx: EventSender, motor_tx: MotorSender) -> Result<EspWebSocketClient<'static>, EspIOError>` (only signature change; `build_uri`/`client_config`/`log_event` unchanged). |
| `net.rs` (unchanged) | — | No diff. |
| `main.rs` (edit) | Create motor channel + `Motors`, spawn motor task **before** WiFi/WS, pass `motor_tx.clone()` to `ws_client::start`. | `fn main() -> Result<(), EspError>` (wiring only). |
| `README.md` (edit, es) | USB/battery hard rule + on-device motor verification flow (Spanish, `user_facing_docs: es`). | — |

**`Motors` struct shape** (private fields; `init` is the only constructor):

```
pub struct Motors {
    in1_left:  LedcDriver<'static>,               // LEFT  IN1 = ch0 → GPIO12
    in2_left:  LedcDriver<'static>,               // LEFT  IN2 = ch1 → GPIO13
    in1_right: LedcDriver<'static>,               // RIGHT IN1 = ch2 → GPIO15
    in2_right: LedcDriver<'static>,               // RIGHT IN2 = ch3 → GPIO14
    _timer:    LedcTimerDriver<'static, LowSpeed>,// declared LAST so it drops after drivers
}
```

`command.rs` does **not** import `motors.rs`; `motors.rs` imports `crate::command` (calls
`command::mix`). `main.rs` wires both.

---

## 3. `net.rs` evolution decision

**`net.rs` is unchanged; a second dedicated `MotorSignal` channel is added** in `motors.rs` —
because motor-command lifecycle (dead-man clock, stop decisions) must not couple to the
network-lifecycle `NetEvent` loop in `main`, and `main`'s WiFi-reconnect handling stays
byte-for-byte untouched.

---

## 4. LEDC resource plan (frozen)

### 4.1 Timer + channels + pins

| Resource | Value |
|----------|-------|
| Timer | `ledc.timer0` (low-speed `LEDC` peripheral, `peripherals.ledc` — not `hledc`) |
| Timer config | `TimerConfig::new()` = **1 kHz, 8-bit** (verified: `const fn new()` returns `frequency: Hertz(1000)`, `resolution: Resolution::Bits8`). No builder call or units import needed; a code comment states "1 kHz, 8-bit". |
| Channels | `channel0..channel3` only |
| Pin types | `Gpio12<'static>`, `Gpio13<'static>`, `Gpio14<'static>`, `Gpio15<'static>` — each already `impl OutputPin` (via the `pin!` macro in `gpio.rs`), so they pass directly to `LedcDriver::new` with **no unsafe** and no `PinDriver` wrapper. |

| Side | IN1 | IN2 |
|------|-----|-----|
| LEFT  | channel0 → GPIO12 | channel1 → GPIO13 |
| RIGHT | channel2 → GPIO15 | channel3 → GPIO14 |

### 4.2 `&timer` sharing and lifetimes/ownership in `main.rs`

- `LedcTimerDriver::new(ledc.timer0, &TimerConfig::new())?` is created **once** in `init`.
- Each driver is built as `LedcDriver::new(ledc.channelN, &timer, gpioM)?` — the `&timer`
  satisfies the `B: Borrow<LedcTimerDriver<'d, LowSpeed>>` bound via the blanket
  `impl Borrow<T> for &T`.
- **Key verified fact** (vendored `ledc.rs`): `LedcDriver::new` **copies** `max_duty` and the
  timer number out of the timer and stores them in the driver's own fields; the driver holds
  **no reference to the timer after construction**. Therefore there is no self-referential
  borrow — the `Motors` struct is a flat struct, `Send + 'static` (both `LedcDriver` and
  `LedcTimerDriver` are `unsafe impl Send`), and it moves cleanly into `thread::spawn`.
- The timer must nevertheless **remain alive (un-dropped) for the whole loop**: its `Drop`
  calls `ledc_timer_rst`, which resets the shared timer and kills PWM for all four channels.
  That is why `_timer` is a field of `Motors` (not a local), and why it is declared **last**
  so Rust drops the four drivers (each `Drop` → `ledc_stop`) before the timer reset.
- `main.rs` moves `peripherals.ledc` and the four pins into `motors::init`, and
  `peripherals.modem` into `wifi::init` — disjoint fields, so all moves are legal.

### 4.3 Duty formula (frozen)

`max_duty = self.<driver>.get_max_duty()` → **256** for 8-bit (`1 << 8`; **not** 255). Duty is
always computed from it, never hardcoded:

```
duty = ((speed.abs() * max_duty as f32).round() as u32).min(max_duty)
```

`speed.abs() ≤ MAX_SPEED = 0.7`, so `duty ≤ 179` — the `.min(max_duty)` clamp and
`set_duty`'s own internal `duty.min(max_duty)` are defense-in-depth, never the source of truth.

---

## 5. Mixing + clamp + deadband order of operations (exact sequence)

`command::mix(v, omega) -> SideSpeeds` runs, in this order:

1. **Deadband on inputs** (`ε = DEADBAND = 0.05`), *before* mixing:
   - if `|v| < ε` **and** `|omega| < ε` → return `{ left: 0.0, right: 0.0 }` (coast).
   - else zero only the near-zero axis: `|v| < ε → v = 0.0`; `|omega| < ε → omega = 0.0`.
2. **Mix**: `left = v - omega`, `right = v + omega` (`+v` = forward, `+omega` = turn left/CCW).
3. **Ratio-preserving normalization**: `m = max(|left|, |right|)`; `if m > 1.0 { left /= m; right /= m; }`.
4. **Clamp each side to `[-1.0, 1.0]`** (`left.clamp(-1.0, 1.0)`, same for right) — no-op
   after step 3, kept as belt-and-suspenders per R2.

`motors::apply` then continues (the driver side):

5. **`MAX_SPEED = 0.7` bring-up clamp**: `left = left.clamp(-MAX_SPEED, MAX_SPEED)`, same for
   right — **after** mix/normalize (step 4), **before** duty (step 7). Documented raise path:
   `MAX_SPEED → 1.0` after the maintainer confirms control on battery.
6. **Invert consts** (last, before encode): `if INVERT_LEFT { left = -left; }`,
   `if INVERT_RIGHT { right = -right; }`.
7. **Duty** per §4.3, then **direction encode** per §6.

Worked examples (spec R2/R4, reproduced for the implementer):

| v | omega | deadband | raw L/R | normalized | `[-1,1]` clamp | `MAX_SPEED=0.7` clamp |
|---|-------|----------|---------|------------|----------------|----------------------|
| 1.0 | 0.0 | — | 1.0 / 1.0 | — | 1.0 / 1.0 | 0.7 / 0.7 |
| 0.0 | 1.0 | — | −1.0 / 1.0 | — | −1.0 / 1.0 | −0.7 / 0.7 |
| 1.0 | 0.5 | — | 0.5 / 1.5 | ÷1.5 → 0.333 / 1.0 | 0.333 / 1.0 | 0.333 / 0.7 |
| 0.02 | 0.01 | both `< ε` → coast | 0 / 0 | — | 0 / 0 | 0 / 0 |
| 0.8 | 0.02 | zero omega | 0.8 / 0.8 | — | 0.8 / 0.8 | 0.7 / 0.7 |

---

## 6. Direction encoding table (frozen)

Per side, given the final signed speed `s` (post-invert) and `duty` from §4.3:

| `s` | IN1 (pin1) | IN2 (pin2) | Meaning |
|-----|-----------|-----------|---------|
| `> 0` | `0` (low) | `duty` (PWM) | forward |
| `< 0` | `duty` (PWM) | `0` (low) | reverse |
| `= 0` | `0` (low) | `0` (low) | coast (stop) |

- LEFT  IN1 = channel0 → GPIO12, IN2 = channel1 → GPIO13.
- RIGHT IN1 = channel2 → GPIO15, IN2 = channel3 → GPIO14.
- `INVERT_LEFT` / `INVERT_RIGHT` (default **`false`**) negate the sign **immediately before**
  this table is applied (step 6 of §5) — the frozen forward/reverse convention itself never
  changes. Brake (both IN full) is **not** used.

---

## 7. Command model + parse rules (frozen)

### 7.1 Serde shapes

```
#[derive(Deserialize)] struct Envelope {
    #[serde(rename = "type")] r#type: String,
    payload: Option<serde_json::Value>,
}
#[derive(Deserialize)] struct MovePayload { v: f32, omega: f32 }
```

- Extra envelope fields (`id`, `ts`, `robot_id`) are **ignored** by serde's default behavior
  (no `deny_unknown_fields`), satisfying R1's tolerance clause.
- `cmd:stop` takes no meaningful payload: `payload` may be `{}` or absent; it is ignored.
- `cmd:move` requires `payload` present and shaped as `{v, omega}`.

### 7.2 Parse + validate flow (`command::parse`)

1. `serde_json::from_str::<Value>(s)` fails → `Err(MalformedJson)`.
2. `serde_json::from_value::<Envelope>(value)` fails (e.g. missing/typed-wrong `type`) →
   `Err(WrongShape)`.
3. Match `envelope.r#type.as_str()`:
   - `"cmd:move"` → `payload.ok_or(WrongShape)?` then
     `serde_json::from_value::<MovePayload>(payload)` fails → `Err(WrongShape)`; else validate:
     `!v.is_finite() || !omega.is_finite() || |v| > 1.0 || |omega| > 1.0` → `Err(InvalidValue)`;
     otherwise `Ok(Cmd::Move { v, omega })`.
   - `"cmd:stop"` → `Ok(Cmd::Stop)`.
   - anything else → `Err(UnknownType)`.

### 7.3 Clamp vs reject (explicit)

- **Reject → coast + `warn!`** (never clamp, never partial-apply): malformed JSON, wrong
  envelope/payload shape, unknown `type`, non-finite `v`/`omega` (NaN/±inf — reachable via
  f32 overflow from huge literals like `1e300`), and out-of-range `|v|>1` / `|omega|>1`.
  The `[-1,1]` range is a **validated contract**, not a clamp target.
- **Clamp** (the only clamps in the slice): deadband zeroing, normalization, the post-mix
  `[-1,1]` clamp, `MAX_SPEED=0.7`, and the duty `.min(max_duty)`.
- Every rejection terminates in `motors.stop()` (coast) per R5. `parse` itself is pure and
  panics nowhere on untrusted input (no `unwrap`/`expect`/`panic!` on the wire bytes).

### 7.4 Bounds

The WS `buffer_size = 1024` (frozen from the prior slice) caps any single frame the callback
forwards, so `parse` work is bounded by construction.

---

## 8. Motor task + safety loop

### 8.1 `motors::run` (exact loop shape)

`run` blocks forever on the motor channel. It keeps a single `moving: bool` (true while any
side has non-zero duty) so the dead-man `Timeout` arm coasts **only if currently moving**,
avoiding a standing 1 Hz write cadence after any stop.

```
run(rx, motors):
    moving = false
    loop {
        match rx.recv_timeout(DEADMAN) {                 // DEADMAN = 1000 ms
            Ok(Text(s)) => match command::parse(&s) {
                Ok(Cmd::Move { v, omega }) => { moving = motors.apply(v, omega); }
                Ok(Cmd::Stop)              => { motors.stop(); moving = false; }
                Err(e)                     => { warn!("invalid command {e:?}; coasting");
                                                motors.stop(); moving = false; }
            },
            Ok(ConnectionLost) => { motors.stop(); moving = false; }
            Err(Timeout)       => { if moving { warn!("dead-man: no cmd:move for 1s; coasting");
                                                motors.stop(); moving = false; } }
            Err(Disconnected)  => { motors.stop(); warn!("motor senders dropped; parking");
                                    thread::park(); }   // all senders gone (safety net)
        }
    }
```

- `motors.apply(v, omega) -> bool`: runs §5 steps 5–7, writes the four `set_duty` values, and
  returns `left != 0.0 || right != 0.0` (so a deadband-coast correctly clears `moving`).
- `motors.stop()`: `set_duty(0)` on all four drivers (coast); idempotent.
- `set_duty`/`config_with_pin` return `Result<_, EspError>`; inside the thread they are
  `.ok()`-swallowed or `if let Err(e) = … { log::warn!("set_duty failed: {e}") }` (the thread
  cannot propagate `EspError` to `main`). These are the only non-panicking error paths.

### 8.2 WS callback forwarding (non-blocking)

`ws_client::start` gains `motor_tx: MotorSender`. The callback keeps the existing
`log_event(event)` + `tx.send(NetEvent::WsEvent).ok()`, and **additionally**, only on
`Ok(event)`:

| `event.event_type` | Forward |
|--------------------|---------|
| `Text(s)` | `motor_tx.send(MotorSignal::Text(s.to_string())).ok()` |
| `Disconnected` / `Close(_)` / `Closed` | `motor_tx.send(MotorSignal::ConnectionLost).ok()` |
| everything else | nothing |

Both sends use an **unbounded** `std::sync::mpsc::Sender` (non-blocking) + `.ok()`, so the
callback never blocks. `Text(&'a str)` is converted to an owned `String` so the channel value
outlives the frame borrow. Parsing happens in the motor task, never here (R6).

### 8.3 Boot order (frozen)

**Motors first, network second.**

```
Peripherals::take() → sysloop → NVS → config::resolve → drop(nvs)
(tx, rx) = channel::<NetEvent>()
(motor_tx, motor_rx) = channel::<MotorSignal>()
motors = motors::init(peripherals.ledc, peripherals.pins.gpio12, gpio13, gpio14, gpio15)?   // PWM built, coasted
thread::spawn(move || motors::run(motor_rx, motors))                                         // motor task owns PWM + stops
wifi = wifi::init(peripherals.modem, …)? → wifi::connect(&mut wifi)?
_ws = ws_client::start(&uri, &client_config(), tx.clone(), motor_tx.clone())?                // callback → motor_tx
loop { rx.recv() → WifiStaDisconnected ⇒ reconnect | WsEvent ⇒ {} | Err ⇒ park }             // unchanged
```

Rationale: the pins are held at coast from the instant LEDC configures them (before any
command path exists), and the motor channel must exist before `ws_client::start` captures
`motor_tx`. The dead-man timer runs harmlessly during the (possibly long) WiFi connect.

### 8.4 Interaction with `NetEvent` / main loop

`net.rs`, `NetEvent`, and `main`'s `rx.recv()` loop are **byte-for-byte unchanged**. The
callback now sends on **two** channels; `NetEvent::WsEvent` still only wakes `main` (no-op).
`main` keeps `motor_tx` alive for the program's lifetime (its `rx.recv()` loop never returns),
so the motor task's `Disconnected` arm is a dead-code safety net, mirroring the existing
`Err(_) => park` in `main`.

---

## 9. Stop-trigger matrix (six triggers → one `stop()`)

| # | Trigger | Code path | Result |
|---|---------|-----------|--------|
| 1 | Boot | `motors::init` constructs all four drivers at duty 0; `run` starts with `moving=false` | coast; no default motion |
| 2 | WS disconnect | callback `Disconnected`/`Close`/`Closed` → `MotorSignal::ConnectionLost` → `run` `Ok(ConnectionLost)` arm | `motors.stop()` + `moving=false` |
| 3 | Parse error | `parse` → `Err(MalformedJson | WrongShape)` → `run` `Text` arm `Err` branch | `warn!` + `motors.stop()` + `moving=false` |
| 4 | Invalid command | `parse` → `Err(UnknownType | InvalidValue)` → same `Err` branch | `warn!` + `motors.stop()` + `moving=false` |
| 5 | Dead-man | `recv_timeout(1000 ms)` → `Err(Timeout)` with `moving=true` | `warn!` + `motors.stop()` + `moving=false` |
| 6 | `cmd:stop` | `parse` → `Ok(Cmd::Stop)` → `run` `Text` arm | `motors.stop()` + `moving=false` |

All six terminate in the single `motors.stop()` (both IN pins low = coast). After any stop, a
later valid `cmd:move` resumes normally via `apply` (R5 scenario).

---

## 10. On-device verification design

### 10.1 USB bench (serial console available; motors **disconnected or L298N motor power off, wheels off the ground**)

Safe state: **motors disconnected** (safest) or the L298N motor-power jumper/Vin removed with
wheels off the ground. **Never drive motors on USB power** (4 motors exceed USB current →
brownout; detector intentionally untouched).

Sequence: `cd firmware && cargo espflash flash` → open serial monitor → send, via the hub or a
test client, one `cmd:move` and observe the logged computed left/right duty and sign; then
verify the three software stops with motors unloaded:

- **disconnect**: kill the hub / close the client → `ConnectionLost` → stop log.
- **malformed frame**: send non-JSON → `warn!` + coast log.
- **dead-man**: send one `cmd:move`, then silence → stop log after ~1 s.

Optionally scope GPIO12–15 with a logic analyzer/multimeter to confirm 1 kHz PWM and the
forward/backward IN1/IN2 encoding. No battery connected.

### 10.2 Battery phase (maintainer; **no serial console**)

Hard rule: **flash via USB → disconnect USB → connect battery** (never both). Maintainer
observes physically and via a LAN workstation (not the robot):

- forward / back / spin-in-place turns behave as expected, and the **direction sense** matches
  `+v`=forward, `+omega`=left (CCW);
- **stop-on-disconnect** (close the web client / kill the hub → robot stops) and **dead-man**
  (release controls → stops in ~1 s);
- the robot appears in hub **`/robots`**;
- the board **boots with motors wired** — normal boot log (not silent): GPIO12 strapping check.

### 10.3 `INVERT_*` correction path

If physical "forward" drives the car backward (or one side is reversed), the maintainer
reports which side; the implementer flips the affected `INVERT_LEFT`/`INVERT_RIGHT` const
(`false → true`), re-flashes, and the maintainer re-observes. The frozen forward/reverse
convention and encoding are never changed.

### 10.4 Maintainer report-back (required evidence)

(a) forward/back/turn direction sense vs. expectation (or which side is inverted); (b)
stop-on-disconnect observed; (c) dead-man stop ~1 s observed; (d) robot listed in hub
`/robots`; (e) normal boot log with motors wired; (f) which `INVERT_*` consts were flipped,
if any (so the code change can be applied and re-flashed).

---

## 11. Flash budget + risks carried

### 11.1 Flash budget

Current app **1,273,008 bytes (30.83%)** of 4 MB. Growth: `serde_json` now actually links
(dominant, ~80–200 KB at `opt-level="z"`, no LTO) + `serde` derive (tiny) +
`esp_idf_hal::ledc` Rust driver (a few KB; the IDF `ledc` component is already in the default
build) + `motors.rs`/`command.rs` (~150–250 lines). **Estimate +100–200 KB → ~1.37–1.45 MB
(~34–36%)**, well within budget. **Re-measure `.bin` during apply** (serde_json cost is the
only real unknown).

### 11.2 Risks carried (from explore, now frozen)

| Risk | Carried mitigation |
|------|--------------------|
| **`max_duty` = 256, not 255** (LEDC 8-bit) | §4.3: always `get_max_duty()`; no hardcoded 255/256. |
| **Timer `Drop` resets the shared LEDC timer** | §4.2: one `LedcTimerDriver`, stored as `Motors._timer` (declared last), `&timer` passed to each channel, no `Rc`. |
| **GPIO12 (MTDI) strapping** | L298N IN pull-downs keep it LOW at reset → 3.3 V flash, normal boot (reference-validated on this kit). LEDC configures GPIO12 only after ROM sampling. A future wiring change that floats/pulls it high breaks boot — documented caveat + §10.2 boot check. No code change. |
| **GPIO15 (MTDO) silent boot if high** | Pull-down keeps LOW → normal log; consistent with prior strap quirk. |
| **Runaway motors (physical hazard)** | §9: all six stop triggers in one motor task; stop = coast. |
| **USB + battery together → board damage** | Hard rule in all artifacts: flash via USB → disconnect USB → connect battery; battery phase is maintainer-performed (§10). |
| **Physical direction reversed** | `INVERT_LEFT`/`INVERT_RIGHT` consts (§6, §10.3), never the convention. |
| **No serial console in battery mode** | Bench-verify the software path on USB (§10.1); battery phase via physical observation + hub `/robots` (§10.2). |
| **`recv_timeout` reliability on std time64** | Time64 already enabled; fallback to `recv()` + `thread::sleep` poll if `recv_timeout` proves unreliable on-device (note in tasks). |

**Safety rule (restated, binding):** never USB + battery at the same time; flash via USB →
disconnect USB → connect battery; all six stop triggers converge on one `stop()` = coast.

---

## 12. Expected diff size (feeds `sdd-tasks` review-budget forecast)

| File | Kind | Est. changed lines |
|------|------|--------------------|
| `firmware/src/command.rs` | new | ~70 |
| `firmware/src/motors.rs` | new | ~140 |
| `firmware/src/ws_client.rs` | edit | ~15 |
| `firmware/src/main.rs` | edit | ~20 |
| `README.md` (es note) | edit | ~12 |
| **Total** | | **≈ 255 (range 220–300)** |

Comfortably under the **400-line review budget**: single PR, `ask-on-risk`, **no chaining**,
no `size:exception`.

---

## 13. Decisions to carry into `sdd-tasks`

1. Implement in work-unit commits: (a) `command.rs` (pure, host-inspectable), (b) `motors.rs`
   (LEDC + task), (c) `ws_client.rs` + `main.rs` wiring, (d) README note — each cross-compiling.
2. `TimerConfig::new()` is exactly 1 kHz/8-bit; do not add a units import or a builder call.
3. `MAX_SPEED`/`INVERT_*`/`DEADMAN`/`DEADBAND` are named `const`s (no NVS extension this slice).
4. `Motors._timer` is declared **last** in the struct (drop order: drivers then timer).
5. No `unwrap`/`expect`/`panic!` on any wire-derived data; `send(…).ok()` on both channels.
6. README note is in Spanish; covers the USB/battery hard rule + §10 verification flow.
7. Re-measure `.bin` during apply and record the number (acceptance criterion 12).
