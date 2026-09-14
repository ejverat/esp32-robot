# Proposal — firmware-motors

> SDD Phase 1 slice: the ESP32 firmware parses `cmd:move` / `cmd:stop` WebSocket
> commands relayed by the hub and drives the 4WD car's L298N motor driver over LEDC PWM,
> completing the robot side of roadmap Phase 1 LAN teleoperation. Safety-first: all six
> stop triggers (boot, WS disconnect, parse error, invalid command, 1 s dead-man,
> `cmd:stop`) are centralized in one motor task; stop = coast.
>
> OUT of scope: camera/video, telemetry publishing (Phase 3), SNTP, TLS/WSS, auth, any
> outbound WS send (the no-send keep-out from `firmware-wifi-ws-client` persists).

Status: proposal (ready for `sdd-spec`).

---

## 1. Why

The robot has its network life (`firmware-wifi-ws-client`: WiFi station + WebSocket client
to the hub) but cannot move. Inbound `Text` frames are currently logged and dropped, so the
hub can route `cmd:move`/`cmd:stop` to the robot and nothing happens. There is no way to
teleoperate the car, which blocks the rest of the roadmap (camera, telemetry, AI) from
being exercised on real hardware.

This change adds the first real application behavior on the receive path: parse the
envelope v0 `cmd:move {v, omega}` / `cmd:stop` commands and turn them into safe L298N PWM
via the LEDC peripheral, with a hard safety policy (stop on any abnormal condition). It is
the smallest slice that makes the robot *act* under hub control — nothing more.

The C++ reference (`ejverat/esp-cam-robot-car`) already validates the hardware on this kit
(L298N pins, 1 kHz 8-bit PWM, coast-on-stop, spin-in-place differential mixing). We port
that behavior, not its architecture.

## 2. What changes

| Area | Change |
|------|--------|
| `firmware/src/motors.rs` (new) | `MotorSignal` enum (`Text(String)`, `ConnectionLost`) + `MotorSender`; `Motors` struct (1 `LedcTimerDriver` + 4 `LedcDriver`: timer0, channels 0–3, 1 kHz, 8-bit); `init`, `apply(v, omega)`, `stop`, and `run(rx, motors)` — the motor task that owns every stop decision. |
| `firmware/src/command.rs` (new) | Envelope v0 deserialization (`type` + `MovePayload { v, omega }`) via `serde`/`serde_json`; `v`/`omega` validation (finite, `[-1,1]`); deadband + differential mixing + normalization; `cmd:move`/`cmd:stop` dispatch. |
| `firmware/src/ws_client.rs` (edit) | Callback forwards `Text(s)` → `MotorSignal::Text(s.to_string())` and `Disconnected`/`Close`/`Closed` → `MotorSignal::ConnectionLost`. `start()` gains a `MotorSender` param. Still **no outbound send**; the callback never blocks (unbounded `mpsc::Sender::send`). |
| `firmware/src/main.rs` (edit) | Create `(motor_tx, motor_rx) = mpsc::channel::<MotorSignal>()`, build `Motors`, spawn `thread::spawn(move || motors::run(motor_rx, motors))`, pass `motor_tx.clone()` to `ws_client::start`. Extract `peripherals.ledc` + `peripherals.pins` for motors (keep `peripherals.modem` for WiFi). |
| `README.md` (edit) | Spanish note (`user_facing_docs: es`): the USB/battery hard rule and the on-device motor verification flow. |

No `firmware/Cargo.toml` change: `serde` (derive) and `serde_json` were pre-declared last
slice and are now actually linked (no new dependency, no new `extra_components` entry).
No `server/` change — the hub already relays `cmd:move`/`cmd:stop` verbatim.

### 2.1 Pin map and PWM scheme (frozen from explore)

