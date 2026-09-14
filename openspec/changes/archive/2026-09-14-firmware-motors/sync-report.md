```yaml
schema: gentle-ai.sync-result/v1
change: firmware-motors
artifactStore: openspec
status: synced
syncState: ready
canonical_spec: openspec/specs/firmware-motors/spec.md
canonical_spec_created: true
canonical_spec_overwrote_existing: false
source_delta_spec: openspec/changes/firmware-motors/specs/firmware-motors/spec.md
spec_sha256: e3587093ab8e5bda4ce61047e43e1bf833feaca3a0fb0b844e8d11f7f31999ab
verify_verdict: pass
verify_blockers: 0
requirements_synced: 7/7
scenarios_synced: 32/32
tasks_total: 37
tasks_complete: 37
tasks_unchecked: 0
same_domain_active_changes: []
destructive_sync: none
next_recommended: sdd-archive
```

# Sync Report — firmware-motors

Sync phase for change `firmware-motors`. Backend `openspec` (filesystem). This phase
copies the change's delta spec into the canonical `openspec/specs/` layer for a **new
domain** (`firmware-motors`). It does **not** move the change to archive and does **not**
touch implementation files.

## Overall status: **synced / syncState: ready**

All sync entry conditions are met:

- `verify-report.md` present with `verdict: pass`, `blockers: 0`,
  `critical_findings: 0`, `requirements: 7/7`, `scenarios: 32/32`, and an independent
  cross-compilation exit `0` (`sha256:414c0397…c8108`).
- `tasks.md`: **0 unchecked** implementation-owned lines (37/37 complete), matching the
  native status engine `taskProgress { total: 37, complete: 37, remaining: 0 }`.
- No MODIFIED or REMOVED requirements (new-domain full spec → all requirements are
  effectively ADDED). No destructive sync → no explicit destructive-approval gate.
- No `## RENAMED Requirements` header present → the helper's unsupported path is not hit.
- No same-domain active changes (`relationships.sameDomainActiveChanges: []`,
  `collisions: []`). The only other canonical domain (`firmware-network`) is untouched.
- `actionContext.mode: repo-local`; workspace and allowed edit root are both
  `/home/ejverat/Projects/esp32-robot`. Canonical target `openspec/specs/firmware-motors/spec.md`
  is inside the authoritative edit root.
- `isNonAuthoritative: false` and `artifactStore: openspec`, so the native status is
  authoritative and filesystem sync applies.

Note on the injected status: the native SDD engine reported `dependencies.sync: blocked`
with `blockedReasons: []`. That "blocked" reflects only that `syncReport` was `missing`
(not yet produced) and the earlier archive-gate on the optional task line; it is not a
real blocker. The blocking condition the verify-report flagged (one unchecked optional
GPIO12–15 scope line) has since been reconciled to `[x]` with the maintainer's "NO
EJECUTADO (no instrument)" disposition — exactly the stale-checkbox reconciliation path
verify recommended — so sync is unblocked.

## Canonical spec write (new domain)

| Item | Value |
|------|-------|
| Source (delta) spec | `openspec/changes/firmware-motors/specs/firmware-motors/spec.md` |
| Canonical target | `openspec/specs/firmware-motors/spec.md` |
| Pre-existing canonical? | No — new domain; directory did not exist |
| Overwrote anything? | No — file created fresh, no existing content replaced |
| Source sha256 | `e3587093ab8e5bda4ce61047e43e1bf833feaca3a0fb0b844e8d11f7f31999ab` |
| Dest sha256 | `e3587093ab8e5bda4ce61047e43e1bf833feaca3a0fb0b844e8d11f7f31999ab` |
| Byte equality (`cmp`) | identical |

Because `openspec/specs/firmware-motors/` did not exist, the helper semantics
("canonical spec does not exist → copy the change spec as the new canonical spec") apply.
The delta is a full-spec form (`## Requirements` with R1–R7), so the copy is the complete
canonical spec; there are no incremental ADDED/MODIFIED/REMOVED blocks to merge and no
prior canonical requirements or sections to preserve.

## Per-requirement sync table (7/7 PASS)

