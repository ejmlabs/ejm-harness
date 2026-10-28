"""Offline subprocess fixtures for the trusted language hosts."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path
from queue import Empty, Queue
from typing import Any

RUNTIME = Path(__file__).parent
NODE = os.environ.get("EJM_HARNESS_TEST_NODE", shutil.which("node"))
NODE_ENTRY = """
import { writeFile } from 'node:fs/promises';
export async function echo(args, ctx) {
  console.log('package diagnostic');
  await ctx.progress('ready');
  await ctx.delta('piece');
  return {args, session: ctx.session};
}
export async function wait(args, ctx) {
  await ctx.progress('started');
  await new Promise(resolve => {
    if (ctx.signal.aborted) resolve();
    else ctx.signal.addEventListener('abort', resolve, {once: true});
  });
  return null;
}
export async function slow(args, ctx) {
  await ctx.progress('started');
  await new Promise(resolve => setTimeout(resolve, 200));
  await writeFile(args.marker, 'finished');
  return null;
}
export async function oversize(args, ctx) { return 'x'.repeat(1024 * 1024); }
export async function progress_overflow(args, ctx) { await ctx.progress('x'.repeat(2049)); }
"""
PYTHON_ENTRY = """
import asyncio
import time
from pathlib import Path

async def echo(args, ctx):
    print('package diagnostic')
    await ctx.progress('ready')
    await ctx.delta('piece')
    return {'args': args, 'session': ctx['session']}

async def wait(args, ctx):
    await ctx.progress('started')
    while not ctx.is_cancelled():
        await asyncio.sleep(0.005)

def slow(args, ctx):
    ctx.progress_sync('started')
    time.sleep(0.2)
    Path(args['marker']).write_text('finished')

async def oversize(args, ctx):
    return 'x' * (1024 * 1024)

async def progress_overflow(args, ctx):
    await ctx.progress('x' * 2049)
