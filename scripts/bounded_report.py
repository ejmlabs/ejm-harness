#!/usr/bin/env python3
"""Capture producer evidence without limiting test process files."""

import os
import selectors
import stat
import subprocess
import sys
from pathlib import Path


def capture(mode: str, output: Path, limit: int, command: list[str]) -> int:
    fifo = Path(str(output) + ".fifo")
    reader = None
    keeper = None
    identity = None
    if mode == "fifo":
        if not hasattr(os, "mkfifo"):
            raise OSError("bounded report capture requires POSIX FIFOs")
        os.mkfifo(fifo, 0o600)
        identity = fifo.stat().st_ino
        reader = os.open(fifo, os.O_RDONLY | os.O_NONBLOCK)
        keeper = os.open(fifo, os.O_WRONLY | os.O_NONBLOCK)
    try:
        process = subprocess.Popen(command, stdout=subprocess.PIPE if mode == "stdout" else None)
        if mode == "stdout":
            assert process.stdout is not None
            reader = process.stdout.fileno()
            os.set_blocking(reader, False)
        assert reader is not None
        written = 0
        overflow = False
        with output.open("wb") as stream, selectors.DefaultSelector() as selector:
            selector.register(reader, selectors.EVENT_READ)
            while True:
                ready = selector.select(timeout=0.1)
                if ready:
                    try:
                        chunk = os.read(reader, 65536)
                    except BlockingIOError:
                        chunk = b""
                    if chunk:
                        retained = chunk[: max(0, limit - written)]
                        stream.write(retained)
                        written += len(retained)
                        overflow |= len(retained) != len(chunk)
                        continue
                if process.poll() is not None:
                    break
            code = process.wait()
        valid_fifo = mode != "fifo" or (
            fifo.exists() and stat.S_ISFIFO(fifo.stat().st_mode) and fifo.stat().st_ino == identity
        )
        if overflow or not valid_fifo or not written:
            print(f"Evidence incomplete: {output}", file=sys.stderr)
            return 1
        return code
    finally:
        if keeper is not None:
            os.close(keeper)
        if mode == "fifo":
            if reader is not None:
                os.close(reader)
            fifo.unlink(missing_ok=True)


def main() -> int:
    if len(sys.argv) < 6 or sys.argv[4] != "--" or sys.argv[1] not in ("fifo", "stdout"):
        raise SystemExit("usage: bounded_report.py fifo|stdout OUTPUT LIMIT -- COMMAND [ARG...]")
    try:
        limit = int(sys.argv[3])
        if limit <= 0:
            raise ValueError("invalid limit")
        return capture(sys.argv[1], Path(sys.argv[2]), limit, sys.argv[5:])
    except (OSError, ValueError) as error:
        print(f"Cannot capture bounded evidence: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
