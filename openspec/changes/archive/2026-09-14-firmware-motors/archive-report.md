# Archive Report — firmware-motors

> Backend: `openspec`. Executor: SDD archive. Change folder moved to
> `openspec/changes/archive/2026-09-14-firmware-motors/`. Canonical spec left in place.

## Archive status

**PASS** — change archived.

## Executive summary

`firmware-motors` met every archive precondition, re-verified directly on disk before the move:

- `verify-report.md` has `verdict: pass`, `blockers: 0`, `critical_findings: 0`,
  `requirements: 7/7`, `scenarios: 32/32`, and an independent cross-compilation exit `0`
  (`sha256:414c0397…c8108`). No unresolved `FAIL`/`BLOCKED`/`CRITICAL` finding.
- `sync-report.md` is final with `syncState: ready`, `status: synced`, `next_recommended:
  sdd-archive`; the new canonical domain spec `openspec/specs/firmware-motors/spec.md` was created.
- `tasks.md` has **37/37 `[x]` and zero unchecked `- [ ]`** implementation tasks
  (re-scanned before the move: `grep -nE '^[[:space:]]*- \[ \]'` → no matches, exit 1;
  `grep -c -- '- \[x\]'` → 37; total checkbox lines → 37).
- The canonical spec `openspec/specs/firmware-motors/spec.md` is **byte-identical** to the delta
  (`sha256:e3587093ab8e5bda4ce61047e43e1bf833feaca3a0fb0b844e8d11f7f31999ab` for both; `cmp` empty).

No archive-time sync fallback was required (the `sdd-sync` phase had already completed a clean
copy) and no destructive merge was performed. The change folder was moved intact to the dated
archive; the canonical spec remains at `openspec/specs/firmware-motors/spec.md`.

## Artifacts read

| Artifact | Path |
|----------|------|
| Proposal | `openspec/changes/firmware-motors/proposal.md` |
| Spec (delta) | `openspec/changes/firmware-motors/specs/firmware-motors/spec.md` |
| Design | `openspec/changes/firmware-motors/design.md` |
| Tasks | `openspec/changes/firmware-motors/tasks.md` |
| Apply progress | `openspec/changes/firmware-motors/apply-progress.md` |
| Verify report | `openspec/changes/firmware-motors/verify-report.md` |
| Sync report | `openspec/changes/firmware-motors/sync-report.md` |
| Explore / preproposal | `openspec/changes/firmware-motors/explore.md`, `preproposal.md` |
| Canonical spec | `openspec/specs/firmware-motors/spec.md` |
| Config | `openspec/config.yaml` |

## Domains synced

| Domain | Canonical path | Sync source |
|--------|----------------|-------------|
| `firmware-motors` | `openspec/specs/firmware-motors/spec.md` | Delta change spec (new domain — byte-identical copy) |

Sync was completed by `sdd-sync` (`sync-report.md`, `syncState: ready`); archive performed **no**
additional spec write and did not modify canonical spec content. The canonical spec is left in
place (not archived away).

## Requirement delta names

The delta has no `## ADDED Requirements` / `## MODIFIED Requirements` / `## REMOVED Requirements`
sections — it is a full new-domain spec under a single `## Requirements` heading. Requirement
headings carried into the canonical spec (7 requirements, 32 scenarios):

- `### Requirement R1: Command intake (envelope v0 parsing)`
- `### Requirement R2: Differential-drive mixing with deadband and normalization`
- `### Requirement R3: LEDC PWM actuation with the frozen pin map`
- `### Requirement R4: Bring-up speed cap`
- `### Requirement R5: Stop policy (safety) — six triggers, one owner`
- `### Requirement R6: Task and architecture constraints`
- `### Requirement R7: On-device verification under the USB/battery safety rule`

ADDED: none (full new-domain spec copy). MODIFIED: none. REMOVED: none.

## Same-domain active change warnings

`relationships.sameDomainActiveChanges = []` and `collisions = []`. `firmware-motors` is a new
domain; the only other canonical domain (`firmware-network`, from the previously archived
`firmware-wifi-ws-client`) is untouched. No sync/archive ordering decision needed.

## Final task completion gate

Re-read `openspec/changes/firmware-motors/tasks.md` immediately before the move:

- `grep -nE '^[[:space:]]*- \[ \]' tasks.md` → **no matches** (exit 1).
- `grep -cE '^[[:space:]]*- \[x\]' tasks.md` → **37**; total checkbox lines → **37**.
- **Zero unchecked implementation task lines.**

## Stale-checkbox reconciliation (performed upstream, verified by archive)

`verify-report.md` was authored while `tasks.md:154` still held one unchecked line — the explicitly
**optional** GPIO12–15 scope measurement (no logic analyzer/oscilloscope on the rig). Verify kept the
verdict at `pass`/`blockers: 0` because the line is optional and the normative PWM frequency/encoding
behavior (R3) is verified statically (source-checked `TimerConfig::new()` = 1 kHz / 8-bit and
`max_duty()` = 256 in vendored `esp-idf-hal-0.46.2`) plus physically (direction tests), and it
prescribed stale-checkbox reconciliation before archive.

`sync-report.md` records that reconciliation: the line is now `[x]` with the maintainer disposition
"**NO EJECUTADO (2026-09-14)**: sin instrumento disponible en el banco; disposición del mantenedor:
la validación conductual on-device (sentidos L1/L2/R1/R2, dead-man ~1 s, corte de enlace,
reconexión) cubre el comportamiento observable de este ítem opcional." This is exactly the remedy
verify recommended.