All verdicts below carry through the verify-report's PASS findings (code inspection plus
maintainer on-device evidence) into the canonical layer.

| # | Requirement | Sync verdict | Evidence pointer (verify-report / apply-progress / spec) |
|---|-------------|--------------|----------------------------------------------------------|
| R1 | Command intake (envelope v0 parse/validate) | PASS | verify-report R1 row: `command.rs:34,40,50,64,93-118,101-107,119-147`; on-device invalid-value reject + coast; 5/5 scenarios |
| R2 | Differential mix + deadband + normalization | PASS | verify-report R2 row: `command.rs:8,119-126,128-130,131-134,136-143`; worked examples `1.0/0.0→1.0/1.0`, `0.0/1.0→-1.0/1.0`, `1.0/0.5→0.333/1.0`; 5/5 scenarios |
| R3 | LEDC PWM frozen pin map / encoding | PASS | verify-report R3 row: `motors.rs:36-42,44-62,92-104,69-83`; `get_max_duty()=256` source-checked in vendored `esp-idf-hal-0.46.2`; scope measurement **not executed (optional)**, norm behavior static+physical PASS; 6/6 scenarios |
| R4 | Bring-up speed cap `MAX_SPEED=0.7` | PASS | verify-report R4 row: `motors.rs:26,71-72,25` (raise path documented); 2/2 scenarios |
| R5 | Stop policy — six triggers, one owner | PASS | verify-report R5 row: `motors.rs:44-62,114-146,85-90`, `ws_client.rs:86-90`; dead-man ~1 s, disconnect near-instant, invalid→coast; 7/7 scenarios |
| R6 | Task/architecture constraints (receive-only) | PASS | verify-report R6 row: `ws_client.rs:79-91`, `motors.rs:117-146`; zero outbound send sites; camera/NVS/server/web/`sdkconfig.defaults` unchanged; 4/4 scenarios |
| R7 | On-device verification under USB/battery rule | PASS | verify-report R7 row + apply-progress §"On-device verification (EJECUTADA)"; README:170-178; directions corrected via `INVERT_LEFT=true`; 3/3 scenarios |

## Acceptance-criteria coverage

Synced criteria (from R1–R7 and the verify-report on-device table):

- Valid `cmd:move`/`cmd:stop` parse; malformed / unknown-type / out-of-range / non-finite
  all `warn!` + coast without panic (R1, R5).
- `left=v-omega`, `right=v+omega`; `+v=forward`, `+omega=CCW`; ε=0.05 input deadband +
  max-normalize + `[-1,1]` clamp (R2).
- 1 kHz / 8-bit timer0 shared via `&timer`, channels 0–3 → GPIO12/13/15/14; duty from
  `get_max_duty()` (no literal); forward = IN2 PWM/IN1 low, reverse = IN1 PWM/IN2 low,
  stop = coast; per-side invert applied last (R3).
- `MAX_SPEED=0.7` bring-up clamp with documented raise-to-1.0 path (R4).
- All six stop triggers (boot, disconnect, parse error, invalid, 1000 ms dead-man,
  `cmd:stop`) converge on one `stop()` coast; later valid move resumes (R5).
- Non-blocking WS callback (unbounded mpsc forward, `.ok()`), motor-task owns parsing and
  all stops, zero outbound `send*` sites, excluded subsystems untouched (R6).
- USB/battery hard rule exercised (flash USB → disconnect → battery); physical directions
  corrected via `INVERT_*` const, convention intact; boot with motors wired; hub `/robots`
  shows `a1` and auto-reconnects (R7).

All 7 requirements / 32 scenarios carried into the canonical spec match verify-report's
7/7, 32/32.

## Ledger state

- `tasks.md`: 37 checkbox lines, **0 unchecked** (`grep -E '^\s*- \[ \]'` → none).
- Optional GPIO12–15 scope line (`tasks.md:154`) is `[x]` with the maintainer disposition:
  "**NO EJECUTADO (2026-09-14)**: sin instrumento disponible en el banco; disposición del
  mantenedor: la validación conductual on-device ... cubre el comportamiento observable de
  este ítem opcional." This is the stale-checkbox reconciliation verify recommended; it is
  not a requirement failure.
