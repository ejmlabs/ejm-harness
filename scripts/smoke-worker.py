#!/usr/bin/env python3
"""Start a built ejm-harness-worker, check its handshake and ask it to shut down."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

PROTOCOL = 1
TIMEOUT_SECONDS = 20


def request(worker: subprocess.Popen[str], frame: dict[str, object]) -> dict[str, object]:
    """Send one frame and return the matching response."""
    if worker.stdin is None or worker.stdout is None:
        raise RuntimeError("worker pipes are not open")
    worker.stdin.write(json.dumps(frame) + "\n")
    worker.stdin.flush()
    line = worker.stdout.readline()
    if not line:
        raise RuntimeError("worker closed stdout before answering")
    response = json.loads(line)
    if not isinstance(response, dict) or response.get("id") != frame["id"]:
        raise RuntimeError(f"unexpected response: {line.strip()}")
    if "error" in response:
        raise RuntimeError(f"worker error: {response['error']}")
    return response


def handshake_and_shutdown(worker: subprocess.Popen[str], version: str) -> None:
    """Check the handshake, then stop the worker cleanly."""
    hello = request(worker, {"id": "1", "method": "hello", "params": {"protocol": PROTOCOL}})
    result = hello.get("result")
    if not isinstance(result, dict):
        raise TypeError(f"no handshake result: {hello}")
    if result.get("protocol") != PROTOCOL or result.get("version") != version:
        raise RuntimeError(f"unexpected handshake: {result}")
    print(f"handshake ok: protocol {result['protocol']} version {result['version']}")
    request(worker, {"id": "2", "method": "shutdown", "params": {}})
    if worker.stdin is not None:
        worker.stdin.close()
    if worker.wait(timeout=TIMEOUT_SECONDS) != 0:
        raise RuntimeError(f"worker exited with status {worker.returncode}")
    print("shutdown ok")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("worker", type=Path, help="path to the ejm-harness-worker binary")
    parser.add_argument("--version", required=True, help="expected worker version")
    args = parser.parse_args()
    try:
        worker = subprocess.Popen(
            [str(args.worker)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
    except OSError as error:
        print(f"smoke failed: cannot start the worker: {error}", file=sys.stderr)
        return 1
    with worker:
        try:
            handshake_and_shutdown(worker, args.version)
        except (RuntimeError, TypeError, subprocess.TimeoutExpired, ValueError) as error:
            worker.kill()
            print(f"smoke failed: {error}", file=sys.stderr)
            return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
