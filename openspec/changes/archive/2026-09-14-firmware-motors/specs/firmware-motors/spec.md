# firmware-motors Specification

> Change: `firmware-motors`. New domain (no canonical spec exists yet).
> This spec describes WHAT must be true after the firmware parses hub-relayed
> `cmd:move {v, omega}` / `cmd:stop` WebSocket commands and drives the L298N motor
> driver over LEDC PWM, with a hard safety policy (stop on any abnormal condition).
> It completes the robot side of roadmap Phase 1 LAN teleoperation. Implementation
> details (exact struct names, loop shape, wiring) belong to the design/tasks phases.

## Purpose

Give the ESP32-CAM 4WD car the ability to *move* under hub control. The firmware
receives envelope v0 `cmd:move` / `cmd:stop` commands as WebSocket `Text` frames
(relayed verbatim by the hub), parses them with `serde`/`serde_json`, maps the
differential-drive `{v, omega}` inputs to per-side speeds, and actuates the L298N
via LEDC PWM (1 kHz, 8-bit). Safety is the core value of this slice: a single
motor task owns every stop decision — boot, WebSocket disconnect, parse error,
invalid command, a 1000 ms dead-man timeout, and explicit `cmd:stop` — and
"stop" always means coast (both IN pins low). The firmware remains receive-only;
it sends zero WebSocket frames.

## Requirements

### Requirement R1: Command intake (envelope v0 parsing)

Rationale: the hub relays `cmd:move`/`cmd:stop` verbatim as JSON `Text` frames; the
firmware must recognize them and fail safe on anything else, using the already-declared
`serde`/`serde_json` dependencies.

The firmware MUST deserialize the v0 envelope `{type, payload}` from inbound
WebSocket `Text` frames using `serde`/`serde_json`. It MUST recognize `cmd:move`
with a payload `{v: f32, omega: f32}` and `cmd:stop` (no meaningful payload). It
MUST tolerate the envelope's other fields (`id`, `ts`, `robot_id`) when present.
It MUST validate that `v` and `omega` are finite and within `[-1, 1]`; a non-finite
or out-of-range value MUST be treated as an invalid command (coast, per R5). Malformed
JSON, a wrong envelope shape, an unknown `type`, and invalid values MUST be handled
without panicking, and MUST result in a coast stop and a `warn!` log (malformed JSON,
wrong shape, unknown `type`, and out-of-range) — never undefined motor behavior.

> Verification: host-inspectable (serde derives + parse/validate path in
> `command.rs`; no `unwrap`/`expect`/`panic!` on untrusted input) + cross-compile
> (`serde`/`serde_json` link and resolve).

#### Scenario: Valid move command parses

- GIVEN an inbound WS `Text` frame `{"type":"cmd:move","id":"c1","ts":1,"robot_id":"a1","payload":{"v":0.5,"omega":0.2}}`
- WHEN the motor task parses it
- THEN it recognizes `cmd:move` with `v = 0.5` and `omega = 0.2`, ignoring `id`/`ts`/`robot_id`

#### Scenario: Valid stop command parses

- GIVEN an inbound WS `Text` frame `{"type":"cmd:stop","payload":{}}`
- WHEN the motor task parses it
- THEN it recognizes `cmd:stop`

#### Scenario: Malformed JSON is rejected safely

- GIVEN an inbound WS `Text` frame that is not valid JSON
- WHEN the motor task parses it
- THEN it logs a `warn!` and coasts the motors, without panicking

#### Scenario: Unknown type is rejected safely

- GIVEN an inbound WS `Text` frame with a `type` outside `cmd:move`/`cmd:stop` (e.g. `cmd:camera`)
- WHEN the motor task parses it
- THEN it logs a `warn!` and coasts the motors, without panicking

#### Scenario: Non-finite or out-of-range values are rejected

- GIVEN a `cmd:move` payload whose `v` or `omega` is `NaN`, infinite, or outside `[-1, 1]`
- WHEN the motor task validates it
- THEN it treats the command as invalid, logs a `warn!`, and coasts the motors

### Requirement R2: Differential-drive mixing with deadband and normalization

Rationale: the server envelope is frozen to a unicycle `{v, omega}` model; the firmware
maps it to per-side speeds with a deadband to filter noise and normalization to preserve
steering direction, and freezes `+omega = turn left (CCW)`.

