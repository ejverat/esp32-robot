# Sync Report — firmware-wifi-ws-client

> Backend: `openspec`. Executor: SDD sync. Canonical spec merge performed into
> `openspec/specs/`. The change folder was **not** moved to archive.

## Verdict

**`syncState: ready`**. The delta spec for domain `firmware-network` has been synced into
the canonical spec at `openspec/specs/firmware-network/spec.md`. All three blockers from the
previous sync run are resolved and re-verified; the change is clean for archive.

## Executive summary

All three previously-reported blockers are confirmed fixed on disk:

1. **On-device checkboxes persisted.** `tasks.md` "On-device verification sequence" now has
   all 4 items `[x]` with inline evidence lines (flash+observe, hub-restart reconnect,
   erase-flash first boot, `.bin` size).
2. **Verify report regenerated (Revision 2).** `verify-report.md` now reads
   `verdict: pass`, `blockers: 0`, `requirements: 10/10`, `scenarios: 25/25`, with the
   requirement table on-device column PASS for R1/R3/R4/R6 and an "On-device verification
   (completed)" section. `evidence_revision` and `build_output_hash` match the settled apply
   candidate.
3. **Credential spec violation resolved.** `firmware/src/config.rs` defaults are back to the
   `CHANGE_ME_*` placeholders and the placeholder warning is restored; real credentials live
   only in device NVS. The password string no longer appears anywhere in the repository
   (it was removed along with the old sync report).

The canonical spec was created by byte-identical copy of the delta spec (new domain; no
prior canonical spec existed). Delta has no `## ADDED/MODIFIED/REMOVED/RENAMED` headers, so
no requirement-name matching or destructive-sync approval was needed.

## Canonical files updated

| File | Action |
|------|--------|
| `openspec/specs/firmware-network/spec.md` | **Created** (copied from `openspec/changes/firmware-wifi-ws-client/specs/firmware-network/spec.md`) |

- Delta sha256: `4de894e97ff84545b7341a25a33994163e8459ffeeb5c632c4825a6e99bdb7dc`
- Canonical sha256: `4de894e97ff84545b7341a25a33994163e8459ffeeb5c632c4825a6e99bdb7dc`
- `diff` empty → byte-identical.

## Requirement delta type

Full new-domain spec. No `## ADDED Requirements`, `## MODIFIED Requirements`,
`## REMOVED Requirements`, or `## RENAMED Requirements` sections present. Nothing to
append/replace/delete; no RENAMED-unsupported path hit.

## Requirement-by-requirement sync table

10 requirements, 25 scenarios. Status legend: SAT = satisfied.

| # | Requirement | Status | Scenarios | Evidence |
|---|-------------|--------|-----------|----------|
| R1 | WiFi station + NVS-resolved credentials | **SAT** | 3/3 | `config.rs` `resolve()`/`read_or_default()` (get_str → non-empty wins → else `set_str` write-back); defaults are `CHANGE_ME_SSID`/`CHANGE_ME_PASSWORD`/`192.168.1.10:8080`/`a1`; grep confirms zero real credentials in source. On-device PASS. |
| R2 | Layered configuration resolution | **SAT** | 3/3 | `config.rs` key consts + NVS → default order; `DEFAULT_ROBOT_ID = "a1"`. |
| R3 | WebSocket client to hub | **SAT** | 3/3 | `ws_client.rs` `build_uri()` (`rsplit_once(':')`, `port.parse::<u16>()`), `percent_encode()`, `TransportOverTCP`, no greeting wait. On-device PASS. |
| R4 | Bounded backoff + STA re-entry + WS auto-reconnect | **SAT** | 4/4 | `wifi.rs` backoff cap 15 s; `StaDisconnected` → re-enter; `disable_auto_reconnect: false`; client created once. On-device PASS (firewall unblock + hub restart). |
| R5 | Explicit non-zero keepalive/client config | **SAT** | 3/3 | `ws_client.rs` `client_config()`: all required fields literal non-zero, `transport = TransportOverTCP`. |
| R6 | Connection state transitions logged | **SAT** | 2/2 | `wifi.rs` attempt/connect/disconnect logs + `ws_client.rs` `log_event()` maps every `WebSocketEventType`. On-device PASS. |
| R7 | Dependency + build contract | **SAT** | 3/3 | `Cargo.toml` `serde`/`serde_json` + `extra_components` `espressif/esp_websocket_client 1.1.0`; cross-compile exit 0. |
| R8 | Main stays alive without busy-spinning | **SAT** | 1/1 | `main.rs` `loop { rx.recv() }`. |
| R9 | No register envelope / heartbeat / `ts` | **SAT** | 1/1 | Zero `EspWebSocketClient::send(...)` sites; only `tx.send(NetEvent::…)`. |
| R10 | Non-goals + build-config stability | **SAT** | 2/2 | No motor/camera/telemetry/SNTP/TLS/auth; `sdkconfig.defaults` byte-identical. |

Summary: **10/10 requirements SAT, 25/25 scenarios SAT**.