"""
HANDLERS = ["echo", "wait", "slow", "oversize", "progress_overflow"]


class Client:
    def __init__(self, language: str, root: Path) -> None:
        self.entry = "entry.mjs" if language == "node" else "entry.py"
        (root / self.entry).write_text(NODE_ENTRY if language == "node" else PYTHON_ENTRY)
        node = NODE
        if node is None:
            raise RuntimeError("Node is required to verify both extension hosts")
        command = (
            [node, str(RUNTIME / "node.mjs")]
            if language == "node"
            else [sys.executable, "-I", "-B", str(RUNTIME / "python.py")]
        )
        self.process = subprocess.Popen(
            command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE
        )
        self.frames: Queue[dict[str, Any] | BaseException] = Queue()
        self.diagnostics = bytearray()
        self.root = root

        def read_output() -> None:
            assert self.process.stdout is not None
            for line in self.process.stdout:
                try:
                    self.frames.put(json.loads(line))
                except ValueError as error:
                    self.frames.put(error)

        def read_diagnostics() -> None:
            assert self.process.stderr is not None
            while chunk := self.process.stderr.read(1024):
                self.diagnostics.extend(chunk)

        self.reader = threading.Thread(target=read_output, daemon=True)
        self.diagnostic_reader = threading.Thread(target=read_diagnostics, daemon=True)
        self.reader.start()
        self.diagnostic_reader.start()

    def send(self, request_id: str | None, method: str, **params: object) -> None:
        frame = {"method": method, "params": params}
        if request_id is not None:
            frame["id"] = request_id
        assert self.process.stdin is not None
        self.process.stdin.write(json.dumps(frame).encode() + b"\n")
        self.process.stdin.flush()

    def next(self, timeout: float = 5) -> dict[str, Any]:
        value = self.frames.get(timeout=timeout)
        if isinstance(value, BaseException):
            raise value
        return value

    def load(self, request_id: str = "load", **overrides: object) -> dict[str, Any]:
        params: dict[str, object] = {
            "package_id": "fixture",
            "root": str(self.root),
            "entry": self.entry,
            "handlers": HANDLERS,
        }
        params.update(overrides)
        self.send(request_id, "load", **params)
        return self.next()

    def invoke(self, request_id: str, handler: str, **args: object) -> None:
        self.send(
            request_id,
            "invoke",
            package_id="fixture",
            handler=handler,
            args=args,
            context={"session": "s1"},
        )

    def close(self) -> None:
        try:
            if self.process.poll() is None:
                self.send("shutdown", "shutdown")
                self.process.wait(timeout=5)
        finally:
            if self.process.poll() is None:
                self.process.kill()
                self.process.wait()
            for stream in (self.process.stdin, self.process.stdout, self.process.stderr):
                if stream is not None:
                    stream.close()
            self.reader.join(timeout=1)
            self.diagnostic_reader.join(timeout=1)


class HostTests(unittest.TestCase):
    def languages(self) -> list[str]:
        self.assertIsNotNone(NODE, "Node is required to verify both extension hosts")
        return ["node", "python"]

    def test_progress_result_and_stdout_isolation(self) -> None:
        for language in self.languages():
            with self.subTest(language=language), tempfile.TemporaryDirectory() as folder:
                client = Client(language, Path(folder))
                try:
                    self.assertIn("result", client.load())
                    client.invoke("echo", "echo", value=7)
                    self.assertEqual(client.next()["method"], "progress")
                    self.assertEqual(client.next()["method"], "delta")
                    self.assertEqual(
                        client.next(),
                        {"id": "echo", "result": {"args": {"value": 7}, "session": "s1"}},
                    )
                    client.close()
                    self.assertIn(b"package diagnostic", client.diagnostics)
                    self.assertEqual(client.process.returncode, 0)
                finally:
                    client.close()

    def test_cancel_and_busy_unload(self) -> None:
        for language in self.languages():
            with self.subTest(language=language), tempfile.TemporaryDirectory() as folder:
                client = Client(language, Path(folder))
                try:
                    self.assertIn("result", client.load())
                    client.invoke("wait", "wait")
                    self.assertEqual(client.next()["method"], "progress")
                    client.send("unload", "unload", package_id="fixture")
                    self.assertIn("error", client.next())
                    client.send(None, "cancel", id="wait")
                    self.assertEqual(client.next()["id"], "wait")
                    client.send("unload2", "unload", package_id="fixture")
                    self.assertEqual(client.next(), {"id": "unload2", "result": None})
                finally:
                    client.close()

    def test_cancel_waits_for_real_work(self) -> None:
        for language in self.languages():
            with self.subTest(language=language), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                marker = root / "finished"
                client = Client(language, root)
                try:
                    self.assertIn("result", client.load())
                    client.invoke("slow", "slow", marker=str(marker))
                    self.assertEqual(client.next()["method"], "progress")
                    client.send(None, "cancel", id="slow")
                    with self.assertRaises(Empty):
                        client.next(timeout=0.05)
                    self.assertIn("error", client.next())
                    self.assertEqual(marker.read_text(), "finished")
                finally:
                    client.close()

    def test_bounds_and_export_validation(self) -> None:
        for language in self.languages():
            with self.subTest(language=language), tempfile.TemporaryDirectory() as folder:
                client = Client(language, Path(folder))
                try:
                    self.assertIn("error", client.load(handlers=["echo", "echo"]))
                    self.assertIn("error", client.load(handlers=["missing"]))
                    self.assertIn("result", client.load())
                    for handler in ("oversize", "progress_overflow"):
                        client.invoke(handler, handler)
                        self.assertIn("error", client.next())
                    client.send("missing", "invoke", package_id="fixture", handler="unregistered")
                    self.assertIn("error", client.next())
                finally:
                    client.close()

    def test_entry_symlink_cannot_escape_root(self) -> None:
        for language in self.languages():
            with self.subTest(language=language), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                client = Client(language, root)
                try:
                    (root / "escape").symlink_to(
                        RUNTIME / ("node.mjs" if language == "node" else "python.py")
                    )
                    self.assertIn("error", client.load(entry="escape"))
                finally:
                    client.close()

    def test_python_host_exits_when_input_is_closed(self) -> None:
        launcher = (
            "import runpy, sys; sys.stdin.close(); "
            f"runpy.run_path({str(RUNTIME / 'python.py')!r}, run_name='__main__')"
        )
        result = subprocess.run(
            [sys.executable, "-I", "-B", "-c", launcher],
            capture_output=True,
            timeout=10,
            check=False,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn(b"closed file", result.stderr)

    def test_shutdown_exits_with_stdin_pipe_owner_alive(self) -> None:
        for language in self.languages():
            with self.subTest(language=language), tempfile.TemporaryDirectory() as folder:
                client = Client(language, Path(folder))
                try:
                    client.send("shutdown", "shutdown")
                    self.assertEqual(client.next(), {"id": "shutdown", "result": None})
                    client.process.wait(timeout=2)
                    self.assertEqual(client.process.returncode, 0)
                finally:
                    client.close()

    def test_input_frame_limit(self) -> None:
        for language in self.languages():
            with self.subTest(language=language), tempfile.TemporaryDirectory() as folder:
                client = Client(language, Path(folder))
                try:
                    assert client.process.stdin is not None
                    client.process.stdin.write(b"x" * (1024 * 1024 + 2))
                    client.process.stdin.flush()
                    client.process.wait(timeout=2)
                    self.assertEqual(client.process.returncode, 1)
                finally:
                    client.close()

    def test_full_work_pool_still_accepts_shutdown(self) -> None:
        for language in self.languages():
            with self.subTest(language=language), tempfile.TemporaryDirectory() as folder:
                client = Client(language, Path(folder))
                try:
                    self.assertIn("result", client.load())
                    for index in range(64):
                        client.invoke(f"wait{index}", "wait")
                    for _index in range(64):
                        self.assertEqual(client.next()["method"], "progress")
                    client.invoke("excess", "echo")
                    self.assertEqual(client.next()["id"], "excess")
                    client.send("shutdown", "shutdown")
                    results = [client.next() for _index in range(65)]
                    self.assertEqual(results[-1], {"id": "shutdown", "result": None})
                    self.assertTrue(all("error" in result for result in results[:-1]))
                    client.process.wait(timeout=2)
                    self.assertEqual(client.process.returncode, 0)
                finally:
                    client.close()

    def test_lifetime_load_limit_requires_restart(self) -> None:
        for language in self.languages():
            with self.subTest(language=language), tempfile.TemporaryDirectory() as folder:
                client = Client(language, Path(folder))
                try:
                    for index in range(512):
                        self.assertIn("result", client.load(f"load{index}", handlers=[]))
                        client.send(f"unload{index}", "unload", package_id="fixture")
                        self.assertIn("result", client.next())
                    self.assertIn("restart", client.load("excess")["error"]["message"])
                finally:
                    client.close()

    def test_framing_rejects_invalid_utf8_and_incomplete_last_line(self) -> None:
        for language in self.languages():
            for raw in (
                b'{"id":"bad","method":"shutdown","params":{"text":"\xff"}}\n',
                b'{"id":"incomplete","method":"shutdown","params":{}}',
            ):
                with (
                    self.subTest(language=language, raw=raw),
                    tempfile.TemporaryDirectory() as folder,
                ):
                    client = Client(language, Path(folder))
                    try:
                        assert client.process.stdin is not None
                        client.process.stdin.write(raw)
                        client.process.stdin.flush()
                        client.process.stdin.close()
                        client.process.wait(timeout=2)
                        self.assertEqual(client.process.returncode, 1)
                        self.assertTrue(client.frames.empty())
                    finally:
                        client.close()

    def test_split_utf8_and_crlf(self) -> None:
        for language in self.languages():
            with self.subTest(language=language), tempfile.TemporaryDirectory() as folder:
                client = Client(language, Path(folder))
                try:
                    self.assertIn("result", client.load())
                    frame = {
                        "id": "utf8",
                        "method": "invoke",
                        "params": {
                            "package_id": "fixture",
                            "handler": "echo",
                            "args": {"text": "雪"},
                            "context": {"session": "utf8"},
                        },
                    }
                    raw = json.dumps(frame, ensure_ascii=False).encode() + b"\r\n"
                    split = raw.index("雪".encode()) + 1
                    assert client.process.stdin is not None
                    client.process.stdin.write(raw[:split])
                    client.process.stdin.flush()
                    time.sleep(0.01)
                    client.process.stdin.write(raw[split:])
                    client.process.stdin.flush()
                    self.assertEqual(client.next()["method"], "progress")
                    self.assertEqual(client.next()["method"], "delta")
                    self.assertEqual(client.next()["result"]["args"], {"text": "雪"})
                finally:
                    client.close()


if __name__ == "__main__":
    unittest.main()