**Archive performed no mechanical checkbox repair** — the reconciliation was already applied in
`tasks.md` upstream by `sdd-sync`/the maintainer. Archive only re-verified the final gate on disk
(0 unchecked, 37 checked, line 154 `[x]` with the disposition note). `apply-progress.md:189` still
mirrors the item as an unchecked working-log line; that is an apply log, not the authoritative
ledger — `tasks.md` is the gate and is 0-unchecked.

## Non-critical partial archive approval

Not required — the tasks ledger is fully reconciled and there is no partial archive. The optional
scope item is dispositioned as **not executed (instrument-gated)** rather than silently completed;
its observable behavior is covered by the on-device tests recorded in `apply-progress.md`.

## Structured status and actionContext findings

- Injected native status: `changeName: firmware-motors`, `artifactStore: openspec`,
  `isNonAuthoritative: false`, `applyState: all_done`, `taskProgress { total: 37, complete: 37,
  remaining: 0, unchecked: [] }`, `taskArtifactErrors: []`, `deferredParentActions: 0/0`.
- The same injected status carries `dependencies.sync: "blocked"`, `dependencies.archive: "blocked"`,
  and `nextRecommended: "sdd-verify"`. That status was authored **before** the final verify→sync→
  reconciliation sequence completed; `isNonAuthoritative` is `false`, so it is **stale**, not a
  non-authoritative carve-out. On-disk re-verification confirms every reported blocker is resolved:
  `verify-report.md` is `verdict: pass`/`0 blockers`, `sync-report.md` is `syncState: ready`, and
  `tasks.md` is 0-unchecked. This is the maintainer/parent-directed archive run whose preconditions
  are confirmed on disk. `blockedReasons: []` is empty.
- `actionContext.mode = "repo-local"`, `workspaceRoot` and `allowedEditRoots` =
  `/home/ejverat/Projects/esp32-robot`, no warnings. All read/written paths (change folder, canonical
  spec, archive target) are inside the authoritative edit root.
- `openspec/config.yaml` has no dedicated `rules.archive` block (its `rules:` list is the SDD
  authoring rules). The default OpenSpec archive convention was applied:
  `openspec/changes/archive/<YYYY-MM-DD>-<change>/`, matching the previously archived
  `firmware-wifi-ws-client`. The repo defines no different archive path.

## Destructive merge approvals or blockers

None. No REMOVED requirements, no MODIFIED blocks, no destructive canonical-spec write (new-domain
full copy). No explicit destructive-sync approval was needed or consumed.

## Archived path

```
openspec/changes/archive/2026-09-14-firmware-motors/
```

Pre-move contents (9 files) preserved, plus this `archive-report.md` (10 files, directory
`specs/firmware-motors/` preserved):

| File | Pre-move present |
|------|------------------|
| `proposal.md` | yes |
| `design.md` | yes |
| `tasks.md` | yes |
| `apply-progress.md` | yes |
| `verify-report.md` | yes |
| `sync-report.md` | yes |
| `explore.md` | yes |
| `preproposal.md` | yes |
| `specs/firmware-motors/spec.md` | yes |
| `archive-report.md` | added by archive |

## Integrity check (pre-move vs post-move)

- Pre-move: 9 files under `openspec/changes/firmware-motors/` (+ `archive-report.md` = 10 written
  before the move).
- Post-move: **10 files** under `openspec/changes/archive/2026-09-14-firmware-motors/` with the same
  set; **no discrepancies**. The `specs/firmware-motors/spec.md` delta remains present (audit trail)
  alongside the canonical copy.
- Canonical spec `openspec/specs/firmware-motors/spec.md` unchanged by archive:
  `sha256:e3587093ab8e5bda4ce61047e43e1bf833feaca3a0fb0b844e8d11f7f31999ab` (matches sync-report and
  delta).

## Canonical spec and implementation after archive

- `openspec/specs/firmware-motors/spec.md` remains in place (not archived away); content untouched
  by archive.
- `firmware/**` implementation files (`command.rs`, `motors.rs`, `ws_client.rs`, `main.rs`),
  `README.md`, and the project skill were **not** read-modified by archive. Archive performs only a
  report write and a folder move.

## Working-tree / delivery notes (informational; not archive blockers)

- The motors implementation changes are **uncommitted** in the working tree: modified
  `README.md`, `firmware/src/main.rs`, `firmware/src/ws_client.rs`; untracked
  `firmware/src/command.rs`, `firmware/src/motors.rs`; untracked `openspec/` artifacts and
  `openspec/specs/firmware-motors/`. Committing/publishing is a separate delivery decision owned by
  the parent/maintainer, not this archive phase.
- Out-of-scope working-tree edit recorded by verify: `.pi/skills/esp32-ondevice-workflow/SKILL.md`
  (+7 lines, USB/battery hard rule + troubleshooting entry). It is a harness/skill doc, unrelated to
  this change's declared authoring surfaces (design §2 lists only the firmware files and `README.md`),
  duplicates the rule already in `README.md`, and does not affect the firmware artifact or this
  archive. Left untouched by archive.

## Index / status file updates

None required. `openspec/` contains no change index/status file (no `changes/README.md`, no per-change
status file). The active change listing now simply no longer contains `firmware-motors` because the
folder moved to `archive/`.

## Observation IDs (Engram)

Not applicable — artifact store is `openspec` (Engram unavailable this session). All traceability is
file-based under `openspec/`.
