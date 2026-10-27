"""Bounded JSONL transport with cooperative host tool cancellation."""

from __future__ import annotations

import asyncio
import contextlib
import inspect
import json
import math
import os
import threading
import time
from collections.abc import AsyncIterator, Awaitable, Callable, Iterator, Sequence
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Generic, Self, TypeVar, cast
from uuid import uuid4

from .protocol import (
    ConfigureParams,
    Event,
    GovernorSnapshot,
    HarnessSnapshot,
    PackageInfo,
    PackagePage,
    RunStarted,
    RunSummary,
    ToolDefinition,
    ToolInvocation,
    ToolOutput,
)
from .protocol import (
    Session as SessionInfo,
)

MAX_FRAME = 1024 * 1024
MAX_PENDING = 64
MAX_OUTPUT = 4 * MAX_FRAME
MAX_QUEUED = 256
NORMAL_REQUESTS = 56
CONTROL_REQUESTS = MAX_PENDING - NORMAL_REQUESTS
CLEANUP_TIMEOUT = 5
T = TypeVar("T")


class HarnessError(Exception):
    """A worker error or a bounded transport failure."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code


@dataclass
class ToolContext:
    session_id: str
    run_id: str
    call_id: str
    deadline_ms: int
    _progress: Callable[[str], Awaitable[None]] = field(repr=False)
    _loop: asyncio.AbstractEventLoop = field(repr=False)
    _cancelled: threading.Event = field(default_factory=threading.Event, repr=False)
    _async_cancelled: asyncio.Event = field(default_factory=asyncio.Event, repr=False)

    @property
    def cancelled(self) -> bool:
        return self._cancelled.is_set()

    async def wait_cancelled(self) -> None:
        await self._async_cancelled.wait()

    async def progress(self, message: str) -> None:
        if self.cancelled:
            raise HarnessError("cancelled", "tool cancelled")
        await self._progress(message)

    def report_progress(self, message: str) -> None:
        """Report from a synchronous tool thread. Async tools await progress()."""
        if self.cancelled:
            raise HarnessError("cancelled", "tool cancelled")
        asyncio.run_coroutine_threadsafe(self.progress(message), self._loop).result(timeout=5)

    def _cancel(self) -> None:
        self._cancelled.set()
        self._async_cancelled.set()


def _cancel_context(context: ToolContext) -> None:
    # The harness owns cancellation; the method stays off the tool-facing surface.
    context._cancel()  # noqa: SLF001 -- same-module owner of the tool context


@dataclass(frozen=True)
class Tool(Generic[T]):
    """A host-owned effect grant with a JSON Schema and a sync or async handler."""

    definition: ToolDefinition
    execute: Callable[[T, ToolContext], ToolOutput | str | Awaitable[ToolOutput | str]]


class AsyncHarness:
    """One worker and governor shared by every session in this host."""

    def __init__(
        self,
        *,
        config: ConfigureParams | None = None,
        tools: Sequence[Tool[Any]] = (),
        worker_path: str | Path | None = None,
        request_timeout: float = 30,
    ) -> None:
        if not math.isfinite(request_timeout) or request_timeout <= 0:
            raise ValueError("request_timeout must be positive")
        self._config = dict(config or {})
        if self._config.get("tools"):
            raise ValueError("register callback definitions and handlers together through tools")
        self._tools = {tool.definition["name"]: tool for tool in tools}
        if len(self._tools) != len(tools) or len(tools) > MAX_PENDING:
            raise ValueError("tool names must be unique, with at most 64 tools")
        self._worker_path = str(
            worker_path or os.environ.get("EJM_HARNESS_WORKER_PATH", "ejm-harness-worker")
        )
        self._request_timeout = request_timeout
        self._process: asyncio.subprocess.Process | None = None
        self._pending: dict[str, asyncio.Future[object]] = {}
        self._callbacks: dict[str, tuple[ToolContext, asyncio.Task[None]]] = {}
        self._tasks: set[asyncio.Task[None]] = set()
        self._cursors: dict[str, int] = {}
        self._changes: dict[str, asyncio.Event] = {}
        self._failure: Exception | None = None
        self._closing = False
        self._send_lock = asyncio.Lock()
        self._request_slots = asyncio.Semaphore(NORMAL_REQUESTS)
        self._control_slots = asyncio.Semaphore(CONTROL_REQUESTS)
        self._queued_requests = [0, 0]
        self._queued_bytes = [0, 0]
        self._output_bytes = 0
        self._pool = ThreadPoolExecutor(max_workers=4, thread_name_prefix="ejm-harness-tool")

    @classmethod
    async def create(
        cls,
        *,
        config: ConfigureParams | None = None,
        tools: Sequence[Tool[Any]] = (),
        worker_path: str | Path | None = None,
        request_timeout: float = 30,
    ) -> AsyncHarness:
        harness = cls(
            config=config, tools=tools, worker_path=worker_path, request_timeout=request_timeout
        )
        return await harness.__aenter__()

    async def __aenter__(self) -> Self:
        if self._process is not None:
            self._assert_open()
            return self
        try:
            await self._start()
        except BaseException:
            await self.close()
            raise
        return self

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
        hello = _object(await self._request("hello", {"protocol": 1}))
        if hello.get("protocol") != 1:
            raise HarnessError("protocol", "worker protocol 1 is required")
        provider = self._config.get("provider")
        packages_requested = self._config.get("packages") or (
            isinstance(provider, dict) and provider.get("kind") == "package"
        )
        capabilities = hello.get("capabilities")
        if packages_requested and (
            not isinstance(capabilities, list) or "packages" not in capabilities
        ):
            raise HarnessError(
                "protocol", "update ejm-harness-worker to a version that supports packages"
            )
        await self._request(
            "configure",
            {
                **self._config,
                "tools": [tool.definition for tool in self._tools.values()],
            },
        )

    async def __aexit__(self, *_exc: object) -> None:
        await self.close()

    async def create_session(self, cwd: str | Path | None = None, title: str = "") -> AsyncSession:
        info = cast(
            "SessionInfo",
            await self._request(
                "session.create",
                {
                    "cwd": str(cwd or Path.cwd()),
                    "title": title,
                },
            ),
        )
        return AsyncSession(self, info)

    async def list_sessions(self) -> list[AsyncSession]:
        sessions = cast("list[SessionInfo]", await self._request("session.list", {}))
        return [AsyncSession(self, info) for info in sessions]

    async def governor(self) -> GovernorSnapshot:
        return cast("GovernorSnapshot", await self._request("governor.status", {}))

    async def status(self) -> HarnessSnapshot:
        return cast("HarnessSnapshot", await self._request("harness.status", {}))

    async def list_packages(self) -> list[PackageInfo]:
        packages: list[PackageInfo] = []
        after = 0
        while True:
            page = cast(
                "PackagePage", await self._request("package.list", {"after": after, "limit": 16})
            )
            packages.extend(page["packages"])
            if len(packages) > 128:
                raise HarnessError("protocol", "package list exceeds 128 entries")
            following = page.get("next")
            if following is None:
                return packages
            if following <= after or following > 128:
                raise HarnessError("protocol", "invalid package page cursor")
            after = following

    async def reload_package(self, package: str) -> None:
        """Reload idle code while retaining the configured manifest and grants."""
        await self._request("package.reload", {"package": package})

    async def invoke_package_command(
        self, package: str, name: str, session_id: str, arguments: object = None
    ) -> object:
        return await self._request(
            "package.command",
            {
                "package": package,
                "name": name,
                "session_id": session_id,
                "arguments": {} if arguments is None else arguments,
            },
        )

    async def render_package_widget(
        self, package: str, name: str, session_id: str, arguments: object = None
    ) -> object:
        """Return widget data for the host to render."""
        return await self._request(
            "package.widget",
            {
                "package": package,
                "name": name,
                "session_id": session_id,
                "arguments": {} if arguments is None else arguments,
            },
        )

    async def events(self, session_id: str, after: int = 0, limit: int = 64) -> list[Event]:
        return cast(
            "list[Event]",
            await self._request(
                "events.list",
                {
                    "session_id": session_id,
                    "after": after,
                    "limit": limit,
                },
            ),
        )

    async def close(self) -> None:
        process = self._process
        if self._closing:
            return
        self._closing = True
        close_deadline = asyncio.get_running_loop().time() + 5
        if process is not None:
            try:
                async with asyncio.timeout(5):
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
        self._pool.shutdown(wait=False, cancel_futures=True)

    async def _request(self, method: str, params: object, *, during_close: bool = False) -> object:
        self._assert_open(during_close=during_close)
        request_id = str(uuid4())
        frame = self._frame({"id": request_id, "method": method, "params": params})
        control = method in {"run.cancel", "run.steer", "run.queue", "shutdown"}
        queue = int(control)
        queue_limit = CONTROL_REQUESTS if control else MAX_QUEUED
        byte_limit = MAX_FRAME if control else MAX_OUTPUT
        if (
            self._queued_requests[queue] >= queue_limit
            or self._queued_bytes[queue] + len(frame) > byte_limit
        ):
            raise HarnessError("busy", "request admission queue reached its count or byte limit")
        slots = self._control_slots if control else self._request_slots
        self._queued_requests[queue] += 1
        self._queued_bytes[queue] += len(frame)
        queued = True
        acquired = False
        sent = False
        future: asyncio.Future[object] | None = None
        try:
            async with asyncio.timeout(self._request_timeout):
                await slots.acquire()
                acquired = True
                self._queued_requests[queue] -= 1
                self._queued_bytes[queue] -= len(frame)
                queued = False
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
            if queued:
                self._queued_requests[queue] -= 1
                self._queued_bytes[queue] -= len(frame)
            if acquired:
                slots.release()
            self._pending.pop(request_id, None)
            if future is not None and not future.done():
                future.cancel()
            elif future is not None and not future.cancelled():
                future.exception()

    def _assert_open(self, *, during_close: bool = False) -> None:
        if self._failure is not None:
            raise self._failure
        if self._process is None:
            raise HarnessError("closed", "enter the harness context before use")
        if self._closing and not during_close:
            raise HarnessError("closed", "harness is closing")

    async def _send(self, value: object) -> None:
        await self._write(self._frame(value))

    @staticmethod
    def _frame(value: object) -> bytes:
        frame = (json.dumps(value, ensure_ascii=False, separators=(",", ":")) + "\n").encode()
        if len(frame) > MAX_FRAME:
            raise HarnessError("frame_limit", "protocol frame exceeds 1 MiB")
        return frame

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

    def _track(self, awaitable: Awaitable[None]) -> asyncio.Task[None]:
        async def execute() -> None:
            await awaitable

        task = asyncio.create_task(execute())
        self._tasks.add(task)
        task.add_done_callback(self._tasks.discard)
        return task

    async def _read(self) -> None:
        process = self._process
        if process is None or process.stdout is None:
            return
        try:
            await self._read_frames(process.stdout)
        except asyncio.CancelledError:
            raise
        except Exception as error:  # noqa: BLE001 -- any reader failure fails the whole harness
            self._fail(error)

    async def _read_frames(self, stdout: asyncio.StreamReader) -> None:
        while True:
            line = await stdout.readline()
            if self._failure is not None:
                return
            if not line:
                raise HarnessError("disconnected", "worker output closed")
            if len(line) > MAX_FRAME or not line.endswith(b"\n"):
                raise HarnessError("frame_limit", "worker frame exceeds 1 MiB or is truncated")
            value = _object(json.loads(line))
            method = value.get("method")
            if method == "tool.invoke":
                invocation = cast("ToolInvocation", value["params"])
                call_id = invocation["call_id"]
                if len(self._callbacks) >= MAX_PENDING or call_id in self._callbacks:
                    raise HarnessError("busy", "callback capacity or unique call_id limit reached")
                context = self._tool_context(invocation)
                task = self._track(self._invoke(str(value["id"]), invocation, context))
                self._callbacks[call_id] = context, task
                self._track(self._watch_callback(context, task))
            elif method == "tool.cancel":
                callback = self._callbacks.get(str(value["params"]["call_id"]))
                if callback:
                    _cancel_context(callback[0])
            elif method == "event":
                event = cast("Event", value["params"])
                session_id = event["session_id"]
                if session_id not in self._cursors and len(self._cursors) >= 1000:
                    raise HarnessError("protocol", "session capacity exceeded")
                self._cursors[session_id] = max(self._cursors.get(session_id, 0), event["sequence"])
                if session_id in self._changes:
                    self._changes[session_id].set()
            elif isinstance(value.get("id"), str):
                future = self._pending.get(value["id"])
                if future is not None and not future.done():
                    if "error" in value:
                        error = _object(value["error"])
                        future.set_exception(
                            HarnessError(str(error["code"]), str(error["message"]))
                        )
                    elif "result" in value:
                        future.set_result(value["result"])
                    else:
                        raise HarnessError("protocol", "response has no result or error")
            else:
                raise HarnessError("protocol", "unknown worker message")

    async def _drain_stderr(self) -> None:
        process = self._process
        if process is not None and process.stderr is not None:
            while await process.stderr.read(8192):
                pass

    def _tool_context(self, invocation: ToolInvocation) -> ToolContext:
        async def progress(message: str) -> None:
            if len(message.encode()) > 16 * 1024:
                raise HarnessError("frame_limit", "progress exceeds 16 KiB")
            await self._send(
                {
                    "method": "tool.progress",
                    "params": {
                        "call_id": invocation["call_id"],
                        "message": message,
                    },
                }
            )

        return ToolContext(
            session_id=invocation["session_id"],
            run_id=invocation["run_id"],
            call_id=invocation["call_id"],
            deadline_ms=invocation["deadline_ms"],
            _progress=progress,
            _loop=asyncio.get_running_loop(),
        )

    async def _execute(self, invocation: ToolInvocation, context: ToolContext) -> object:
        tool = self._tools.get(invocation["name"])
        if tool is None:
            raise HarnessError("tool", f"unregistered tool {invocation['name']}")
        remaining = max(0, context.deadline_ms / 1000 - time.time())
        if remaining <= 0 or context.cancelled:
            raise HarnessError("timeout", "tool deadline exceeded before execution")
        result: object
        if inspect.iscoroutinefunction(tool.execute):
            result = tool.execute(invocation["arguments"], context)
        else:

            def execute_sync() -> object:
                if context.cancelled or time.time() * 1000 >= context.deadline_ms:
                    raise HarnessError("cancelled", "tool cancelled before execution")
                return tool.execute(invocation["arguments"], context)

            result = await asyncio.get_running_loop().run_in_executor(
                self._pool,
                execute_sync,
            )
        if inspect.isawaitable(result):
            result = await result
        return result

    async def _invoke(
        self, request_id: str, invocation: ToolInvocation, context: ToolContext
    ) -> None:
        try:
            result = await self._execute(invocation, context)
            if self._failure is None:
                await self._send(
                    {
                        "id": request_id,
                        "result": {"content": result} if isinstance(result, str) else result,
                    }
                )
        except (Exception, asyncio.CancelledError) as error:  # noqa: BLE001 -- any tool failure is reported to the worker
            if isinstance(error, asyncio.CancelledError):
                _cancel_context(context)
            if self._failure is None:
                try:
                    await self._send(
                        {"id": request_id, "error": {"code": "tool", "message": str(error)[:4096]}}
                    )
                except Exception as send_error:  # noqa: BLE001 -- a failed error report fails the harness
                    self._fail(send_error)
        finally:
            self._callbacks.pop(invocation["call_id"], None)

    async def _watch_callback(self, context: ToolContext, task: asyncio.Task[None]) -> None:
        cancelled = asyncio.create_task(context.wait_cancelled())
        try:
            remaining = max(0, context.deadline_ms / 1000 - time.time())
            await asyncio.wait(
                {task, cancelled}, timeout=remaining, return_when=asyncio.FIRST_COMPLETED
            )
            if task.done():
                return
            _cancel_context(context)
            done, _ = await asyncio.wait({task}, timeout=CLEANUP_TIMEOUT)
            if not done:
                self._fail(
                    HarnessError("cleanup_timeout", "host callback did not stop within 5 seconds")
                )
        finally:
            cancelled.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await cancelled

    def _fail(self, error: Exception) -> None:
        if self._failure is not None:
            return
        self._failure = error
        for future in self._pending.values():
            if not future.done():
                future.set_exception(error)
        for context, task in list(self._callbacks.values()):
            _cancel_context(context)
            task.cancel()
        for change in self._changes.values():
            change.set()
        process = self._process
        if process is not None and process.returncode is None:
            with contextlib.suppress(ProcessLookupError):
                process.terminate()

    async def _changed(self, session_id: str) -> None:
        self._assert_open()
        if session_id not in self._changes:
            if len(self._changes) >= 1000:
                raise HarnessError("busy", "session iterator capacity exceeded")
            self._changes[session_id] = asyncio.Event()
        change = self._changes[session_id]
        with contextlib.suppress(TimeoutError):
            await asyncio.wait_for(change.wait(), timeout=0.25)
        change.clear()
        self._assert_open()


async def _harness_request(harness: AsyncHarness, method: str, params: object) -> object:
    return await harness._request(method, params)  # noqa: SLF001 -- same-module transport owner


def _harness_call(harness: Harness, awaitable: Awaitable[T]) -> T:
    return harness._call(awaitable)  # noqa: SLF001 -- same-module owner of the private event loop


@dataclass
class AsyncSession:
    _harness: AsyncHarness = field(repr=False)
    info: SessionInfo

    @property
    def id(self) -> str:
        return self.info["id"]

    async def start(self, prompt: str) -> AsyncRun:
        after = self._harness._cursors.get(self.id, 0)  # noqa: SLF001 -- same-module cursor owner
        started = cast(
            "RunStarted",
            await _harness_request(
                self._harness,
                "run.start",
                {
                    "session_id": self.id,
                    "prompt": prompt,
                },
            ),
        )
        return AsyncRun(self._harness, started["run_id"], self.id, after)

    async def events(self, after: int = 0, limit: int = 64) -> list[Event]:
        return await self._harness.events(self.id, after, limit)

    async def invoke_package_command(
        self, package: str, name: str, arguments: object = None
    ) -> object:
        return await self._harness.invoke_package_command(package, name, self.id, arguments)

    async def render_package_widget(
        self, package: str, name: str, arguments: object = None
    ) -> object:
        return await self._harness.render_package_widget(package, name, self.id, arguments)


class AsyncRun:
    def __init__(self, harness: AsyncHarness, run_id: str, session_id: str, after: int = 0) -> None:
        self._harness = harness
        self.id = run_id
        self.session_id = session_id
        self._after = after
        self._summary: RunSummary | None = None

    async def __aiter__(self) -> AsyncIterator[Event]:
        after = self._after
        while True:
            events = await self._harness.events(self.session_id, after)
            if not events:
                await self._harness._changed(self.session_id)  # noqa: SLF001 -- same-module change notifier
                continue
            for event in events:
                after = event["sequence"]
                if event.get("run_id") != self.id:
                    continue
                if event["type"] == "run_finished":
                    self._summary = event["summary"]
                yield event
                if event["type"] == "run_finished":
                    return

    async def wait(self) -> RunSummary:
        if self._summary is not None:
            return self._summary
        async for event in self:
            if event["type"] == "run_finished":
                return event["summary"]
        raise HarnessError("protocol", "run ended without a summary")

    async def cancel(self) -> None:
        if self._summary is None:
            await _harness_request(
                self._harness, "run.cancel", {"session_id": self.session_id, "run_id": self.id}
            )

    async def steer(self, text: str) -> None:
        await _harness_request(
            self._harness,
            "run.steer",
            {"session_id": self.session_id, "run_id": self.id, "text": text},
        )

    async def queue(self, text: str) -> None:
        await _harness_request(
            self._harness,
            "run.queue",
            {"session_id": self.session_id, "run_id": self.id, "text": text},
        )


class Harness:
    """Synchronous host. A private event loop keeps callback transport responsive."""

    def __init__(
        self,
        *,
        config: ConfigureParams | None = None,
        tools: Sequence[Tool[Any]] = (),
        worker_path: str | Path | None = None,
        request_timeout: float = 30,
    ) -> None:
        self._loop = asyncio.new_event_loop()
        self._thread = threading.Thread(
            target=self._loop.run_forever, name="harness-sdk", daemon=True
        )
        self._thread.start()
        self._closed = False
        try:
            self._async = self._call(
                AsyncHarness.create(
                    config=config,
                    tools=tools,
                    worker_path=worker_path,
                    request_timeout=request_timeout,
                )
            )
        except BaseException:
            self._loop.call_soon_threadsafe(self._loop.stop)
            self._thread.join()
            self._loop.close()
            raise

    def _call(self, awaitable: Awaitable[T]) -> T:
        async def run() -> T:
            return await awaitable

        if self._closed or threading.current_thread() is self._thread:
            if inspect.iscoroutine(awaitable):
                awaitable.close()
            if self._closed:
                raise HarnessError("closed", "harness closed")
            raise HarnessError(
                "reentrant",
                "use an async client or a synchronous tool handler for nested SDK calls",
            )
        return asyncio.run_coroutine_threadsafe(run(), self._loop).result()

    def __enter__(self) -> Self:
        return self

    def __exit__(self, *_exc: object) -> None:
        self.close()

    def create_session(self, cwd: str | Path | None = None, title: str = "") -> Session:
        return Session(self, self._call(self._async.create_session(cwd, title)))

    def list_sessions(self) -> list[Session]:
        return [Session(self, session) for session in self._call(self._async.list_sessions())]

    def governor(self) -> GovernorSnapshot:
        return self._call(self._async.governor())

    def status(self) -> HarnessSnapshot:
        return self._call(self._async.status())

    def list_packages(self) -> list[PackageInfo]:
        return self._call(self._async.list_packages())

    def reload_package(self, package: str) -> None:
        self._call(self._async.reload_package(package))

    def invoke_package_command(
        self, package: str, name: str, session_id: str, arguments: object = None
    ) -> object:
        return self._call(self._async.invoke_package_command(package, name, session_id, arguments))

    def render_package_widget(
        self, package: str, name: str, session_id: str, arguments: object = None
    ) -> object:
        return self._call(self._async.render_package_widget(package, name, session_id, arguments))

    def events(self, session_id: str, after: int = 0, limit: int = 64) -> list[Event]:
        return self._call(self._async.events(session_id, after, limit))

    def close(self) -> None:
        if self._closed:
            return
        try:
            self._call(self._async.close())
        finally:
            self._closed = True
            self._loop.call_soon_threadsafe(self._loop.stop)
            self._thread.join()
            self._loop.close()


@dataclass
class Session:
    _harness: Harness = field(repr=False)
    _async: AsyncSession = field(repr=False)

    @property
    def id(self) -> str:
        return self._async.id

    @property
    def info(self) -> SessionInfo:
        return self._async.info

    def start(self, prompt: str) -> Run:
        return Run(self._harness, _harness_call(self._harness, self._async.start(prompt)))

    def events(self, after: int = 0, limit: int = 64) -> list[Event]:
        return _harness_call(self._harness, self._async.events(after, limit))

    def invoke_package_command(self, package: str, name: str, arguments: object = None) -> object:
        return _harness_call(
            self._harness, self._async.invoke_package_command(package, name, arguments)
        )

    def render_package_widget(self, package: str, name: str, arguments: object = None) -> object:
        return _harness_call(
            self._harness, self._async.render_package_widget(package, name, arguments)
        )


@dataclass
class Run:
    _harness: Harness = field(repr=False)
    _async: AsyncRun = field(repr=False)

    @property
    def id(self) -> str:
        return self._async.id

    @property
    def session_id(self) -> str:
        return self._async.session_id

    def __iter__(self) -> Iterator[Event]:
        iterator = self._async.__aiter__()
        while True:
            try:
                yield _harness_call(self._harness, iterator.__anext__())
            except StopAsyncIteration:
                return

    def wait(self) -> RunSummary:
        return _harness_call(self._harness, self._async.wait())

    def cancel(self) -> None:
        _harness_call(self._harness, self._async.cancel())

    def steer(self, text: str) -> None:
        _harness_call(self._harness, self._async.steer(text))

    def queue(self, text: str) -> None:
        _harness_call(self._harness, self._async.queue(text))


def _object(value: object) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise HarnessError("protocol", "worker frame must be an object")
    return cast("dict[str, Any]", value)