The firmware MUST compute side speeds as `left = v - omega` and `right = v + omega`.
`+v` MUST mean forward and `+omega` MUST mean turn left (CCW from above), matching the
reference `turnLeft`. Before mixing, a deadband `ε = 0.05` MUST be applied to the
**inputs**: when both `|v| < ε` and `|omega| < ε`, the firmware MUST coast (both sides
0); otherwise only the near-zero axis is zeroed (`|omega| < ε` → treat `omega = 0`;
`|v| < ε` → treat `v = 0`), then the mix proceeds. When `max(|left|, |right|)` exceeds
1, the firmware MUST normalize both by that maximum so the ratio (steering direction) is
preserved. After mixing, the firmware MUST clamp each side speed to `[-1, 1]`.

> Verification: host-inspectable (mixing + deadband + normalization math in
> `command.rs`/`motors.rs`; sign convention documented as single source of truth).

#### Scenario: Straight forward

- GIVEN `v = 1.0` and `omega = 0.0`
- WHEN the mix is computed
- THEN `left = 1.0` and `right = 1.0` (both sides forward at full speed)

#### Scenario: Spin left (CCW)

- GIVEN `v = 0.0` and `omega = 1.0`
- WHEN the mix is computed
- THEN `left = -1.0` (backward) and `right = 1.0` (forward), producing a CCW spin-in-place

#### Scenario: Normalization preserves steering direction

- GIVEN `v = 1.0` and `omega = 0.5`
- WHEN the mix is computed
- THEN `left = 0.5` and `right = 1.5` is normalized by `1.5`, yielding `left ≈ 0.33` and `right = 1.0`

#### Scenario: Both axes within deadband coast

- GIVEN `|v| < 0.05` and `|omega| < 0.05`
- WHEN the inputs are processed
- THEN the motors coast (both sides 0) regardless of mixing

#### Scenario: Near-zero omega axis is zeroed

- GIVEN `v = 0.8` and `|omega| < 0.05`
- WHEN the inputs are processed
- THEN `omega` is treated as 0 and the mix yields `left = 0.8`, `right = 0.8` (straight, not a clipped turn)

### Requirement R3: LEDC PWM actuation with the frozen pin map

Rationale: ports the reference C++ PWM scheme (1 kHz, 8-bit, coast-on-stop) to the Rust
`esp_idf_hal::ledc` driver using the frozen GPIO mapping and direction encoding.

The firmware MUST drive the L298N via one `LedcTimerDriver` on **timer0** at **1 kHz,
8-bit** resolution, shared by **channels 0–3**, mapped: LEFT IN1 = channel 0 → GPIO 12,
LEFT IN2 = channel 1 → GPIO 13, RIGHT IN1 = channel 2 → GPIO 15, RIGHT IN2 = channel 3
→ GPIO 14. The duty for a side MUST be computed from `LedcDriver::get_max_duty()` (which
returns **256** for 8-bit), never a hardcoded 255/256 constant. Direction encoding per
side MUST be: forward = IN2 PWM + IN1 low; reverse = IN1 PWM + IN2 low; stop = coast
(both IN low). Per-side `INVERT_LEFT` / `INVERT_RIGHT` constants MUST be applied at the
last step (negating the sign before the direction encoding). The single timer MUST be
shared by passing `&timer` to each channel (never `Rc<LedcTimerDriver>`, which is not
`Send`), so the struct holding one timer + four drivers is `Send + 'static` and can move
into the motor task thread.

> Verification: cross-compile (LEDC APIs resolve for `xtensa-esp32-espidf`) +
> host-inspectable (resource plan, `get_max_duty()` usage, `&timer` sharing, encoding) +
> on-device (bench: logic analyzer/multimeter confirms 1 kHz PWM and forward/backward
> IN1/IN2 encoding on GPIO 12–15).

#### Scenario: Single shared timer, four channels

- GIVEN the firmware initializes the LEDC peripheral
- WHEN the motor drivers are built
- THEN one `LedcTimerDriver` on timer0 (1 kHz, 8-bit) is shared via `&timer` across channels 0–3 mapped to GPIO 12/13/15/14

#### Scenario: Duty derives from get_max_duty, never a hardcoded constant