| Side | IN1 (pin1) | IN2 (pin2) | forward | backward |
|------|------------|------------|---------|----------|
| LEFT  | GPIO 12 (ch0) | GPIO 13 (ch1) | IN1=0, IN2=PWM | IN1=PWM, IN2=0 |
| RIGHT | GPIO 15 (ch2) | GPIO 14 (ch3) | IN1=0, IN2=PWM | IN1=PWM, IN2=0 |

- One `LedcTimerDriver` on **timer0**, **1 kHz**, **8-bit**, shared across all four
  channels via `&timer` (NOT `Rc<LedcTimerDriver>`, which is not `Send`). The struct
  (1 timer + 4 drivers) is `Send + 'static` and moves into `thread::spawn`.
- Duty is computed from `get_max_duty()` (returns **256** for 8-bit, not 255), never a
  hardcoded constant. Coast = both IN pins 0; brake (both full) is **not** used.

### 2.2 Command model and mixing

- `left = v - omega`, `right = v + omega` (spin-in-place). `+v` = forward, `+omega` =
  turn left (CCW). `v`/`omega` in `[-1,1]`, finite.
- Deadband `ε = 0.05` applied to the **inputs** before mixing: both within ε → coast; else
  zero only the near-zero axis, then mix.
- Normalize by `max(|left|, |right|)` when it exceeds 1 (preserves steering direction).
- `MAX_SPEED = 0.7` bring-up cap applied to the resulting side speeds before duty.
- Per-side `INVERT_LEFT` / `INVERT_RIGHT` consts (escape hatch for physical direction).

## 3. Impact

- **Firmware binary grows** — `serde_json` was declared-but-unused last slice and now
  links (dominant cost), plus `serde` derive + `esp_idf_hal::ledc` Rust driver + new code.
  Estimate **+100–200 KB → ~1.37–1.45 MB (~34–36%)** of 4 MB; re-measure `.bin` in apply.
- **Robot now moves** under hub command, so the hardware safety rule becomes load-bearing:
  a runaway motor on disconnect/error is a physical hazard (mitigated by centralized stops).
- **No server behavior change**; no NVS schema change (constants only, no config extension).
- **Receive-only still holds**: the firmware still sends zero WS frames.

## 4. Key design decisions (frozen — the 7 pre-proposal decisions, verbatim)

1. **Physical forward sense**: reference C++ convention (`IN2` = PWM forward, `IN1` low;
   symmetric for reverse) with per-side `INVERT_LEFT`/`INVERT_RIGHT` consts as the escape
   hatch. Physical direction validated on battery during on-device; if reversed, fix via
   the const, never the convention.
2. **`omega` sign**: `+omega` = turn left (CCW), matching the reference `turnLeft`. The
   future web panel must respect it.
3. **Dead-man timeout**: 1000 ms without `cmd:move` → coast stop from the motor task.
4. **Deadband**: `ε = 0.05` on `v` and `omega` before mixing.
5. **Speed limit**: `MAX_SPEED = 0.7` for the first battery test; raise to 1.0 after
   control is confirmed (same change or a scoped correction).
6. **`cmd:stop` = coast** (both IN low). Brake (both IN high) is deferred.
7. **GPIO12 as motor PWM**: accepted (reference-validated on this kit). Caveat documented:
   a wiring change could affect MTDI strapping at boot.

**Hardware safety rule (all artifacts):** no protection against USB + battery together.
On-device motor tests: **flash via USB → disconnect USB → connect battery**. Battery-only =
no serial console, so verification is by the maintainer's physical observation + hub
`/robots`. If the maintainer fixes the electronics, the flow updates (rig skill + artifacts).

## 5. Non-goals (explicit)

- Any **outbound WS send** (`register`, `telemetry`, `pong`, heartbeat) — keep-out holds.
- Camera / MJPEG video; telemetry publishing (Phase 3); SNTP; TLS/WSS; auth.
- NVS config extension for motor tuning (constants now; NVS keys are a later option).
- Brake mode (both IN full); PID/closed-loop speed control.
- Web UI changes; `server/` changes (hub already relays commands verbatim).
- `sdkconfig.defaults` functional change; partition-table change; brownout-detector change.
- `embassy-executor` — not present, not added (blocking loop, like the prior slice).

