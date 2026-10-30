"""Bounded JSONL transport to one persistent worker process."""

from __future__ import annotations

import asyncio
import contextlib
import math
import os
from collections.abc import AsyncIterator, Coroutine, Sequence
from contextlib import asynccontextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import Any, cast
from uuid import uuid4

from .callbacks import ToolCallbacks
from .errors import HarnessError
from .events import EventFeed
from .frames import as_object, decode_frame, encode_frame
from .limits import (
    CONTROL_REQUESTS,
    MAX_FRAME,
    MAX_OUTPUT,
    MAX_PENDING,
    MAX_QUEUED,
    NORMAL_REQUESTS,
    SHUTDOWN_TIMEOUT,
)
from .protocol import Event, ToolInvocation
from .tools import Tool

CONTROL_METHODS = frozenset({"run.cancel", "run.steer", "run.queue", "shutdown"})


@dataclass
class _Lane:
    """In-flight slots for one kind of request, with a bounded queue of requests waiting for one."""

    slots: asyncio.Semaphore
    max_queued: int
    max_bytes: int
    queued: int = 0
    queued_bytes: int = 0

    @asynccontextmanager
    async def admit(self, size: int) -> AsyncIterator[None]:
        """Wait in the queue for a slot, then hold it for the body."""
        if self.queued >= self.max_queued or self.queued_bytes + size > self.max_bytes:
            raise HarnessError("busy", "request admission queue reached its count or byte limit")
        self.queued += 1
        self.queued_bytes += size
        try:
            await self.slots.acquire()
        finally:
            self.queued -= 1
            self.queued_bytes -= size
        try:
            yield
        finally:
            self.slots.release()


