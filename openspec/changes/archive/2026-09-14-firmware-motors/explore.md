# Explore — firmware-motors

> SDD Phase 1 slice (completes the robot side of roadmap Phase 1 — LAN teleoperation):
> the ESP32 firmware parses `cmd:move` / `cmd:stop` WebSocket commands from the hub and
> drives the 4WD car's L298N motor driver over LEDC PWM. Safety-first: the robot must
> stop on disconnect, parse error, invalid command, and command timeout (dead-man).
>
> OUT of scope: camera/video, telemetry publishing, SNTP, TLS/WSS, auth, any outbound
> WS send (the no-send keep-out from `firmware-wifi-ws-client` persists).

Status: exploration complete. This is context/notes only; no implementation.

---

## 1. Goal

Make the ESP32-CAM 4WD car *move* under hub control. The previous slice
(`firmware-wifi-ws-client`) gave the firmware its network life: WiFi STA + a WebSocket
client to the hub, receiving events but sending nothing. This slice adds the first real
application behavior on the receiving path: parse the envelope v0 `cmd:move` /
`cmd:stop` commands and turn them into safe L298N PWM output via the LEDC peripheral,
with a hard safety policy (stop on any abnormal condition).

The robot is battery-powered in real use (no serial console), so correctness of the
software path must be provable on the bench (USB, motors disconnected) and the physical
behavior verified by the maintainer's observation plus hub state (`/robots`).

## 2. Current-state summary

### 2.1 Firmware (from `firmware-wifi-ws-client`, applied and on-device verified)

- `firmware/Cargo.toml`: `esp-idf-svc 0.52.1` (features `critical-section`,
  `embassy-time-driver`, `embassy-sync`), `embassy-time 0.5`, `log 0.4`,
  `serde 1 (derive)`, `serde_json 1` — **both serde crates declared but unreferenced**
  (pre-provisioned for this exact slice). `extra_components` injects
  `espressif/esp_websocket_client` 1.1.0. No LEDC/GPIO dependency change is required:
  `esp_idf_svc::hal` re-exports `esp-idf-hal 0.46.2`, which already provides
  `ledc::LedcDriver` / `ledc::LedcTimerDriver` (LEDC is an on-by-default IDF component
  for esp32; no Kconfig/Cargo change).
- `firmware/src/main.rs`: `link_patches()` → logger → `Peripherals::take()` →
  `EspSystemEventLoop` → NVS → `config::resolve` → `mpsc::channel::<NetEvent>()` →
  `wifi::init` → `wifi::connect` → `ws_client::start` → `loop { rx.recv() }`
  (`WifiStaDisconnected` → reconnect, `WsEvent` → no-op, `Err` → park).
- `firmware/src/net.rs`: `enum NetEvent { WifiStaDisconnected, WsEvent }` +
  `type EventSender = std::sync::mpsc::Sender<NetEvent>` (unbounded channel).
- `firmware/src/ws_client.rs`: callback-style `EspWebSocketClient`; the callback runs on
  a hidden ESP-IDF task, logs every event, and does `tx.send(NetEvent::WsEvent)`.
  **Inbound `Text` frames are currently ignored** (`debug!` only). `buffer_size = 1024`
  caps WS frame size, which also bounds any JSON parse in the callback.
- `firmware/src/config.rs`: NVS namespace `"robot"`, string keys `wifi.ssid`,
  `wifi.password`, `hub.url`, `hub.robot_id`; NVS → compiled-default resolution with
  first-boot write-back.
- `firmware/sdkconfig.defaults`: only stack sizes; **must stay functionally unchanged**
  (keep-out from the previous slice). No `partitions.csv` (default single-app table).

### 2.2 Server (hub)

- `server/src/message.rs` defines envelope v0: `{ type, id?, ts?, robot_id?, payload }`
  with `type ∈ { "cmd:move", "cmd:stop", "cmd:camera", "telemetry", "video", "config",
  "ping", "pong", "register", "robots:list", "error" }`. `payload` is free-form `Value`.
