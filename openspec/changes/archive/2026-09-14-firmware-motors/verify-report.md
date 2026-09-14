```yaml
schema: gentle-ai.verify-result/v1
evidence_revision: sha256:25e0a447d6a4b7aafe4516b230c0a8f6334e5a815f0f6ce140a1b6e197fd1ef9
verdict: pass
blockers: 0
critical_findings: 0
requirements: 7/7
scenarios: 32/32
test_command: N/A (firmware-only change; no host tests per openspec/config.yaml testing.firmware)
test_exit_code: 0
test_output_hash: sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
build_command: /nix/store/3r9ar16m779lj6j2rxcmp87iq6kvddwc-esp32-robot-firmware-fhs/bin/esp32-robot-firmware-fhs -c 'cd firmware && cargo build'
build_exit_code: 0
build_output_hash: sha256:414c0397259ef0280ea5b7ffac762d48b2650aad5b8ba894abd844bb581c8108
```

# Verify Report — firmware-motors

Verification phase for change `firmware-motors` (ESP32-CAM firmware parses hub-relayed
`cmd:move {v, omega}` / `cmd:stop` and drives the L298N over LEDC PWM, with the six-trigger
coast-stop policy). Backend: `openspec`. Read-only verification: no implementation file was
edited.

`evidence_revision` digests the concatenated `sha256sum` of the evidence set: `spec.md`,
`design.md`, `tasks.md`, `apply-progress.md`, `firmware/src/command.rs`, `firmware/src/motors.rs`,
`firmware/src/ws_client.rs`, `firmware/src/main.rs`, `firmware/Cargo.toml`, `README.md`.

## Overall verdict

**verified / pass** — `blockers: 0`, `critical_findings: 0`. Independent cross-compilation exits
`0`; all 7 requirements and all 32 scenarios are covered by code inspection plus the maintainer's
on-device evidence. No FAIL and no unresolved spec mismatch.

