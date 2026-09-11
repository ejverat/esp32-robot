#!/usr/bin/env python3
"""Strap-safe serial capture for the ESP32-CAM (CH340 + MB adapter).

Opening the serial port can toggle DTR/RTS and strap GPIO0 low on this rig,
which drops the chip into ROM download mode. This script opens the port,
immediately releases both lines (GPIO0 high, EN high), optionally pulses RTS
to reset into a normal boot, then prints every line it receives.

Usage (inside the firmware FHS wrapper, which provides python3):
    python3 assets/serial_capture.py --reset --seconds 30
"""

import argparse
import fcntl
import os
import select
import struct
import termios
import time

TIOCM_DTR = 0x002
TIOCM_RTS = 0x004


def set_lines(fd, dtr, rts):
    """Set DTR/RTS state via TIOCMSET (stty cannot drive these lines)."""
    current = struct.unpack("i", fcntl.ioctl(fd, termios.TIOCMGET, struct.pack("i", 0)))[0]
    current = (current | TIOCM_DTR) if dtr else (current & ~TIOCM_DTR)
    current = (current | TIOCM_RTS) if rts else (current & ~TIOCM_RTS)
    fcntl.ioctl(fd, termios.TIOCMSET, struct.pack("i", current))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", default="/dev/ttyUSB0")
    parser.add_argument("--baud", type=int, default=115200)
    parser.add_argument("--seconds", type=float, default=30.0)
    parser.add_argument("--reset", action="store_true", help="pulse RTS to reset into a normal boot")
    args = parser.parse_args()

    fd = os.open(args.port, os.O_RDWR | os.O_NOCTTY)
    attrs = termios.tcgetattr(fd)
    attrs[2] = termios.CS8 | termios.CREAD | termios.CLOCAL
    attrs[4] = attrs[5] = args.baud
    termios.tcsetattr(fd, termios.TCSANOW, attrs)

    # Release first: the open-time glitch must not strap GPIO0 low.
    set_lines(fd, False, False)
    time.sleep(0.3)
    if args.reset:
        set_lines(fd, False, True)   # EN low, GPIO0 stays high
        time.sleep(0.4)
        set_lines(fd, False, False)  # release -> normal boot

    buf = b""
    deadline = time.time() + args.seconds
    while time.time() < deadline:
        ready, _, _ = select.select([fd], [], [], 0.3)
        if not ready:
            continue
        try:
            chunk = os.read(fd, 1024)
        except OSError:
            break
        if chunk:
            buf += chunk
            while b"\n" in buf:
                line, buf = buf.split(b"\n", 1)
                print(line.decode("latin1", errors="replace").rstrip("\r"), flush=True)
    os.close(fd)


if __name__ == "__main__":
    main()