- `cmd:move` payload is `{ "v": <f32>, "omega": <f32> }` (per the `message.rs` doc
  comment); `cmd:stop` has no meaningful payload. **No server-side spec document defines
  the sign convention or range of `v`/`omega`** — the firmware must define and document
  it (see §4.1 and open question Q1).
- `server/src/ws.rs`: client→robot commands are relayed **verbatim** as raw JSON text
  (`RouteToRobot`); the hub extracts only `robot_id` for routing and never parses or
  rewrites the envelope. So the firmware receives `{"type":"cmd:move","id":…,"ts":…,
  "robot_id":"a1","payload":{"v":0.5,"omega":0.2}}` as a WS `Text` frame.

### 2.3 Reference C++ (same kit, hardware-validated)

`/tmp/esp-cam-robot-car/lib/CarRobot/src/`:

- `Motor.cpp`: `Motor(pin1, pin2)`; `forward()` = `analogWrite(pin1, 0)` +
  `analogWrite(pin2, speed)`; `backward()` = `analogWrite(pin1, speed)` +
  `analogWrite(pin2, 0)`; `setSpeed(0)` ⇒ both pins 0 (**coast**, not brake). Arduino
  `analogWrite` on ESP32 ≈ **1 kHz, 8-bit (0–255)** via LEDC.
- `RobotGPIO.hpp`: `RIGHT_MOTOR_CONTROL_PIN1 = 15`, `RIGHT_MOTOR_CONTROL_PIN2 = 14`,
  `LEFT_MOTOR_CONTROL_PIN1 = 12`, `LEFT_MOTOR_CONTROL_PIN2 = 13` (pin1 = IN1, pin2 = IN2
  per side).
- `DriveController.cpp`: `turnLeft` = right forward + left backward; `turnRight` = right
  backward + left forward; `goForward`/`goBackward` = both sides same direction; `stop`
  = both sides speed 0. This is the **spin-in-place** differential-drive convention.

## 3. Hardware facts (authoritative for this slice)

### 3.1 L298N wiring (reference-validated, must be ported unchanged)

| Side | IN1 (pin1) | IN2 (pin2) | forward (code) | backward (code) |
|------|------------|------------|----------------|-----------------|
| LEFT  | GPIO 12 | GPIO 13 | IN1=0, IN2=PWM | IN1=PWM, IN2=0 |
| RIGHT | GPIO 15 | GPIO 14 | IN1=0, IN2=PWM | IN1=PWM, IN2=0 |

- Per side: **PWM on one IN input while the other is LOW** (the reference uses no
  ENA/ENB PWM pin; the ENA/ENB jumpers are tied enable). Both LOW = **coast**; both
  HIGH (full duty) = **brake** (the reference never brakes — it only coasts).
- Two motors per side share one L298N channel (the kit's L298N drives left pair + right
  pair in parallel), so one PWM signal per IN pin still drives both wheels of that side.
- Physical forward/reverse depends on motor wiring (which motor wire goes to which
  L298N OUT terminal). The reference's "forward" label is validated on this kit, but the
  **physical forward direction must be re-confirmed by the maintainer** (see Q1).

### 3.2 CRITICAL hardware safety constraint (must land in artifacts)

This rig has **no protection against USB + battery connected at the same time**. Motor
tests on device: **flash via USB → disconnect USB → connect battery**. Battery-only =
**no serial console**, so on-device motor verification is by the maintainer's **physical
observation + hub state (`/robots`)**, not serial logs. Serial logs are available only
for USB-only benches with motors disconnected. If the maintainer fixes the electronics
later, the flow may change.

Also note: GPIO12 (MTDI) and GPIO15 (MTDO) are ESP32 **strapping pins** — see §4.6.

## 4. Findings and recommendations (per explore question)

### 4.1 Command model: differential-drive `{v, omega}`

**Recommendation: keep the server's differential-drive `{v, omega}`** and map it to two
side speeds in the firmware. Do **not** change the envelope to raw left/right speeds —
the envelope is already fixed by `server/src/message.rs`, and `{v, omega}` is the right
abstraction for a future web panel / AI agent (matches a unicycle model).

Mapping (spin-in-place, matching the reference `DriveController`):