**Archive readiness caveat (not a verdict change):** `tasks.md` still holds one unchecked line —
the explicitly **optional** GPIO12–15 scope measurement (no instrument on the rig; see "Task
completion status"). Per the native status engine this keeps `archive: blocked`
(`taskProgress.remaining: 1`) and `nextRecommended: sdd-apply`; the line needs a stale-checkbox
reconciliation or an explicitly recorded non-critical partial-archive exception before archive.
It is **not** a requirement failure: R3's normative duty/encoding behavior is verified statically
and physically, and the scope measurement itself is marked optional in both `tasks.md` and
`apply-progress.md`.

## Independent cross-compilation (not trusting apply's claim)

Command (FHS wrapper invoked directly; the flake `shellHook` `exec`s the wrapper and drops `-c`
from `nix develop -c`):

```
/nix/store/3r9ar16m779lj6j2rxcmp87iq6kvddwc-esp32-robot-firmware-fhs/bin/esp32-robot-firmware-fhs -c 'cd firmware && cargo build'
```

Result (exit code **0**; combined stdout+stderr hashed as `sha256:414c0397…c8108`):

```
🔧 [firmware-fhs] entorno FHS — target xtensa-esp32-espidf
   ✅ toolchain 'esp' instalado
   cd firmware && cargo build
warning: linker stderr: [ldproxy] Running ldproxy
  |
  = note: `#[warn(linker_messages)]` on by default

warning: `esp32-robot-firmware` (bin "esp32-robot-firmware") generated 1 warning
    Finished `dev` profile [optimized + debuginfo] target(s) in 0.29s
```

The single warning is the benign `linker_messages` notice (`ldproxy`), matching the archived
precedent. `serde`/`serde_json` link for `xtensa-esp32-espidf`, and the LEDC/GPIO types resolve.

## Requirement-by-requirement verdict

| # | Requirement | Verdict (static) | On-device | Evidence (file:line) |
|---|-------------|------------------|-----------|----------------------|
| R1 | Command intake (envelope v0 parse/validate) | PASS | PASS (invalid-value variant) | `command.rs:34` (`Envelope`, `payload: Option<Value>`), `:40`/`:50`/`:64` (manual `MovePayload` reader; `IgnoredAny` tolerates `id`/`ts`/`robot_id`), `:93`-`:118` (`parse`: `MalformedJson` → `WrongShape` → `UnknownType` → `InvalidValue`), `:101`-`:107` (`is_finite` + `abs()>1.0` rejection), `:119`-`:147` (`mix`) |
| R2 | Differential mix + deadband + normalization | PASS | PASS | `command.rs:8` (`DEADBAND=0.05`), `:119`-`:126` (both-axes deadband → coast; per-axis zeroing), `:128`-`:130` (`left=v-omega`, `right=v+omega`), `:131`-`:134` (normalize by `max(|l|,|r|)` when `>1.0`), `:136`-`:143` (`clamp(-1,1)`) |
| R3 | LEDC PWM with frozen pin map / encoding | PASS | Physical PASS; scope measurement **not executed, optional** | `motors.rs:36`-`:42` (`_timer` declared last), `:44`-`:62` (one `ledc.timer0` + `&TimerConfig::new()`; channels 0–3 → GPIO12/13/15/14 via `&timer`), `:92`-`:104` (`get_max_duty()`, duty formula, forward/reverse/coast encoding), `:69`-`:83` (invert applied last) |
| R4 | Bring-up speed cap `MAX_SPEED=0.7` | PASS | PASS (folded into direction test) | `motors.rs:26` (`MAX_SPEED=0.7`), `:71`-`:72` (clamp after mix, before duty), `:25` (documented raise path to `1.0`) |
| R5 | Stop policy — six triggers, one owner | PASS | PASS (dead-man, disconnect, invalid→coast) | `motors.rs:44`-`:62` (boot = duty 0), `:114`-`:146` (`run`: `Text`→`parse`, `Stop`→`stop`, `Err`→`warn!`+`stop`, `ConnectionLost`→`stop`, `Timeout`+`moving`→`stop`, `Disconnected`→`stop`+`park`), `:85`-`:90` (`stop()` = all four duty 0); callback wiring at `ws_client.rs:86`-`:90` |
| R6 | Task/architecture constraints (receive-only) | PASS | n/a | `ws_client.rs:79`-`:91` (callback forwards `Text`/`ConnectionLost` via unbounded `mpsc` `.send(…).ok()`, returns immediately), `motors.rs:117`-`:146` (all parsing/stop decisions in the motor task; `command::parse` is called only here), zero outbound WS send sites, `sdkconfig.defaults`/`wifi.rs`/`net.rs`/`config.rs` unchanged |
| R7 | On-device verification under the USB/battery rule | PASS | PASS with caveats | `apply-progress.md` §"On-device verification (EJECUTADA)"; `README.md:170`-`:178` (hard rule + `INVERT_*` path); `motors.rs:30` (`INVERT_LEFT=true`) |

### Static detail worth recording (R3 timer math, independently source-checked)

The frozen claim "`TimerConfig::new()` = 1 kHz / 8-bit and `get_max_duty()` = 256" was verified
against the vendored dependency, not just trusted:

- `esp-idf-hal-0.46.2/src/ledc.rs:66`-`72`: `TimerConfig::new()` → `frequency: Hertz(1000)`,
  `resolution: Resolution::Bits8`.
- `ledc.rs:535`-`:541`: `max_duty()` → `1 << bits()` = `1 << 8` = **256** on `esp32` (only the
  20-bit ESP32 and 14-bit other-chip maxima subtract 1).
- `ledc.rs:227`: `LedcDriver` copies `max_duty` at construction; `ledc.rs:180`/`:397`:
  `LedcTimerDriver`/`LedcDriver` are `unsafe impl Send` — so `Motors` is `Send + 'static` and
  moves into `thread::spawn` with no `Rc` and no self-referential borrow.

### Scenario coverage (32/32)

| Requirement | Scenarios | Coverage argument |
|-------------|-----------|-------------------|
| R1 | 5 | `Envelope` without `deny_unknown_fields` ignores `id`/`ts`/`robot_id`; `MovePayload` visitor ignores unknown keys and requires `v`/`omega`; `Text`→`parse` arms give `Ok(Move)`/`Ok(Stop)`/`Err(MalformedJson|WrongShape|UnknownType|InvalidValue)`, all converging on `warn!` + coast |
| R2 | 5 | Worked examples reproduce exactly: `1.0/0.0→1.0/1.0`; `0.0/1.0→-1.0/1.0` (CCW); `1.0/0.5→0.333/1.0`; both `<ε`→`0/0`; `0.8/0.02`→`0.8/0.8` |
| R3 | 6 | One timer shared via `&timer`; duty from `get_max_duty()` (no literal 255/256 — grep-confirmed); `s>0`→`(IN1=0, IN2=duty)`; `s<0`→`(duty, 0)`; `s=0`→`(0,0)`; invert negates the sign before `drive_side` |
| R4 | 2 | `clamp(-MAX_SPEED, MAX_SPEED)` on both sides after mix/normalize; raise path documented in `motors.rs:25` and in spec R4 |
| R5 | 7 | Boot coast, disconnect→`ConnectionLost`→coast, parse error→coast, invalid→coast, dead-man `recv_timeout(1000ms)`→coast, `cmd:stop`→coast, and later valid `cmd:move`→`apply` resumes; all six share one `stop()` |
| R6 | 4 | Non-blocking unbounded channel sends + `.ok()`; parsing only in `motors::run`; zero `EspWebSocketClient::send*` sites; excluded subsystems untouched |
| R7 | 3 | Bench phase ran USB-only (motors driven only in battery phase); battery phase flashed via USB → disconnected USB → battery; direction corrected via `INVERT_LEFT=true` with the convention unchanged |

## On-device evidence review

The maintainer's evidence in `apply-progress.md` §"On-device verification (EJECUTADA)" covers the
acceptance criteria:

| Criterion | Reported result | Assessment |
|-----------|-----------------|------------|
| USB bench bring-up | Flash OK (`.bin` 1,385,088 B = 33.55 %); normal boot; `config resolved` → `wifi:connected` → `sta ip 192.168.1.130` → `WebSocket connected` | Covered |
| Software reject + coast | `cmd:move v=5` → `W invalid command InvalidValue; coasting` on serial | Covered (invalid-value variant of the shared `Err` arm) |
| Directions (physical) | L1/L2 inverted with this kit's wiring, R1/R2 correct; after `INVERT_LEFT=true` + re-flash: L1 forward, L2 backward, R1/R2 correct | Covered; the spec's `INVERT_*` correction path was actually exercised and re-validated |
| Dead-man | One `cmd:move v=0.5` then silence → stop at **~1 s** (maintainer-timed) | Covered |
| Disconnect | Continuous stream (one command every 0.25 s, dead-man always renewed) + hub cut with a generated audible marker (`pw-play` beep): wheels stopped **almost instantly with the beep (<1 s)** | Covered |
| Boot with motors wired | Normal (non-silent) boot log; GPIO12/MTDI strapping OK | Covered |
| Hub `/robots` | Robot `a1` listed; reconnected by itself after the hub cut | Covered |
| PWM scope measurement (GPIO12–15) | **Not executed** — no logic analyzer/oscilloscope | **Not executed, optional** (explicitly optional in `tasks.md` and `apply-progress.md`) |

### Honesty notes on the evidence's strength

- **Timing judgments are human-observed, not instrumented.** "Dead-man ~1 s" is a maintainer
  stopwatch estimate and "stopped almost instantly" is a visual/audible estimate; neither is a
  measured waveform. They are consistent with the code (`DEADMAN = 1000 ms`, coast on the
  `Timeout` arm) but do not by themselves prove millisecond accuracy.
- **The disconnect-path distinction was reinforced methodologically, not instrumented.** Because
  the stream ran at 0.25 s cadence (dead-man continuously renewed), a stop caused by the dead-man
  would have lagged the link cut by roughly a full second; the observed near-simultaneity with the
  audible marker is what argues for `ConnectionLost → coast` rather than the dead-man path. This
  is a reasonable inference from good bench technique, not a direct trace of the `ConnectionLost`
  arm.
- **Malformed-JSON (as opposed to out-of-range) was not separately injected on-device.** The
  observed `InvalidValue` rejection exercises the same `Err(e)` branch that handles
  `MalformedJson`/`WrongShape`/`UnknownType`, and `parse` maps each variant to that branch
  statically (`command.rs:93`-`:118` + `motors.rs:124`-`:128`), so this is a coverage nuance, not a
  gap in behavior.
- **`.bin` size is reported, not independently reproducible here.** `cargo build` leaves the ELF
  in `target/`; no `esp32-robot-firmware.bin` artifact exists in the repo, so verify cannot rehash
  1,385,088 bytes. The figure is consistent with the design estimate (~1.37–1.45 MB) and the
  +112,080 B delta vs the prior 1,273,008 B baseline.

## Bench findings (follow-ups, not spec failures)

1. **Left motors' higher duty deadband at ~30 %.** At duty ≈ 30 % (`v = 0.3` → 77/256) the left
   motors do not start; at 70 % (179/256) they do. This is a hardware/mechanical characteristic of
   these motors, not a spec violation — R2/R4 only require the mixing/deadband/clamp math and the
   L298N encoding, and `+v = forward` holds. Proposed follow-up (recorded in `apply-progress.md`):
   an optional minimum-duty floor applied only when `|speed| > ε`. Not part of this change; would
   need its own scoped change if adopted.
2. **Slow clients cause dead-man pulses.** A client spacing `cmd:move` frames more than ~1 s apart
   sees the robot stop between commands (the 1000 ms dead-man expires by design). Expected
   behavior of R5, documented as a bench finding; a future client-side heartbeat/cadence contract
   (or a longer window) is a separate design decision.

Neither finding blocks the change; both are candidates for a follow-up change.

## Task completion status

Scanned `openspec/changes/firmware-motors/tasks.md` for `^\s*- \[ \]`:

- **Unchecked implementation-owned lines remaining: 1** (exact line, as it appears in `tasks.md`
  line 154):

  ```
  - [ ] Optional: scope GPIO12–15 (logic analyzer/multimeter) to confirm 1 kHz PWM and the forward/backward IN1/IN2 encoding. No battery connected. <!-- sdd-owner: implementation -->
  ```

  This is the instrument-gated optional measurement (mirrored as
  `- [ ] Opcional: scope GPIO12–15 (PWM 1 kHz + encode IN1/IN2) — **no ejecutado** (sin
  osciloscopio)` at `apply-progress.md:189`). No other unchecked lines exist: **36 checked**,
  37 total per the native status engine.
- All four implementation tasks, the 6 keep-out checklist items, and the Phase A/Phase B
  on-device items are `[x]`. The status engine's `taskProgress` (`36/37`) matches this scan.
- **Archive gate:** per the native status contract, any unchecked implementation-owned line is an
  archive blocker, so `archive` and `sync` remain `blocked` until this line is reconciled (stale-
  checkbox reconciliation with a note that the measurement is instrument-gated, or an explicitly
  recorded non-critical partial-archive exception). The parent per this phase's instructions treats
  the line as "not executed, optional", which is why the envelope verdict stays `pass` with
  `blockers: 0`; verify does **not** claim archive readiness and does not itself edit `tasks.md`.

## Strict TDD compliance

**N/A.** `openspec/config.yaml` sets `strict_tdd: true` but scopes it to server-side and
host-testable code, and `testing.firmware` states explicitly that TDD (RED/GREEN/REFACTOR) does
not apply to firmware-only code (no host runner; validated by cross-compilation + on-device
checks). This change touches only `firmware/` sources and `README.md`; no host-testable code
changed. No `TDD Cycle Evidence` table is required, and none is asserted in `apply-progress.md`.

## Assertion quality findings

**N/A** — no test files were created or modified by this change (no host test runner exists for
`firmware/`). The strict-TDD assertion-quality audit is therefore inapplicable; its absence is not
a finding.

## Test / validation commands

- Independent cross-compile (required command for firmware): wrapper `cd firmware && cargo build`
  → exit **0** (hash `sha256:414c0397…c8108`).
- Host tests: none exist for `firmware/` (`openspec/config.yaml` → `testing.firmware`); no
  `server/` or other host-testable surface was touched, so `cd server && cargo test` is out of
  scope for this change.
- On-device: `espflash flash` + strap-safe serial capture via the project skill
  `.pi/skills/esp32-ondevice-workflow/`; evidence recorded by the maintainer in `apply-progress.md`.

## Keep-out verification

| Keep-out | Result |
|----------|--------|
| Zero outbound `send()` / `EspWebSocketClient::send*` sites | PASS — grep of `firmware/src` finds only internal channel sends: `wifi.rs:57` (`NetEvent::WifiStaDisconnected`), `ws_client.rs:80` (`NetEvent::WsEvent`), `ws_client.rs:84`/`:89` (`MotorSignal`). No `register`, `telemetry`, `pong`, or heartbeat. |
| No camera / telemetry / SNTP / TLS-WSS / auth code | PASS — no such code in the diff. |
| No NVS/config extension for motor tuning | PASS — only `const`s (`MAX_SPEED`, `INVERT_*`, `DEADMAN`, `DEADBAND`); `config.rs` unchanged. |
| No `server/`, web UI, partition-table, brownout change | PASS — no diff in those surfaces. |
| `firmware/sdkconfig.defaults` byte-identical | PASS — `git status` clean for the file. |
| No `embassy-executor`; `Cargo.toml` unchanged | PASS — only the pre-existing `embassy-executor-timer-queue` transitive note; `firmware/Cargo.toml`, `Cargo.lock` unchanged. |
| No `unwrap`/`expect`/`panic!` on wire-derived data | PASS — grep of `command.rs`/`motors.rs`/`main.rs`/`ws_client.rs` finds none; `set_duty` errors are `warn!`-swallowed (`motors.rs:106`). |

## Review workload / PR boundary findings

Authored changed lines (excluding `openspec/` artifacts and build-generated lockfiles);
independent `git` accounting, not the apply claim:

| File | + | − |
|------|---|---|
| `firmware/src/command.rs` (new) | 145 | 0 |
| `firmware/src/motors.rs` (new) | 150 | 0 |
| `firmware/src/ws_client.rs` | 15 | 0 |
| `firmware/src/main.rs` | 15 | 1 |
| `README.md` | 9 | 0 |
| **Total** | **334** | **1** |

**Total changed = 335 lines**, under the 400-line budget. No `size:exception`, no chaining.
`tasks.md` forecast 257 and `apply-progress.md` reported 333; the actual count is **335** (the
`motors.rs` new-file length is 150 lines, not the 148 apply reported — a reporting discrepancy of
+2 lines, immaterial to the budget). The overage vs the 257 forecast traces to deviation D2
(manual `MovePayload` deserialization) plus comments/blank lines, as apply documented.

- **Workload boundary respected.** Single PR, 4 work-unit commits as planned; `openspec/`
  artifacts excluded; no scope creep in implementation files. `Cargo.lock`,
  `components_esp32.lock`, and `sdkconfig.defaults` are byte-identical.
- **Post-apply fix delta (on-device correction):** `INVERT_LEFT` was flipped `false → true` after
  the battery-phase direction test (`motors.rs:30`; `INVERT_RIGHT` stays `false`). This is the
  spec-sanctioned R7 correction path, not scope creep; it is a value change on one existing line
  and does not alter the workload count materially. The design's frozen default was `false`, so
  the shipped default now deviates from `design.md` while remaining fully consistent with R7 and
  the frozen forward/reverse convention (which is untouched).
- **Out-of-scope working-tree edit (WARNING, non-blocking):**
  `.pi/skills/esp32-ondevice-workflow/SKILL.md` has **+7 lines** in the working tree (USB/battery
  hard rule in the Hard Rules list and the troubleshooting table). It is a harness/skill doc, not
  part of the change's declared authoring surfaces (design §2 lists only the firmware files and
  `README.md`), and it does not affect the firmware artifact. Counting it, total authored working-
  tree lines would be 342 — still under budget. It duplicates the rule documented in `README.md`
  and is consistent with R7; recording it so the reviewer knows why it is in the diff.