## Acceptance criteria (proposal §9) — 11 criteria

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 1 | Cargo.toml deps + component + cross-compile | SAT | `Cargo.toml` serde/serde_json + `espressif/esp_websocket_client` 1.1.0; build exit 0. |
| 2 | NVS→defaults resolution + first-boot persist; no secrets beyond placeholders | SAT | `config.rs` resolution/persist; defaults are `CHANGE_ME_*`; grep shows no real credentials in source. |
| 3 | WiFi STA + bounded backoff + logging + re-entry | SAT | `wifi.rs`; on-device reconnect validated. |
| 4 | WS dial URI + URL-encode + `TransportOverTCP` | SAT | `ws_client.rs` `build_uri`/`percent_encode`/`client_config`. |
| 5 | Explicit non-zero config, `disable_auto_reconnect=false` | SAT | `ws_client.rs` `client_config()`. |
| 6 | Client created once, auto-reconnect, events logged, no greeting wait | SAT | `main.rs` (once, `_ws` held) + `ws_client.rs` `log_event`. |
| 7 | No register/heartbeat/`ts` | SAT | Zero `send()` sites. |
| 8 | `main()` alive, no busy-spin | SAT | `loop { rx.recv() }`. |
| 9 | `sdkconfig.defaults` unchanged | SAT | Byte-identical (apply-progress keep-out). |
| 10 | README firmware note (NVS, erase-flash, brownout) | SAT | Note present, Spanish, describes `CHANGE_ME_*` placeholders — now consistent with source. |
| 11 | Non-goals hold | SAT | No excluded subsystems. |

Summary: **11/11 SAT**.

## Task ledger state

- Tasks 1–7: 22/22 subtasks `[x]`.
- Keep-out checklist: 5/5 `[x]`.
- On-device sequence: 4/4 `[x]` with inline evidence.
- **Total: 31/31 `[x]`, 0 unchecked.**

## Documentation consistency

- `README.md` firmware note (Spanish) covers NVS credentials, the `erase-flash` clear path,
  and the brownout/power caveat, and describes the compiled defaults as `CHANGE_ME_*`
  placeholders — **consistent** with `config.rs`.
- `apply-progress.md` finding #1 records the final credential-handling state (defaults
  reverted to placeholders, warning restored, real credentials only in device NVS) —
  **consistent** with `config.rs`.
- `verify-report.md` Revision 2 evidence/revision hashes match the settled apply candidate.
- No real password string remains anywhere in the repository; the on-device SSID and hub IP
  appear only as verification evidence lines in `tasks.md`/`apply-progress.md`/`verify-report.md`
  (accepted on-device evidence, not source secrets).

## Structured status and actionContext findings

- Injected native status marked `dependencies.sync = "blocked"` and `nextRecommended =
  "sdd-verify"`; that status was authored before the maintainer's three fixes. On-disk
  re-verification shows the blockers are resolved, so sync proceeds. (The status
  `isNonAuthoritative` flag is `false`; this is an explicit maintainer-directed re-run whose
  fixes are confirmed on disk.)
- `actionContext.mode = "repo-local"`, `workspaceRoot` and `allowedEditRoots` =
  `/home/ejverat/Projects/esp32-robot`, no warnings. All written paths are inside the allowed
  root.
- Collisions: `relationships.sameDomainActiveChanges = []`, `collisions = []`. No other active
  change touches `firmware-network`; no archive/sync ordering decision needed.
- No `rules.sync` block present in `openspec/config.yaml` → default native helper/guardrail
  semantics applied.

## Residual non-blocking observations

1. **Brownout / TWDT are documented-only.** The brownout detector is untouched (risk
   documented in README); the TWDT risk is mitigated by bounded backoff, but any
   disable/feed-watchdog follow-up is deferred and not implemented. No spec requirement
   violated.
2. **WSS/TLS + auth deferred to Phase 4** (proposal non-goal) — as intended.
3. **CH340 strap quirk** (documented in `apply-progress.md` finding #3): the MB adapter can
   strap GPIO0 low on serial-open, landing the chip in ROM download mode; workaround is
   release DTR/RTS + RST pulse. Not a defect.
4. **NVS provisioning path is a future change.** Real credentials are provisioned into device
   NVS out-of-band; an in-band runtime provisioning path (serial/AP) is not part of this
   change and is recommended for a later phase.
5. On-device SSID and hub IP remain in the change artifacts' evidence lines; harmless but
   worth noting if the repository is ever made public.

## Validation checks performed

- Re-read `tasks.md`, `verify-report.md`, `apply-progress.md`, `config.rs`, `README.md`,
  `proposal.md`, `openspec/config.yaml`, and the delta spec.
- Grepped the repository for the real SSID and hub IP (found only in change-artifact evidence
  lines) and for the real password (found only in the old sync report, now removed).
- Confirmed the delta spec contains no `## ADDED/MODIFIED/REMOVED/RENAMED` headers.
- Confirmed canonical target `openspec/specs/` did not exist (new domain).
- Copied the delta spec and verified byte-identity via `sha256sum` + `diff`.

## Next recommended phase

**`sdd-archive`** — verification is clean (`verdict: pass`, 0 blockers), sync is complete,
and the ledger is 31/31 with zero unchecked implementation tasks. Archive target:
`openspec/changes/archive/YYYY-MM-DD-firmware-wifi-ws-client`.