## 6. Risks (carried from explore.md) + mitigations

| Risk | Mitigation |
|------|------------|
| **Runaway motors** on disconnect/parse error (physical hazard) | Motor task owns all six stop triggers; stop = coast. One reviewable place. |
| **USB + battery simultaneously** (no protection) → board damage | Hard rule in artifacts; battery phase is maintainer-performed; bench phase uses USB with motors disconnected / motor power off. |
| **GPIO12 (MTDI) strapping** breaks boot if high at reset | L298N pull-downs keep it LOW (reference-validated); documented caveat + on-device boot check. No code change. |
| **GPIO15 (MTDO)** silent boot if high at reset | Pull-down keeps it LOW → normal log; consistent with prior strap quirk. |
| **`max_duty` = 256 not 255** (LEDC 8-bit) | Compute duty from `get_max_duty()`; never hardcode 255. |
| **Timer `Drop` resets shared LEDC timer** | One `LedcTimerDriver`, passed as `&timer` to each channel, stored in the struct (no `Rc`). |
| **Brownout on USB-powered motor tests** | Motors-disconnected bench only; battery phase uses battery power; brownout detector untouched. |
| **`recv_timeout`/`Instant` reliability on std time64** | Time64 already enabled; fallback to `recv()` + `std::thread::sleep` poll if needed. |
| **Physical direction reversed** (wiring) | Single source of truth + `INVERT_*` consts; hardware confirmation on battery. |
| **Flash growth from linking serde_json** | Re-measure `.bin` in apply; expected ~34–36%, well within budget. |
| **No serial console in battery mode** | Bench-verify software path on USB; battery phase via physical observation + hub `/robots`. |

## 7. Rollback

- Revert the commit and re-flash the previous firmware. No server, schema, or partition-table
  changes; no new NVS keys. `serde`/`serde_json` were already declared, so `Cargo.toml` is
  untouched. Physical safety is preserved by flashing the prior (immobile) firmware.

## 8. Delivery forecast

Expected diff **~200–300 changed lines** across `motors.rs` (~120–160), `command.rs`
(~60–90), and small edits to `ws_client.rs`, `main.rs`, and `README.md` — **comfortably
under the 400-line review budget**. **Single PR**, `ask-on-risk`, **no chaining**. The only
line-budget unknowns are `main.rs`/`ws_client.rs` plumbing (small) and README verbosity; if
the estimate were to approach 400, this proposal would flag it explicitly — it does not.

## 9. Acceptance criteria (inputs for `sdd-spec`)

Evidence types: **[inspect]** = host-side source/diff review, **[build]** = `nix develop
.#firmware-fhs` + `cd firmware && cargo build`, **[on-device]** = flash + observation.

1. **[build]** `serde`/`serde_json` are now actually referenced (no `Cargo.toml` change, no
   new dependency/`extra_components` entry), and the firmware **cross-compiles** for
   `xtensa-esp32-espidf` without errors.
2. **[inspect] + [build]** LEDC resource plan: one `LedcTimerDriver` on **timer0, 1 kHz,
   8-bit**, shared via `&timer` across four `LedcDriver` channels 0–3 mapped
   LEFT IN1=ch0→GPIO12, LEFT IN2=ch1→GPIO13, RIGHT IN1=ch2→GPIO15, RIGHT IN2=ch3→GPIO14.
   The struct (1 timer + 4 drivers) is `Send + 'static` and moved into `thread::spawn`;
   no `Rc<LedcTimerDriver>`.
3. **[inspect]** Duty is computed from `get_max_duty()` (256 for 8-bit) — no hardcoded 255.
4. **[inspect]** Mixing math: `left = v - omega`, `right = v + omega`; deadband `ε = 0.05`
   applied to `v`/`omega` inputs before mixing (both within ε → coast; else zero the
   near-zero axis); normalize by `max(|left|,|right|)` when > 1.