- GIVEN an 8-bit LEDC timer
- WHEN a side duty is computed
- THEN it is `(|speed| * get_max_duty()) as u32` with `get_max_duty()` returning 256, with no literal 255/256

#### Scenario: Forward encodes IN2 PWM, IN1 low

- GIVEN a positive side speed
- WHEN the side is actuated
- THEN IN1 is driven to 0 and IN2 to the computed duty

#### Scenario: Reverse encodes IN1 PWM, IN2 low

- GIVEN a negative side speed
- WHEN the side is actuated
- THEN IN1 is driven to the computed duty and IN2 to 0

#### Scenario: Stop coasts both pins

- GIVEN any stop condition
- WHEN the motors stop
- THEN both IN pins are driven to 0 (coast, not brake)

#### Scenario: Per-side invert applied last

- GIVEN `INVERT_LEFT` (or `INVERT_RIGHT`) is `true`
- WHEN a side speed is encoded
- THEN the sign is negated immediately before the direction encoding, leaving the frozen forward/reverse convention intact

### Requirement R4: Bring-up speed cap

Rationale: the first battery test runs at reduced speed for safety, with a documented
path back to full range once control is confirmed.

The firmware MUST clamp the magnitude of both side speeds to `MAX_SPEED = 0.7` after
mixing and normalization and before duty computation. This cap MUST be documented as a
bring-up clamp to be raised to `1.0` after the maintainer confirms control on battery.

> Verification: host-inspectable (`MAX_SPEED = 0.7` const applied to both sides; the
> raise-to-1.0 intent documented).

#### Scenario: Side speeds are capped at 0.7

- GIVEN a mix/normalization yields a side speed of magnitude `1.0`
- WHEN the speed cap is applied
- THEN both side speeds are clamped to a magnitude of `0.7`

#### Scenario: Raise path is documented

- GIVEN the bring-up cap is reviewed
- WHEN control is confirmed on battery
- THEN the spec documents that `MAX_SPEED` raises to `1.0` (same change or a scoped correction)

### Requirement R5: Stop policy (safety) — six triggers, one owner

Rationale: a teleop robot that keeps moving on disconnect or error is a physical hazard;
all stop decisions are centralized in the motor task and "stop" always means coast.

The motor task MUST own every stop decision. All six triggers MUST result in a coast
stop (both IN pins low): (1) **boot** — motors start stopped with no default motion;
(2) **WebSocket disconnect** — `Disconnected`/`Close`/`Closed` events; (3) **parse
error** — malformed JSON or wrong envelope shape; (4) **invalid command semantics** —
non-finite or out-of-range values, or unknown `type`; (5) **dead-man** — no valid
`cmd:move` within 1000 ms (enforced via `recv_timeout` in the motor task); (6) explicit
**`cmd:stop`**. After any stop, a later valid `cmd:move` MUST resume motion normally.

> Verification: host-inspectable (all six triggers terminate in one `stop()` in the motor
> task loop) + on-device (USB bench: serial logs show stop on disconnect, malformed
> frame, and dead-man after ~1 s).

#### Scenario: Boot starts stopped

- GIVEN the firmware boots
- WHEN the motor task initializes
- THEN both sides are at duty 0 (coast) with no default motion

#### Scenario: WebSocket disconnect coasts immediately

- GIVEN a connected WebSocket with motors possibly running
- WHEN a `Disconnected`, `Close`, or `Closed` event is forwarded to the motor task
- THEN the motors coast immediately

#### Scenario: Parse error coasts

- GIVEN an inbound frame that fails to parse or has the wrong shape
- WHEN the motor task handles it
- THEN it logs a `warn!` and coasts the motors

#### Scenario: Invalid command coasts

- GIVEN a command with non-finite/out-of-range values or an unknown `type`
- WHEN the motor task handles it
- THEN it logs a `warn!` and coasts the motors

#### Scenario: Dead-man coasts after 1000 ms

- GIVEN a valid `cmd:move` was applied and then no further `cmd:move` arrives
- WHEN 1000 ms elapse without a valid move
- THEN the motor task coasts the motors

#### Scenario: Explicit stop coasts

- GIVEN the motors are running
- WHEN a `cmd:stop` is received
- THEN the motors coast

#### Scenario: Later valid move resumes

- GIVEN the motors have coasted due to any stop trigger
- WHEN a later valid `cmd:move` is received
- THEN motion resumes normally