## Structured status and actionContext findings

- Native status consumed: `changeName: firmware-motors`, `artifactStore: openspec`,
  `isNonAuthoritative: false`, `applyState: ready`, `nextRecommended: sdd-apply`.
- `actionContext.mode: repo-local`; `workspaceRoot` `/home/ejverat/Projects/esp32-robot`;
  `allowedEditRoots` `[/home/ejverat/Projects/esp32-robot]`; no warnings. All touched files and
  the built artifact are inside the authoritative workspace, so implementation ownership is proven.
- Dependencies in the consumed status: `verify: ready`, `sync: blocked`, `archive: blocked`, with
  the single blocked reason being the optional unchecked task line above. `blockedReasons` in the
  status JSON is empty; the actionable gate is the unchecked task line, handled under "Task
  completion status". `sync`/`archive` remain blocked until it is reconciled; `verify` itself is
  clean.
- No dependency, supersede, amend, or conflict relationships; no collisions; no same-domain active
  changes.

## Confirmed apply deviations (behavior-neutral)

1. **D1 — imports via `esp_idf_svc::hal::…` instead of `esp_idf_hal::…`.** `esp-idf-hal` is only a
   transitive dependency; `esp_idf_svc::hal` re-exports the same types (`LedcDriver`,
   `LedcTimerDriver`, `LowSpeed`, `LEDC`, `TimerConfig`, `Gpio12..15`). Behavior-identical.