```
left  = v - omega
right = v + omega
```

- `v`: linear speed, `[-1, 1]`, `+` = forward.
- `omega`: angular speed, `[-1, 1]`, `+` = turn left (CCW from above).
- Sanity: `v=0, omega=+1` → left `-1` (backward), right `+1` (forward) → spin left,
  exactly the reference `turnLeft`. ✓

Normalization (standard differential-drive clamp, preserves steering direction):

```
m = max(|left|, |right|)
if m > 1 { left /= m; right /= m; }
```

Deadband (recommend `ε = 0.05` on the inputs, before mixing):

- If `|v| < ε` and `|omega| < ε` → **coast stop** (both sides 0).
- Else zero only the near-zero axis (e.g. `|omega| < ε` → treat `omega = 0`), then mix.
- Apply the deadband to `v`/`omega` (inputs) rather than to the final left/right, so a
  slow in-place turn isn't clipped asymmetrically.

Range/validity: `v`/`omega` must be finite and in `[-1, 1]`; otherwise → stop + `warn!`
(see §4.3). Deserialize into a `struct MovePayload { v: f32, omega: f32 }` and validate.

**Sign convention is currently unvalidated** (the web panel is future; the server
defines no convention). The firmware must **document its convention** and expose it as a
single source of truth; the physical forward direction is resolved on hardware (Q1).

### 4.2 PWM scheme: LEDC channels/timers

Verified against vendored `esp-idf-hal 0.46.2` (`src/ledc.rs`) and the shipped
`ledc_simple`/`ledc_threads`/`ledc_fade` examples:

- **API**: `esp_idf_hal::ledc::{LedcDriver, LedcTimerDriver, config::TimerConfig}` (via
  `esp_idf_svc::hal::ledc`). Construction:
  `LedcDriver::new(channel, timer_driver, pin)` where `channel: LedcChannel`,
  `timer_driver: Borrow<LedcTimerDriver<..>>`, `pin: impl OutputPin + 'd`.
- **No `Peripheral`/unsafe `impl` needed for the pins.** The raw `Gpio12/Gpio13/Gpio14/
  Gpio15` types already implement `OutputPin` (marker trait via the `pin!` macro in
  `gpio.rs`). Pass `peripherals.pins.gpio12` etc. directly to `LedcDriver::new`.
- **Resource plan**: one timer + four channels, low-speed mode (`peripherals.ledc`, not
  `peripherals.hledc`). ESP32 exposes `timer0..timer3` and `channel0..channel7` in
  `ledc::LEDC`. Use `timer0` for all four channels (same frequency/resolution for every
  motor is correct and cheapest):
  - LEFT IN1 = channel0 → GPIO12; LEFT IN2 = channel1 → GPIO13
  - RIGHT IN1 = channel2 → GPIO15; RIGHT IN2 = channel3 → GPIO14
- **Timer sharing gotcha (important)**: `LedcTimerDriver`'s `Drop` calls `ledc_timer_rst`
  (resets the timer). To share one timer across four channels, create the timer **once**
  and pass `&timer_driver` to each `LedcDriver::new` (blanket `impl Borrow<T> for &T`),
  then store the timer alongside the four drivers. **Do NOT use `Rc<LedcTimerDriver>`**
  (the official `ledc_threads` example does): `Rc` is not `Send`, and we want to move
  the motor struct into a dedicated thread. Both `LedcDriver` and `LedcTimerDriver` are
  `unsafe impl Send`, and the pins/timer/channels are `'static`, so a struct holding
  1 timer + 4 drivers is `Send + 'static` and moves cleanly into `thread::spawn`.
- **Frequency**: **1 kHz** to match the reference's `analogWrite` (≈1 kHz) — validated
  on hardware. 20 kHz would silence motor whine but is untested on this L298N (BJTs have
  switching losses at higher frequency); keep 1 kHz, note 20 kHz as a later tuning knob.