- `apply-progress.md` still mirrors the same item as an unchecked working-log line
  (`:189`, "no ejecutado (sin osciloscopio)"). That file is an apply working log, not the
  sync gate; the authoritative ledger is `tasks.md`, which is 0-unchecked.

## Doc-consistency notes

- **README ↔ R7 rule**: `README.md:171-177` states the hard rule "Nunca conectes USB y
  batería a la vez: flashea por USB → desconecta el USB → conecta la batería", the bench
  motors-disconnected requirement, and the `INVERT_LEFT`/`INVERT_RIGHT` correction path.
  Consistent with R7 and the synced spec.
- **design ↔ shipped `INVERT_*` drift (intentional, documented)**: `design.md:44,169`
  freeze the default `INVERT_LEFT = false`, `INVERT_RIGHT = false`. Shipped
  `firmware/src/motors.rs:30` has `INVERT_LEFT = true` (R7 on-device direction correction
  after the battery-phase test; `INVERT_RIGHT` stays `false`). This is a **known,
  intentional, documented drift** resolved on-device via the spec-sanctioned correction
  path; the frozen forward/reverse convention is untouched. Recorded here so a future
  reader consults the on-device record, not only `design.md`. It does not affect the
  canonical spec copy (the spec describes the invert mechanism, not the shipped literal).

## Residual non-blocking observations (follow-ups, not sync blockers)

Carried from verify-report "Bench findings" and "Residual risks":

1. **Left-motor ~30 % duty deadband** — left motors do not start near duty ≈ 30 %
   (`v=0.3`→77/256); start at 70 %. Hardware/mechanical characteristic, not a spec
   violation. Possible follow-up: optional minimum-duty floor when `|speed|>ε`; needs its
   own scoped change.
2. **Slow-client dead-man pulses** — a client spacing `cmd:move` > ~1 s apart sees
   stop-between-commands (the 1000 ms dead-man by design). Expected R5 behavior; a
   client-side heartbeat/cadence contract or longer window is a separate design decision.
3. **Optional GPIO12–15 scope measurement not executed** — no instrument on the rig;
   dispositioned as above; norm PWM frequency/encoding verified statically (source-checked
   `TimerConfig::new()` / `max_duty()`) plus physical direction tests.

## Validation checks performed

- Re-read `verify-report.md`: confirmed YAML front-matter `verdict: pass`, `blockers: 0`,
  `critical_findings: 0`, `requirements: 7/7`, `scenarios: 32/32`.
- Scanned `tasks.md` for `^\s*- \[ \]`: 0 unchecked (37 total); confirmed the optional line
  disposition at `:154`.
- Confirmed no canonical `openspec/specs/firmware-motors/` existed before writing (new
  domain; `firmware-network` is the only other canonical domain, untouched).
- `sha256sum` on source and dest (identical) + `cmp` byte-equality after copy.
- Confirmed no `## ADDED/MODIFIED/REMOVED/RENAMED Requirements` delta headers (full new-
  domain spec), so no merge/destructive/unsupported-rename path applies.
- Grep-confirmed `INVERT_LEFT` default (design `false` vs shipped `true`) and README rule
  text for the doc-consistency notes.

## Destructive sync / approval

None. New-domain copy only: no REMOVED requirements, no large MODIFIED blocks. No explicit
destructive-approval required and none consumed.

## Same-domain collisions

None. `sameDomainActiveChanges: []`, `collisions: []`. `firmware-motors` is a new domain;
no other active change touches `specs/firmware-motors/spec.md`.

## Archive readiness

**Ready for `sdd-archive`.** Clean verify (pass / 0 blockers / 0 critical), completed sync
(this report), 0 unchecked implementation tasks, no collisions, no destructive-sync gate.
The one archive caveat raised in `verify-report.md` (the optional unchecked scope line) has
been resolved by the stale-checkbox reconciliation in `tasks.md`, which is the exact remedy
that report prescribed. No CRITICAL issues have no-override status here because there are
none.