2. **D2 — manual `MovePayload` `Deserialize` reading `v`/`omega` as `f64` (then `as f32`) instead
   of the frozen `#[derive(Deserialize)]` with `f32`.** Root cause is a real Xtensa LLVM 21.1.3
   ISel bug (`PCREL_WRAPPER` constant pool `[2 x float] [-1.0, 1.0]` from `serde_core`'s f32
   `copysign` NaN-preservation path), which broke codegen. The public shape `{v: f32, omega: f32}`
   is preserved; extra keys are tolerated via `IgnoredAny`; missing/wrong-typed fields →
   `WrongShape`; `parse` still rejects non-finite and out-of-range values, so `1e300 → f32::INFINITY
   → InvalidValue`. Only the (rejected) NaN sign payload differs. **Behavior-neutral; PASS.**
3. **Post-apply fix — `INVERT_LEFT` `false → true`** (on-device R7 correction; see workload
   section). Behavior is the spec-sanctioned direction correction.

## Exact blockers

None.

## Residual risks (non-blocking)

1. **Archive gate open:** the optional GPIO12–15 scope line is unchecked, so `sync`/`archive` stay
   blocked until reconciled (stale-checkbox reconciliation or recorded non-critical partial
   archive). The normative PWM frequency/encoding behavior remains verified by source inspection
   of `TimerConfig::new()`/`max_duty()` plus the physical direction tests.
2. **Timing evidence is human-observed** (dead-man ~1 s, disconnect near-instant), not
   instrumented; the disconnect path's separation from the dead-man was argued by the 0.25 s
   command cadence + audible marker, not by a trace.
3. **`.bin` size (1,385,088 B) is applied/reported, not independently reproduced** by verify (the
   repo holds no app `.bin`).
4. **Bench follow-ups recorded but not addressed:** left-motor ~30 % duty deadband; slow-client
   dead-man pulses. Both are expected behavior/hardware for this slice and need their own scoped
   change if actioned.
5. **Design-vs-code drift on the `INVERT_LEFT` default** (design says `false`, code ships `true`).
   Intentional per R7; a future reader should consult the on-device record, not only `design.md`.
6. **GPIO12/GPIO15 strapping sensitivity** remains a wiring, not code, risk; the on-device boot
   check with motors wired passed on this kit.