### Requirement R6: Task and architecture constraints

Rationale: keeps the WebSocket callback non-blocking, concentrates all PWM state in one
thread, and preserves the receive-only keep-out from the prior slice.

The WebSocket callback MUST never block: it MUST only forward inbound `Text(String)`
frames and connection-loss signals to the motor task through a dedicated channel
(unbounded `mpsc::Sender::send`, which is non-blocking). The motor task MUST own all
PWM/LEDC state, and all parsing and stop decisions MUST live in the motor task (not the
WS callback). The firmware MUST send **zero** outbound WebSocket frames — no `send*`
call sites (`register`, `telemetry`, `pong`, heartbeat). The change MUST NOT touch
camera, NVS, web, or server code, and MUST NOT modify `sdkconfig.defaults` functionally.

> Verification: host-inspectable (channel forwarding, single owner of LEDC state, zero
> `send*` sites, diff review of excluded areas) + cross-compile.

#### Scenario: Callback is non-blocking

- GIVEN an inbound frame or connection-loss event in the WS callback
- WHEN the callback handles it
- THEN it forwards `Text(String)` / `ConnectionLost` over the motor channel via a non-blocking send and returns immediately

#### Scenario: Parsing lives in the motor task

- GIVEN the firmware structure
- WHEN an inbound frame arrives
- THEN JSON parsing and every stop decision occur in the motor task, not the WS callback

#### Scenario: No outbound frames

- GIVEN the change diff is reviewed
- WHEN checking for WebSocket sends
- THEN there are zero `EspWebSocketClient::send*` call sites

#### Scenario: Excluded subsystems and build config untouched

- GIVEN the change diff and `sdkconfig.defaults` are reviewed
- WHEN the change is applied
- THEN no camera/NVS/web/server change and no functional `sdkconfig.defaults` change are present

### Requirement R7: On-device verification under the USB/battery safety rule

Rationale: the rig has no protection against USB + battery together, and battery-only
operation has no serial console; verification therefore splits into a USB bench phase
and a maintainer battery phase under a hard rule.

On-device motor verification MUST follow the hard rule: **never USB + battery at the same
time**; the flow is **flash via USB → disconnect USB → connect battery**. The USB bench
phase MUST run with motors disconnected (or L298N motor power off, wheels off the ground),
using the serial console to verify the software path (computed left/right duty, stop on
disconnect / malformed frame / dead-man). The battery phase MUST be performed by the
maintainer with no serial console, verifying physical forward/back/turn and direction
sense, stop-on-disconnect and dead-man, and the robot appearing in hub `/robots`; the
board MUST still boot with motors wired (GPIO12 strapping check). If physical forward is
reversed, direction MUST be corrected via the `INVERT_LEFT`/`INVERT_RIGHT` constants, never
by changing the frozen convention.

> Verification: on-device (USB bench logs + maintainer battery observation + hub
> `/robots`).

#### Scenario: Bench phase avoids USB motor load

- GIVEN a USB-only bench
- WHEN the firmware is exercised
- THEN motors are disconnected or motor power is off, and no motors are driven on USB power

#### Scenario: Battery phase respects the hard rule

- GIVEN an on-device motor test
- WHEN the maintainer runs it
- THEN they flash via USB, disconnect USB, then connect battery — never both at once

#### Scenario: Physical direction corrected via invert consts

- GIVEN the battery phase shows forward drives the car backward
- WHEN direction is corrected
- THEN `INVERT_LEFT`/`INVERT_RIGHT` is flipped for the affected side, leaving the frozen forward/reverse convention unchanged

## Non-goals / keep-outs

The change MUST NOT:

- Send any outbound WebSocket frame (`register`, `telemetry`, `pong`, heartbeat) — receive-only holds.
- Add camera/MJPEG video, telemetry publishing (Phase 3), SNTP, TLS/WSS, or auth code.
- Extend NVS config for motor tuning (constants only; NVS keys are a later option).
- Add brake mode (both IN full) or PID/closed-loop speed control.
- Change `server/` behavior, the web UI, the partition table, the brownout detector, or `sdkconfig.defaults` functionally.
- Add `embassy-executor` (blocking loop, like the prior slice).
- Change the envelope v0 schema or the `{v, omega}` command model — the hub relays commands verbatim.