- **Duty resolution**: **8-bit** (`TimerConfig::new()` default is `Resolution::Bits8`).
  **Gotcha**: `LedcDriver::get_max_duty()` returns **256** for 8-bit (LEDC's
  `max_duty = 1 << bits`; 256 == 100% duty), **not** 255. Do not hardcode 255 — compute
  `duty = (|speed| * max_duty) as u32` from `get_max_duty()`, and let `set_duty` clamp.
  (255 vs 256 is a <0.5% difference, but using `get_max_duty()` is resolution-robust and
  avoids the off-by-one surprise.)
- **Forward/reverse encoding** (per side, from §3.1):
  - forward: IN1 channel = 0, IN2 channel = duty
  - backward: IN1 channel = duty, IN2 channel = 0
  - stop (coast): IN1 = 0, IN2 = 0
  - (brake = IN1 = max, IN2 = max — **not used this slice**; coast matches the reference)

### 4.3 Safety / stop policy (critical) — all in scope

The motor task MUST own every stop decision in one place. Recommended policy (all six):

| Trigger | Action | In this slice? |
|---------|--------|----------------|
| Boot (before any command) | Motors start stopped (duty 0); no default motion | **Yes** (inherent; assert explicitly) |
| WS `Disconnected` / `Close(reason)` / `Closed` | Stop immediately (motors must not run without a command path) | **Yes** |
| WS parse error (invalid JSON / wrong shape) | Stop + `warn!` (fail-safe: one bad message halts motion) | **Yes** |
| Invalid command (out-of-range / non-finite / unknown `type`) | Stop + `warn!` | **Yes** |
| Command timeout (dead-man): no valid `cmd:move` for N ms | Stop | **Yes** (recommend N = **1000 ms**) |
| `cmd:stop` received | Stop | **Yes** |

Rationale: a teleop robot that keeps moving on disconnect/error is a physical hazard.
Stopping on parse error/invalid command is conservative (a single malformed frame halts
motion) but is the safe default for a first motor slice; clamping instead of stopping
can be a later refinement. The dead-man timeout (1000 ms default) covers a frozen hub /
browser / WiFi stall where no `Disconnected` event fires. "Stop" = **coast** (both IN
pins 0), matching the reference — brake (both full) is a possible later enhancement but
heats the L298N and complicates the slice.

### 4.4 Inbound message handling (routing to the motor task)

**Recommendation: a second, dedicated channel + a dedicated motor task thread.** Keep
`NetEvent` as-is (network-only); add a `MotorSignal` channel. The WS callback stays
minimal and does **not** parse JSON; all parsing and all stop decisions live in the
motor task.

```
// net.rs unchanged. New in motor.rs:
pub enum MotorSignal {
    Text(String),       // inbound WS text frame to parse
    ConnectionLost,     // WS Disconnected / Close / Closed
}
pub type MotorSender = std::sync::mpsc::Sender<MotorSignal>;
```

- `main` creates `(motor_tx, motor_rx) = mpsc::channel::<MotorSignal>()`, builds the
  `Motors` (LEDC) struct, spawns `thread::spawn(move || motor::run(motor_rx, motors))`,
  and passes `motor_tx.clone()` to `ws_client::start` (signature gains a `MotorSender`).
- The WS callback (hidden ESP-IDF task) does, in addition to current logging:
  - `WebSocketEventType::Text(s)` → `motor_tx.send(MotorSignal::Text(s.to_string()))`
  - `Disconnected` / `Close(_)` / `Closed` → `motor_tx.send(MotorSignal::ConnectionLost)`
  - `tx.send(NetEvent::WsEvent)` unchanged (wakes main; harmless no-op there).
- `motor::run` blocks on `rx.recv_timeout(DEADMAN)` (unbounded `mpsc::Sender::send` is
  non-blocking, so the callback never blocks; `recv_timeout` gives the dead-man tick for
  free):

```
loop {
    match rx.recv_timeout(DEADMAN) {
        Ok(MotorSignal::Text(s))    => match command::parse(&s) {
            Ok(Cmd::Move { v, omega }) => { apply(motors, v, omega); last_move = Instant::now(); }
            Ok(Cmd::Stop) | Err(_)      => { stop(motors); log (warn on Err); }
        },
        Ok(MotorSignal::ConnectionLost) => stop(motors),
        Err(Timeout)  => if last_move.elapsed() > DEADMAN { stop(motors); },
        Err(Disconnected) => break,   // all senders gone; park
    }
}
```