5. **[inspect]** Direction mapping: forward = IN1=0 / IN2=duty; backward = IN1=duty /
   IN2=0; stop = coast (both 0). Per-side `INVERT_LEFT`/`INVERT_RIGHT` consts negate the
   sign before encoding. `+omega` = turn left (CCW).
6. **[inspect]** `MAX_SPEED = 0.7` clamps the side-speed magnitude before duty computation.
7. **[inspect]** All six stop triggers centralized in the motor task: (1) boot (motors
   initialized stopped, no default motion), (2) WS disconnect via `ConnectionLost`,
   (3) parse error, (4) invalid command (non-finite / out-of-range / unknown `type`),
   (5) dead-man 1000 ms via `recv_timeout`, (6) `cmd:stop`. Every trigger → `stop()` (coast).
8. **[inspect]** No outbound send: zero `EspWebSocketClient::send*` call sites. The WS
   callback only forwards `Text(String)` → `MotorSignal::Text` and
   `Disconnected`/`Close`/`Closed` → `MotorSignal::ConnectionLost`, never blocking.
9. **[inspect]** `command.rs` deserializes envelope v0 `{type, payload:{v, omega}}`,
   validates finite + `[-1,1]`, and dispatches `cmd:move`/`cmd:stop`; unknown type → stop +
   `warn!`.
10. **[on-device, USB bench]** Motors disconnected / L298N motor power off: serial logs
    show computed left/right duty for a `cmd:move`; stop is observed on (a) disconnect,
    (b) malformed frame, (c) dead-man after ~1 s. No motors driven on USB power.
11. **[on-device, battery — maintainer]** Flash via USB → **disconnect USB** → **connect
    battery** (never both). Physical observation confirms forward/back/turn, direction
    sense (INVERT consts if reversed), stop-on-disconnect + dead-man, and the robot appears
    in hub `/robots`. Board boots with motors wired (GPIO12 strapping check).
12. **[build]** Flash budget re-measured: `.bin` size recorded; expected ~1.37–1.45 MB
    (~34–36%), within the 4 MB budget.
13. **[inspect]** Non-goals hold: no telemetry, no camera, no NVS config extension, no brake
    mode, no web UI, no `server/` change, no functional `sdkconfig.defaults` change, no
    `embassy-executor`. A Spanish README note documents the USB/battery rule + on-device
    flow.

## 10. Testing note

Firmware-only change on an Xtensa target: no host-side `cargo test` possible. Validation is
cross-compilation (criterion 1) plus on-device observation split by the USB/battery rule
(criteria 10–11). No server tests change.

---

## Phase envelope

- **status**: `proposal` written — ready for `sdd-spec`.
- **executive_summary**: Adds motor actuation to the firmware by parsing hub-relayed
  `cmd:move {v, omega}` / `cmd:stop` and driving the L298N over LEDC (timer0, 1 kHz, 8-bit),
  with differential mixing + deadband + `MAX_SPEED=0.7`, and all six stop triggers
  centralized in one motor task (coast). Receive-only holds; no server/config changes.
- **artifacts**: `openspec/changes/firmware-motors/proposal.md` (this file);
  `preproposal.md` header status line corrected (decisions untouched).
- **next_recommended**: `sdd-spec` — turn the 13 acceptance criteria into a
  `firmware-motors` spec (safety/stop policy, mixing math, PWM/pin map, no-send, non-goals),
  then `sdd-design` carrying the LEDC `&timer` sharing + `max_duty=256` gotchas.
- **risks**: runaway motors (centralized stops), USB+battery (hard rule + maintainer
  battery phase), GPIO12 strapping (accepted, documented), `max_duty=256` + timer `Drop`
  footguns, physical direction unvalidated (`INVERT_*` consts), serde_json flash growth
  (~+100–200 KB, re-measure in apply).
