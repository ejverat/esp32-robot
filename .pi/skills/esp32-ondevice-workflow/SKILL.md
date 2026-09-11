---
name: esp32-ondevice-workflow
description: "Trigger: flash ESP32, espflash, serial capture, on-device verification, boot log, chip stuck in download mode. Run, capture, and verify this repo's ESP32-CAM firmware on real hardware."
license: Apache-2.0
metadata:
  author: gentleman-programming
  version: "1.0"
---

# ESP32-CAM On-Device Workflow

## Activation Contract

Use when flashing, monitoring, or verifying firmware on the physical ESP32-CAM
(AI-Thinker + MB adapter + CH340). Not for host-only builds or server work.

## Hard Rules

- Run builds/flashes through the FHS wrapper binary, invoked directly:
  `$(ls -d /nix/store/*-esp32-robot-firmware-fhs/bin/*firmware-fhs | head -1) -c 'cd firmware && cargo build'`.
  `nix develop .#firmware-fhs -c` drops the command (shellHook execs the wrapper).
- Never rely on `espflash monitor --non-interactive` for logs: it can leave the chip
  in download mode. Capture raw serial at 115200 instead.
- Opening `/dev/ttyUSB0` can strap GPIO0 low (CH340/MB auto-reset). After opening,
  immediately set DTR=0 and RTS=0 before reading; reset with an RTS pulse
  (RTS=1 for 0.4 s → RTS=0) while keeping DTR=0.
- `espflash flash` ends in download mode on this rig. Boot the app with the RST
  button, a USB power cycle, or the RTS pulse before expecting app logs.
- `waiting for download` loops with `rst:0xb TGWDT_CPU_RESET` are the ROM idle
  timeout, not an app watchdog failure.
- Flash preserves NVS; only `espflash erase-flash` clears it. The first boot after
  an erase writes the compiled defaults into NVS.
- The port is 660 root:dialout. If `sg`/`newgrp` fail (broken setuid wrappers after
  a nixos-rebuild), ask the user for `sudo chmod 666 /dev/ttyUSB0` — it reverts on
  replug; permanent fix is a udev rule for `idVendor 1a86`.

## Decision Gates

| Situation | Action |
| --- | --- |
| Need app boot logs | open port → release lines → RTS pulse → read ≥30 s |
| Chip loops in download mode | press RST / replug USB; stop toggling lines |
| No port permission | ask user for `chmod 666` (or the udev rule) |
| Verify connectivity | `curl http://<hub-ip>:8080/robots` — robot id appears after WS connect |
| Hub unreachable from LAN but `curl` works locally | NixOS firewall: `sudo iptables -I INPUT -p tcp --dport 8080 -j ACCEPT`; persistent: `networking.firewall.allowedTCPPorts = [ 8080 ]` |
| Credentials in source defaults | keep `CHANGE_ME_*` placeholders; real values live only in NVS |

## Execution Steps

1. Probe: wrapper `espflash board-info` → expect esp32 v3.1 / 4 MB.
2. Build: wrapper `cd firmware && cargo build`.
3. Flash: wrapper `cd firmware && espflash flash --port /dev/ttyUSB0 --baud 115200 target/xtensa-esp32-espidf/debug/esp32-robot-firmware`; record app size vs 4 MB.
4. Boot + capture: run `assets/serial_capture.py --reset` inside the wrapper env → expect `config resolved`, `wifi:connected`, `sta ip`, `WebSocket connected`.
5. Confirm on hub: `curl http://<hub-ip>:8080/robots` shows the robot id.
6. Reconnect test: restart the hub; poll `/robots` — reconnect within seconds, no device reset.
7. First-boot test: `espflash erase-flash` + flash + boot → `first boot: persisted defaults to NVS`.

## Output Contract

Report chip info, app size vs 4 MB, boot-log evidence lines, `/robots` result,
reconnect and first-boot outcomes, permission steps taken, and any strap quirks
observed.

## References

- `assets/serial_capture.py` — strap-safe serial capture (release lines, optional RTS reset).
- `firmware/src/config.rs` — NVS keys, defaults, first-boot write-back.
- `openspec/changes/archive/2026-09-11-firmware-wifi-ws-client/apply-progress.md` — on-device findings and check evidence.