- `Instant` / `recv_timeout` use the std time64 toolchain already in place (`espidf_time64`,
  `std::thread::sleep` already used in `wifi.rs`); if `recv_timeout` proves unreliable
  on-device, fall back to `recv()` + a `std::thread::sleep` poll — note this in tasks.
- Parsing in the motor task (not the WS callback) centralizes **every** stop trigger in
  one reviewable place and keeps serde out of the WS library task. `buffer_size = 1024`
  already bounds any single frame the callback would forward, so the parse is bounded.

### 4.5 Config: constants vs NVS

**Recommendation: keep PWM frequency, duty resolution, speed limit, and dead-man timeout
as `const`s in `motor.rs` for this slice.** NVS config for motor tuning is premature
(YAGNI), and the existing `robot` namespace stores strings only. If/when needed, `EspNvs`
already provides typed `get_u32`/`set_u32` (verified in esp-idf-svc 0.52.1 `nvs.rs`) for
a future `motor.deadman_ms` / `motor.max_speed` key — but not now. A speed-limit `const`
(e.g. `MAX_SPEED: f32 = 1.0`, or a reduced bring-up cap like `0.7`) is the only tuning
knob worth naming explicitly this slice.

### 4.6 GPIO 12 caveat (strapping pins)

- **GPIO12 = MTDI** (flash-voltage strapping): sampled at reset to choose VDD_SDIO
  (HIGH → 1.8 V flash, which can **fail boot**; LOW → 3.3 V, normal). The L298N module's
  IN inputs have pull-downs to GND (keeps motors off at power-on), so GPIO12 sits LOW at
  reset and boot is safe — **the reference drove GPIO12 on this exact kit and booted
  fine**. In firmware, LEDC config of GPIO12 happens well after boot, so the pin is only
  driven after the ROM has sampled it. **Risk to document, not a code change**: if
  wiring/pull-downs change, a floating or high GPIO12 at reset breaks boot.
- **GPIO15 = MTDO**: HIGH at reset → silent boot (no UART log). Pull-down keeps it LOW →
  normal log. Consistent with the previous slice's "MB/CH340 strap quirk" finding.
- **GPIO13 (MTCK) / GPIO14 (MTMS)** are JTAG pins, usable as GPIO with JTAG disabled
  (the default). No strapping issue.
- **Action**: keep the reference pin mapping unchanged; add an on-device check that the
  board boots with motors wired (boot log appears) and note the caveat in the proposal's
  risks and README.

### 4.7 Telemetry: out of scope, keep-out holds

Confirmed: telemetry is Phase 3. The no-send keep-out from the previous slice
persists: **zero `EspWebSocketClient::send(...)` call sites** — the firmware still only
*receives* (now it parses received commands, but never emits `register`, `telemetry`,
`pong`, or anything else). Note in the spec that the WS client remains receive-only.

### 4.8 On-device verification plan (USB/battery constraint)

Because USB+battery is forbidden and battery-only has no serial console, split
verification into two phases:

**Bench (USB only, motors DISCONNECTED or L298N motor power off, wheels off the ground):**
- Serial console IS available. Verify the full software path end-to-end without load:
  hub sends `cmd:move` → WS `Text` → motor task parse → `set_duty` → log the computed
  left/right duty. Optionally scope GPIO12–15 with a logic analyzer / multimeter to
  confirm the PWM (duty/frequency) and the forward/backward IN1/IN2 encoding. **Do not
  drive motors on USB power** (4 motors exceed USB current → brownout; and the brownout
  detector is intentionally untouched).
- Verify stop-on-disconnect (kill hub / close client → `ConnectionLost` → stop logged),
  stop-on-parse-error (send a malformed frame), and dead-man (send one `cmd:move`, then
  silence → stop after ~1 s) — all observable via logs without motors.

