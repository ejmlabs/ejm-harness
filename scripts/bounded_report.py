#!/usr/bin/env python3
"""Capture producer evidence without limiting test process files."""

import hashlib
import heapq
import importlib.util
import json
import os
import selectors
import stat
import subprocess
import sys
import time
from pathlib import Path
from typing import BinaryIO

spec = importlib.util.spec_from_file_location("junit_stream", Path(__file__).with_name("junit_stream.py"))
if spec is None or spec.loader is None:
    raise RuntimeError("Cannot load JUnit evidence collector")
junit_stream = importlib.util.module_from_spec(spec)
spec.loader.exec_module(junit_stream)


class VitestRecords:
    def __init__(self, limit: int) -> None:
        self.limit = max(0, limit - 64)
        self.buffer = b""
        self.discarding = False
        self.header = False
        self.finished = False
        self.incomplete = False
        self.seen = junit_stream.IdentityFilter()
        self.exact: set[str] = set()
        self.retained: set[str] = set()
        self.ambiguous: set[str] = set()
        self.records: list[tuple[tuple[int, int], str, bytes]] = []
        self.size = 0
        self.first_failure = False
        self.reference: set[str] = set()
        reference = os.environ.get("EJM_REGISTRY_VITEST_REFERENCE")
        if reference:
            try:
                path = Path(reference)
                if path.stat().st_size <= limit:
                    with path.open("rb") as source:
                        for line in source:
                            record = json.loads(line)
                            if record.get("kind") == "case" and record.get("status") == "failed":
                                self.reference.add(json.dumps([record["file"], record["name"]]))
            except (OSError, ValueError, KeyError, TypeError):
                self.reference.clear()

    def add(self, chunk: bytes) -> None:
        self.buffer += chunk
        while b"\n" in self.buffer:
            line, self.buffer = self.buffer.split(b"\n", 1)
            if self.discarding:
                self.discarding = False
                continue
            self.record(line)
        if len(self.buffer) > 65536:
            self.buffer = b""
            self.discarding = True
            self.incomplete = True

    def record(self, line: bytes) -> None:
        try:
            record = json.loads(line)
            kind = record["kind"]
            if kind == "start" and not self.header:
                self.header = True
                return
            if not self.header or self.finished:
                raise ValueError("invalid record order")
            if kind == "end":
                if type(record.get("incomplete")) is not bool:
                    raise ValueError("invalid end record")
                self.finished = True
                self.incomplete |= record["incomplete"]
                return
            if kind != "case":
                raise ValueError("invalid case")
            file, name, status = record["file"], record["name"], record["status"]
            if not isinstance(file, str) or not isinstance(name, str) or status not in ("passed", "failed", "skipped"):
                raise ValueError("invalid case")
            key = json.dumps([file, name])
            maybe_duplicate = self.seen.add(key)
            if len(self.exact) < 20_000:
                duplicate = key in self.exact
                self.exact.add(key)
            else:
                duplicate = maybe_duplicate
            if duplicate:
                if key in self.retained:
                    self.ambiguous.add(key)
                self.incomplete = True
                return
            first = status == "failed" and not self.first_failure
            self.first_failure |= status == "failed"
            self.incomplete |= record.get("truncated") is True
            message = record.get("message")
            message_limit = min(8192, self.limit // 4)
            if isinstance(message, str) and len(message) > message_limit:
                half = message_limit // 2
                record["message"] = message[:half] + "\n...\n" + (message[-half:] if half else "")
                self.incomplete = True
            raw = json.dumps(record, ensure_ascii=False, separators=(",", ":")).encode() + b"\n"
            priority = 3 if first else 2 if status == "failed" else 1 if key in self.reference else 0
            digest = int.from_bytes(hashlib.sha256(key.encode()).digest(), "big")
            rank = (priority, -digest)
            if len(raw) > self.limit:
                self.incomplete = True
                return
            evicted = []
            while self.size + len(raw) > self.limit or len(self.records) >= 20_000:
                if not self.records or self.records[0][0] >= rank:
                    for item in evicted:
                        heapq.heappush(self.records, item)
                        self.size += len(item[2])
                    self.incomplete = True
                    return
                item = heapq.heappop(self.records)
                evicted.append(item)
                self.size -= len(item[2])
            self.incomplete |= bool(evicted)
            for _, victim, _ in evicted:
                self.retained.remove(victim)
                self.ambiguous.discard(victim)
            heapq.heappush(self.records, (rank, key, raw))
            self.retained.add(key)
            self.size += len(raw)
        except (ValueError, KeyError, TypeError, RecursionError):
            self.incomplete = True

    def write(self, stream: BinaryIO, *, partial: bool = False) -> bool:
        incomplete = partial or self.incomplete or not self.header or not self.finished or bool(self.buffer)
        if self.header:
            stream.write(b'{"kind":"start"}\n')
            for _, key, line in sorted(self.records, key=lambda item: (-item[0][0], item[1])):
                if key not in self.ambiguous:
                    stream.write(line)
            stream.write(json.dumps({"kind": "end", "incomplete": incomplete}, separators=(",", ":")).encode() + b"\n")
        return incomplete


def capture(mode: str, output: Path, limit: int, command: list[str]) -> int:
    if os.name == "nt":
        raise OSError("bounded report capture is unsupported on Windows")
    fifo = Path(str(output) + ".fifo")
    reader = None
    keeper = None
    identity = None
    if mode in ("fifo", "fifo-jsonl", "fifo-xml"):
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
        records = VitestRecords(limit) if mode == "fifo-jsonl" else None
        junit = junit_stream.JunitRecords(limit, os.environ.get("EJM_HARNESS_JUNIT_REFERENCE")) if mode == "fifo-xml" else None
        with output.open("wb") as stream, selectors.DefaultSelector() as selector:
            dirty = False
            last_snapshot = time.monotonic()

            def snapshot(*, final: bool = False) -> None:
                nonlocal dirty, last_snapshot
                if dirty:
                    stream.seek(0)
                    stream.truncate()
                    if records is not None:
                        records.write(stream, partial=not final)
                    elif junit is not None:
                        stream.write(junit.snapshot())
                    stream.flush()
                    dirty = False
                    last_snapshot = time.monotonic()

            selector.register(reader, selectors.EVENT_READ)
            while True:
                ready = selector.select(timeout=0.1)
                if ready:
                    try:
                        chunk = os.read(reader, 65536)
                    except BlockingIOError:
                        chunk = b""
                    if chunk:
                        if junit is not None:
                            junit.add(chunk)
                        if records is not None:
                            records.add(chunk)
                            written += len(chunk)
                            dirty = True
                        else:
                            if not overflow:
                                retained = chunk[: max(0, limit - written)]
                                stream.write(retained)
                                stream.flush()
                                written += len(retained)
                                overflow |= len(retained) != len(chunk)
                            if overflow and junit is not None:
                                dirty = True
                        if dirty and time.monotonic() - last_snapshot >= 0.1:
                            snapshot()
                        continue
                snapshot()
                if process.poll() is not None:
                    break
            code = process.wait()
            if records is not None:
                overflow |= records.incomplete or not records.header or not records.finished or bool(records.buffer)
                dirty = True
                snapshot(final=True)
            if junit is not None:
                projection = junit.finish()
                if overflow:
                    stream.seek(0)
                    stream.truncate()
                    stream.write(projection)
                    stream.flush()
        valid_fifo = mode == "stdout" or (
            fifo.exists() and stat.S_ISFIFO(fifo.stat().st_mode) and fifo.stat().st_ino == identity
        )
        if overflow or not valid_fifo or not written:
            print(f"Evidence incomplete: {output}", file=sys.stderr)
            return 1
        return code
    finally:
        if keeper is not None:
            os.close(keeper)
        if mode in ("fifo", "fifo-jsonl", "fifo-xml"):
            if reader is not None:
                os.close(reader)
            fifo.unlink(missing_ok=True)


def main() -> int:
    if len(sys.argv) < 6 or sys.argv[4] != "--" or sys.argv[1] not in ("fifo", "fifo-jsonl", "fifo-xml", "stdout"):
        raise SystemExit("usage: bounded_report.py fifo|fifo-jsonl|fifo-xml|stdout OUTPUT LIMIT -- COMMAND [ARG...]")
    try:
        limit = int(sys.argv[3])
        if limit <= 0 or (sys.argv[1] in ("fifo-jsonl", "fifo-xml") and limit < 64):
            raise ValueError("invalid limit")
        return capture(sys.argv[1], Path(sys.argv[2]), limit, sys.argv[5:])
    except (OSError, ValueError) as error:
        print(f"Cannot capture bounded evidence: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
