#!/usr/bin/env python3
"""Measure deterministic ejm-harness workloads and ejm-cli first paint."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import platform
import select
import signal
import subprocess
import sys
import tempfile
import time
from datetime import UTC, datetime
from pathlib import Path
from typing import Any


def process_memory(pid: int) -> tuple[int, int]:
    """Return root RSS and summed descendant RSS, both in bytes."""
    output = subprocess.check_output(["ps", "-axo", "pid=,ppid=,rss="], text=True)
    rows = {}
    for line in output.splitlines():
        fields = line.split()
        if len(fields) == 3:
            child, parent, kib = map(int, fields)
            rows[child] = (parent, kib * 1024)
    descendants = {pid}
    while True:
        expanded = descendants | {
            child for child, (parent, _) in rows.items() if parent in descendants
        }
        if expanded == descendants:
            break
        descendants = expanded
    return rows.get(pid, (0, 0))[1], sum(
        rss for child, (_, rss) in rows.items() if child in descendants
    )


def measure_workload(binary: Path, agents: int, parked: bool) -> dict[str, Any]:
    command = [str(binary), "--agents", str(agents), "--rounds", "3", "--park-ms", "1000"]
    if parked:
        command.append("--parked")
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    peak_root = peak_tree = 0
    samples = 0
    deadline = time.monotonic() + 90
    started = time.monotonic()
    try:
        while process.poll() is None:
            if time.monotonic() >= deadline:
                raise TimeoutError("harness workload exceeded 90 seconds")
            root, tree = process_memory(process.pid)
            peak_root = max(peak_root, root)
            peak_tree = max(peak_tree, tree)
            samples += 1
            time.sleep(0.02)
        stdout, stderr = process.communicate(timeout=5)
        if process.returncode:
            raise RuntimeError(f"workload failed: {stderr[-4000:]}")
        result: dict[str, Any] = json.loads(stdout)
        result.update(
            sampled_peak_engine_rss_bytes=peak_root,
            sampled_peak_process_tree_rss_bytes=peak_tree,
            rss_samples=samples,
            rss_minimum_sample_interval_ms=20,
            elapsed_ms=round((time.monotonic() - started) * 1000, 3),
        )
        return result
    finally:
        if process.poll() is None:
            process.kill()
            process.communicate(timeout=5)


def first_paint(binary: Path, state: Path) -> float:
    if sys.platform == "win32":
        raise RuntimeError("first-paint measurements require Linux or macOS")

    import fcntl
    import pty
    import struct
    import termios

    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 100, 0, 0))
    environment = os.environ.copy()
    environment["TERM"] = "xterm-256color"

    def controlling_terminal() -> None:
        os.setsid()
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)

    started = time.perf_counter()
    process = subprocess.Popen(
        [str(binary), "--replay", "--state-dir", str(state)],
        stdin=slave,
        stdout=slave,
        stderr=slave,
        env=environment,
        preexec_fn=controlling_terminal,
    )
    output = bytearray()
    try:
        while b"What shall we build?" not in output:
            if time.perf_counter() - started > 10 or process.poll() is not None:
                raise RuntimeError(f"CLI did not paint: {bytes(output[-1000:])!r}")
            if select.select([master], [], [], 0.01)[0]:
                output.extend(os.read(master, 65536))
                if len(output) > 1024 * 1024:
                    raise RuntimeError("unexpected terminal output volume")
        elapsed = (time.perf_counter() - started) * 1000
        os.write(master, b"/quit\r")
        deadline = time.monotonic() + 10
        while process.poll() is None:
            if time.monotonic() >= deadline:
                raise TimeoutError("CLI did not exit after first paint")
            # Draining remains necessary while the terminal restores its screen.
            if select.select([master], [], [], 0.02)[0]:
                os.read(master, 65536)
        if process.returncode:
            raise RuntimeError(f"CLI exited with {process.returncode}")
        return round(elapsed, 3)
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)
        os.close(master)
        os.close(slave)


def startup(binary: Path, iterations: int) -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="ejm-harness-startup-") as folder:
        state = Path(folder)
        cold_state = first_paint(binary, state)
        samples = [first_paint(binary, state) for _ in range(iterations)]
    ordered = sorted(samples)
    return {
        "metric": "process spawn to first composer paint over a real PTY",
        "first_fresh_state_ms": cold_state,
        "warm_samples_ms": samples,
        "warm_p50_ms": ordered[math.ceil(len(ordered) * 0.50) - 1],
        "warm_p95_ms": ordered[math.ceil(len(ordered) * 0.95) - 1],
    }


def host_facts() -> dict[str, object]:
    """Record hardware so a result cannot be mistaken for target acceptance."""
    facts: dict[str, object] = {
        "platform": platform.platform(),
        "architecture": platform.machine(),
        "logical_cpus": os.cpu_count(),
    }
    if sys.platform == "darwin":
        facts["model"] = subprocess.check_output(["sysctl", "-n", "hw.model"], text=True).strip()
        facts["physical_memory_bytes"] = int(
            subprocess.check_output(["sysctl", "-n", "hw.memsize"], text=True).strip()
        )
    elif sys.platform.startswith("linux"):
        facts["physical_memory_bytes"] = os.sysconf("SC_PHYS_PAGES") * os.sysconf("SC_PAGE_SIZE")
    return facts


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--benchmark-bin", type=Path, default=Path("target/release/ejm-harness-bench")
    )
    parser.add_argument("--cli-bin", type=Path, default=Path("../ejm-cli/target/release/ejm-cli"))
    parser.add_argument("--agents", type=int, default=100)
    parser.add_argument("--iterations", type=int, default=20)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if os.name != "posix":
        parser.error("the RSS and PTY measurements currently require Linux or macOS")
    if not 1 <= args.agents <= 1000 or not 2 <= args.iterations <= 100:
        parser.error("agents must be 1..1000 and iterations 2..100")
    binary = args.benchmark_bin.resolve(strict=True)
    cli = args.cli_bin.resolve(strict=True)
    report = {
        "recorded_at": datetime.now(UTC).isoformat(),
        "host": host_facts(),
        "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
        "profile": "release",
        "binary_sha256": {
            "ejm_harness_bench": hashlib.file_digest(binary.open("rb"), "sha256").hexdigest(),
            "ejm_cli": hashlib.file_digest(cli.open("rb"), "sha256").hexdigest(),
        },
        "workloads": [measure_workload(binary, args.agents, mode) for mode in (True, False)],
        "cli_startup": startup(cli, args.iterations),
        "limits": [
            "Deterministic provider fixtures, not hosted API latency or coding quality.",
            "RSS samples can miss short peaks; summed tree RSS double counts shared pages.",
            "Benchmark monitor and SDK hosts are excluded from engine RSS.",
            "A modern host result does not validate the four core, eight GB target machine.",
        ],
    }
    output = json.dumps(report, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(output)
    sys.stdout.write(output)


if __name__ == "__main__":
    main()