**Battery (physical observation; maintainer performs):**
- Flash via USB → **disconnect USB** → **connect battery**. No serial console.
- Verify: forward/back/turn spin-in-place physically; robot appears at hub `/robots`
  (from a workstation on the LAN, not the robot); stop-on-disconnect (close the web
  client / kill hub → robot stops); dead-man (release controls → robot stops in ~1 s);
  **physical forward direction** (record whether "forward" drives the car forward — see
  Q1); board boots with motors wired (GPIO12 strapping check).

**Maintainer's checklist** (goes into the proposal/tasks): flash → disconnect USB →
connect battery → observe motion → confirm direction sense → confirm stop-on-disconnect
+ dead-man → confirm hub `/robots` shows the robot.

### 4.9 Flash budget

Current app: **1,273,008 bytes (30.83%)** of 4 MB. This slice's growth:

- `serde_json` was declared-but-unused last slice (not linked); it now **links** —
  dominant cost, estimate **~80–200 KB** at `opt-level = "z"` (no LTO configured).
- `serde` derive for `MovePayload` (tiny); `esp_idf_hal::ledc` Rust driver (a few KB —
  the IDF `ledc` component is already in the default build); `motor.rs` + `command.rs`
  (~150–250 lines of code).
- **Estimate: +100–200 KB → ~1.37–1.45 MB (~34–36%)**, well within the flash budget.
  **Re-measure `.bin` during apply** (the exact serde_json cost is the only real
  unknown).

## 5. Open questions for the maintainer (numbered, actionable, with options)

1. **Physical forward direction.** The code-level "forward" (IN2 PWM) is ported verbatim
   from the reference, but the reference does not guarantee which physical direction the
   car moves. **Options:** (a) validate on battery and, if reversed, swap the two motor
   wires; (b) keep code as-is and accept a single `INVERT` const to negate per-side
   later; (c) validate and report before apply so the sign convention can be frozen in
   the spec. **Recommend (c).**
2. **`omega` sign convention.** `+omega = turn left` (CCW) is proposed and documented;
   the web panel (future) must agree. **Options:** (a) freeze `+omega = left` now;
   (b) `+omega = right`. **Recommend (a)** (matches the reference `turnLeft` convention).
3. **Dead-man timeout value.** Proposed **1000 ms** (responsive teleop, fast fail-safe).
   **Options:** 500 ms (stricter, may stop during brief command gaps), 1000 ms
   (recommended), 2000 ms (looser). **Recommend 1000 ms.**
4. **Deadband threshold.** Proposed `ε = 0.05`. **Options:** 0.02 (finer low-speed
   control), 0.05 (recommended, filters joystick noise), 0.10 (coarser). **Recommend 0.05.**
5. **Speed limit during bring-up.** **Options:** (a) full range `[-1,1]`; (b) a reduced
   `const MAX_SPEED = 0.7` cap for the first battery test. **Recommend (b)** for safety,
   then raise to (a) after the maintainer confirms control.
6. **Stop = coast vs brake.** Proposed **coast** (matches reference, no L298N heating).
   **Options:** (a) coast only; (b) add brake (both IN full) as a later `cmd:stop` mode.
   **Recommend (a)** now, (b) deferred.
7. **GPIO12 strapping risk acceptance.** Confirm the maintainer accepts driving GPIO12 as
   a motor PWM output (reference-validated on this kit), with the documented caveat that
   a wiring change could break boot. **Options:** accept (recommended) / require a
   pin remap (no spare pins on ESP32-CAM; camera occupies 0,2,4,5,18,19,21,22,23,25,26,
   27,32,34–36,39, and 4 is the flash LED — remap is not practical).

## 6. Risks