class Transport:
    """One worker process shared by every session. A failure poisons it and rejects pending work."""

    def __init__(
        self,
        *,
        tools: Sequence[Tool[Any]],
        worker_path: str | Path | None,
        request_timeout: float,
    ) -> None:
        if not math.isfinite(request_timeout) or request_timeout <= 0:
            raise ValueError("request_timeout must be positive")
        self._tools = {tool.definition["name"]: tool for tool in tools}
        if len(self._tools) != len(tools) or len(tools) > MAX_PENDING:
            raise ValueError("tool names must be unique, with at most 64 tools")
        self._worker_path = str(
            worker_path or os.environ.get("EJM_HARNESS_WORKER_PATH", "ejm-harness-worker")
        )
        self._request_timeout = request_timeout
        self._process: asyncio.subprocess.Process | None = None
        self._pending: dict[str, asyncio.Future[object]] = {}
        self._tasks: set[asyncio.Task[None]] = set()
        self._failure: Exception | None = None
        self._closing = False
        self._send_lock = asyncio.Lock()
        self._normal = _Lane(asyncio.Semaphore(NORMAL_REQUESTS), MAX_QUEUED, MAX_OUTPUT)
        self._control = _Lane(asyncio.Semaphore(CONTROL_REQUESTS), CONTROL_REQUESTS, MAX_FRAME)
        self._output_bytes = 0
        self._feed = EventFeed()
        self._callbacks = ToolCallbacks(self._tools, self._send, self._fail, self._track)

    async def close(self) -> None:
        process = self._process
        if self._closing:
            return
        self._closing = True
        close_deadline = asyncio.get_running_loop().time() + SHUTDOWN_TIMEOUT
        if process is not None:
            try:
                async with asyncio.timeout(SHUTDOWN_TIMEOUT):
                    if self._failure is None and process.returncode is None:
                        with contextlib.suppress(HarnessError, OSError):
                            await self._request("shutdown", {}, during_close=True)
                    if process.stdin:
                        process.stdin.close()
                    await process.wait()
            except TimeoutError:
                with contextlib.suppress(ProcessLookupError):
                    process.kill()
                await process.wait()
        self._fail(HarnessError("closed", "harness closed"))
        current = asyncio.current_task()
        tasks = [task for task in self._tasks if task is not current]
        for task in tasks:
            task.cancel()
        if tasks:
            done, _ = await asyncio.wait(
                tasks, timeout=max(0, close_deadline - asyncio.get_running_loop().time())
            )
            for task in done:
                if not task.cancelled():
                    task.exception()
        self._callbacks.shutdown()

    async def _start(self) -> None:
        self._process = await asyncio.create_subprocess_exec(
            self._worker_path,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            limit=MAX_FRAME,
        )
        self._track(self._read())
        self._track(self._drain_stderr())

    async def _request(self, method: str, params: object, *, during_close: bool = False) -> object:
        self._assert_open(during_close=during_close)
        request_id = str(uuid4())
        frame = encode_frame({"id": request_id, "method": method, "params": params})
        lane = self._control if method in CONTROL_METHODS else self._normal
        sent = False
        future: asyncio.Future[object] | None = None
        try:
            async with asyncio.timeout(self._request_timeout), lane.admit(len(frame)):
                self._assert_open(during_close=during_close)
                future = asyncio.get_running_loop().create_future()
                self._pending[request_id] = future
                sent = True
                await self._write(frame)
                return await future
        except TimeoutError as error:
            outcome = "its effect may be uncertain" if sent else "no effect was sent"
            failure = HarnessError("timeout", f"request {method} timed out; {outcome}")
            if sent:
                self._fail(failure)
            raise failure from error
        finally:
            self._pending.pop(request_id, None)
            if future is not None and not future.done():
                future.cancel()
            elif future is not None and not future.cancelled():
                future.exception()

    def _cursor(self, session_id: str) -> int:
        return self._feed.cursor(session_id)

    async def _changed(self, session_id: str) -> None:
        self._assert_open()
        await self._feed.wait(session_id)
        self._assert_open()

    def _assert_open(self, *, during_close: bool = False) -> None:
        if self._failure is not None:
            raise self._failure
        if self._process is None:
            raise HarnessError("closed", "enter the harness context before use")
        if self._closing and not during_close:
            raise HarnessError("closed", "harness is closing")

    async def _send(self, value: object) -> None:
        await self._write(encode_frame(value))

    async def _write(self, frame: bytes) -> None:
        process = self._process
        self._assert_open(during_close=True)
        if process is None or process.stdin is None:
            raise HarnessError("closed", "worker input is unavailable")
        if self._output_bytes + len(frame) > MAX_OUTPUT:
            raise HarnessError("busy", "worker output buffer reached 4 MiB")
        self._output_bytes += len(frame)
        try:
            async with self._send_lock:
                process.stdin.write(frame)
                await asyncio.wait_for(process.stdin.drain(), timeout=5)
        except (TimeoutError, OSError) as error:
            failure = HarnessError("disconnected", "worker input stalled or closed")
            self._fail(failure)
            raise failure from error
        finally:
            self._output_bytes -= len(frame)

    def _track(self, coroutine: Coroutine[Any, Any, None]) -> asyncio.Task[None]:
        task = asyncio.create_task(coroutine)
        self._tasks.add(task)
        task.add_done_callback(self._tasks.discard)
        return task

    async def _read(self) -> None:
        process = self._process
        if process is None or process.stdout is None:
            return
        try:
            while True:
                line = await process.stdout.readline()
                if self._failure is not None:
                    return
                self._receive(decode_frame(line))
        except Exception as error:
            self._fail(error)

    def _receive(self, message: dict[str, Any]) -> None:
        method = message.get("method")
        if method == "tool.invoke":
            self._callbacks.start(str(message["id"]), cast("ToolInvocation", message["params"]))
        elif method == "tool.cancel":
            self._callbacks.cancel(str(message["params"]["call_id"]))
        elif method == "event":
            self._feed.record(cast("Event", message["params"]))
        elif isinstance(message.get("id"), str):
            self._settle(message)
        else:
            raise HarnessError("protocol", "unknown worker message")

    def _settle(self, response: dict[str, Any]) -> None:
        future = self._pending.get(response["id"])
        if future is None or future.done():
            return
        if "error" in response:
            error = as_object(response["error"])
            future.set_exception(HarnessError(str(error["code"]), str(error["message"])))
        elif "result" in response:
            future.set_result(response["result"])
        else:
            raise HarnessError("protocol", "response has no result or error")

    async def _drain_stderr(self) -> None:
        process = self._process
        if process is not None and process.stderr is not None:
            while await process.stderr.read(8192):
                pass

    def _fail(self, error: Exception) -> None:
        if self._failure is not None:
            return
        self._failure = error
        for future in self._pending.values():
            if not future.done():
                future.set_exception(error)
        self._callbacks.abort_all()
        self._feed.wake_all()
        process = self._process
        if process is not None and process.returncode is None:
            with contextlib.suppress(ProcessLookupError):
                process.terminate()