| Risk | Mitigation |
|------|------------|
| **USB+battery simultaneously** (no protection) → board damage | Hard rule in artifacts: flash via USB → disconnect USB → connect battery; never both. Maintainer-performed battery phase. |
| **Motors keep running on disconnect/error** (physical hazard) | Motor task owns all stop triggers (§4.3): disconnect, parse error, invalid, dead-man, `cmd:stop`, boot-stopped. |
| **GPIO12 (MTDI) strapping** breaks boot if pulled high at reset | L298N pull-downs keep it LOW (reference-validated); document caveat + on-device boot check (§4.6). No code change. |
| **GPIO15 (MTDO) silent boot** if high at reset | Pull-down keeps it LOW → normal boot log; consistent with prior strap quirk. |
| **`max_duty` = 256 not 255** (LEDC 8-bit) | Use `get_max_duty()` and compute duty from it; don't hardcode 255 (§4.2). |
| **Timer `Drop` resets shared LEDC timer** | Create one `LedcTimerDriver`, pass `&timer` to each channel, store it in the struct (no `Rc`, which isn't `Send`) (§4.2). |
| **Brownout on USB-powered motor tests** | Motors-disconnected bench only; battery phase uses battery power. Brownout detector untouched (prior keep-out). |
| **`recv_timeout`/`Instant` reliability on std time64** | Already time64-enabled; fallback to `recv()` + `sleep` poll if needed (§4.4). |
| **Physical direction reversed** (wiring) | Single source of truth + hardware confirmation (Q1); swap wires or negate in one const. |
| **Flash growth from linking serde_json** | Re-measure `.bin` during apply; expected ~34–36%, well within budget (§4.9). |
| **No serial console in battery mode** → limited observability | Bench-verify software path on USB; battery phase via physical observation + hub `/robots` (§4.8). |

## 7. Proposed scope boundary

### In scope (this slice)

- New `firmware/src/motor.rs`: `MotorSignal` enum + `MotorSender`, `Motors` struct
  (1 `LedcTimerDriver` + 4 `LedcDriver`, 1 kHz / 8-bit, pins per §3.1), `init`,
  `apply(v, omega)`, `stop`, and `run(rx, motors)` task loop with dead-man.
- New `firmware/src/command.rs` (or parse helpers inside `motor.rs`): envelope
  deserialization (`type` + `MovePayload { v, omega }`), `v`/`omega` validation +
  deadband, differential mixing + normalization, `cmd:move` / `cmd:stop` dispatch.
- `firmware/src/ws_client.rs`: extend the callback to forward `Text` →
  `MotorSignal::Text` and `Disconnected`/`Close`/`Closed` → `MotorSignal::ConnectionLost`
  (signature gains a `MotorSender`; still **no outbound send**).
- `firmware/src/main.rs`: build the motor channel + `Motors`, spawn the motor task, pass
  `motor_tx.clone()` to `ws_client::start` (extract `peripherals.ledc` + `peripherals.pins`
  for motors, `peripherals.modem` for WiFi).
- `serde`/`serde_json` now actually used (no Cargo.toml change needed).
- README note (Spanish, `user_facing_docs: es`): USB/battery rule + on-device motor
  verification flow.
- On-device verification: USB bench (software path + stop triggers) + maintainer battery
  phase (physical behavior, direction sense, `/robots`).

### Out of scope (explicit)

- Camera / MJPEG video; telemetry publishing (Phase 3); SNTP; TLS/WSS; auth.
- **Any outbound WS send** (`register`, `telemetry`, `pong`, heartbeat) — keep-out holds.
- NVS config for motor tuning (constants only, §4.5); brake mode; PID/closed-loop speed
  control; static IP/mDNS.
- `sdkconfig.defaults` functional change and any partition-table change (prior keep-outs).
- Brownout-detector modification (documented risk only).
- `server/` changes — none (the hub already relays `cmd:move`/`cmd:stop` verbatim).

## 8. Notes for the proposal/design phases

- Carry the exact LEDC resource plan (timer0 + channel0–3) and the `&timer` (not `Rc`)
  sharing pattern into design — this is the one place a reviewer can get burned.
- Carry the `max_duty = 256` (8-bit) quirk into design.
- Carry the motor-task ownership of all stop triggers into spec as hard requirements
  (the safety policy is the core value of this slice).
- Expected diff is small (~150–250 code lines across `motor.rs`, `command.rs`, edits to
  `ws_client.rs`/`main.rs`/`README.md`) — well under the 400-line review budget; single
  PR, `ask-on-risk`, no chaining anticipated.
