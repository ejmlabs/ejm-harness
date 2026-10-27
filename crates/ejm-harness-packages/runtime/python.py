"""Trusted, shared Python extension host. This process is not a sandbox."""

from __future__ import annotations

import asyncio
import contextlib
import importlib.util
import inspect
import json
import os
import signal
import sys
import threading
import uuid
from collections.abc import Callable, Iterator, Mapping
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from pathlib import Path
from queue import Queue
from typing import Any

MAX_FRAME = 1024 * 1024
MAX_ACTIVE = 64
MAX_PACKAGES = 128
MAX_LOADS = 512
MAX_PENDING_BYTES = 2 * MAX_FRAME
PROTOCOL_OUTPUT = sys.stdout.buffer
sys.stdout = sys.stderr


def object_value(value: object, name: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise TypeError(f"{name} must be an object")
    return value


def text(value: object, name: str, limit: int = 256) -> str:
    if not isinstance(value, str) or not value or len(value.encode("utf-8")) > limit:
        raise ValueError(f"{name} must be a nonempty string of at most {limit} bytes")
    return value


def error_message(error: BaseException) -> str:
    return str(error).encode("utf-8", errors="replace")[:2048].decode("utf-8", errors="replace")


def reject_constant(_value: str) -> None:
    raise ValueError("JSON numbers must be finite")


class Output:
    """One blocking writer with bounded admission and no executor backlog."""

    def __init__(self) -> None:
        self.pending = 0
        self.bytes = 0
        self.condition = asyncio.Condition()
        self.lock = asyncio.Lock()
        self.failure: BaseException | None = None
        self.loop = asyncio.get_running_loop()
        self.queue: Queue[tuple[bytes, asyncio.Future[None]]] = Queue(maxsize=1)
        threading.Thread(
            target=self._write, name="ejm-harness-extension-output", daemon=True
        ).start()

    def _finish(self, done: asyncio.Future[None], error: BaseException | None) -> None:
        if not done.done():
            if error is None:
                done.set_result(None)
            else:
                done.set_exception(error)

    def _write(self) -> None:
        while True:
            data, done = self.queue.get()
            error = None
            try:
                PROTOCOL_OUTPUT.write(data)
                PROTOCOL_OUTPUT.flush()
            except (OSError, ValueError) as caught:
                error = caught
            with contextlib.suppress(RuntimeError):
                self.loop.call_soon_threadsafe(self._finish, done, error)

    async def send(self, frame: dict[str, Any], *, terminal: bool = False) -> None:
        data = json.dumps(frame, ensure_ascii=True, allow_nan=False, separators=(",", ":")).encode()
        if len(data) > MAX_FRAME:
            raise ValueError("output frame exceeds 1 MiB")
        data += b"\n"
        async with self.condition:
            while self.pending >= MAX_ACTIVE or self.bytes + len(data) > MAX_PENDING_BYTES:
                if self.failure is not None:
                    raise RuntimeError("protocol output failed") from self.failure
                if not terminal:
                    raise RuntimeError("extension output queue is full")
                await self.condition.wait()
            if self.failure is not None:
                raise RuntimeError("protocol output failed") from self.failure
            self.pending += 1
            self.bytes += len(data)
        try:
            async with self.lock:
                done: asyncio.Future[None] = self.loop.create_future()
                self.queue.put_nowait((data, done))
                await done
        except (OSError, ValueError) as error:
            self.failure = error
            raise
        finally:
            async with self.condition:
                self.pending -= 1
                self.bytes -= len(data)
                self.condition.notify_all()


@dataclass
class Request:
    id: str
    cancelled: threading.Event = field(default_factory=threading.Event)
    running: bool = True
    task: asyncio.Task[None] | None = None


@dataclass
class Package:
    namespace: str
    handlers: dict[str, Callable[..., Any]] | None = None
    active: int = 0


class Context(Mapping[str, Any]):
    """Context fields plus cooperative cancellation and bounded output helpers."""

    def __init__(self, fields: dict[str, Any], request: Request, output: Output) -> None:
        self._fields = fields
        self._request = request
        self._output = output
        self._loop = asyncio.get_running_loop()
        self._loop_thread = threading.get_ident()
        self.cancelled = request.cancelled

    def __getitem__(self, key: str) -> object:
        return self._fields[key]

    def __iter__(self) -> Iterator[str]:
        return iter(self._fields)

    def __len__(self) -> int:
        return len(self._fields)

    def is_cancelled(self) -> bool:
        return self.cancelled.is_set()

    def raise_if_cancelled(self) -> None:
        if self.is_cancelled():
            raise RuntimeError("request cancelled")

    async def _emit(self, method: str, value: str, limit: int) -> None:
        self.raise_if_cancelled()
        if not self._request.running:
            raise RuntimeError("invocation has finished")
        if not isinstance(value, str) or len(value.encode("utf-8")) > limit:
            raise ValueError(f"{method} must be a string of at most {limit} bytes")
        key = "message" if method == "progress" else "text"
        await self._output.send({"method": method, "params": {"id": self._request.id, key: value}})

    async def progress(self, message: str) -> None:
        await self._emit("progress", message, 2048)

    async def delta(self, value: str) -> None:
        await self._emit("delta", value, 64 * 1024)

    def _emit_sync(self, method: str, value: str, limit: int) -> None:
        if threading.get_ident() == self._loop_thread:
            raise RuntimeError("async handlers must await progress() or delta()")
        asyncio.run_coroutine_threadsafe(self._emit(method, value, limit), self._loop).result()

    def progress_sync(self, message: str) -> None:
        self._emit_sync("progress", message, 2048)

    def delta_sync(self, value: str) -> None:
        self._emit_sync("delta", value, 64 * 1024)


def remove_namespace(namespace: str) -> None:
    for name in tuple(sys.modules):
        if name == namespace or name.startswith(f"{namespace}."):
            sys.modules.pop(name, None)


class Host:
    def __init__(self) -> None:
        self.output = Output()
        self.packages: dict[str, Package] = {}
        self.active: dict[str, Request] = {}
        self.pool = ThreadPoolExecutor(max_workers=4, thread_name_prefix="ejm-harness-extension")
        self.closing = False
        self.close_task: asyncio.Task[None] | None = None
        self.exit_code = 0
        self.loads = 0
        self.on_failure: Callable[[BaseException], None] = lambda _error: None

    async def load(self, params: dict[str, Any], request: Request) -> dict[str, Any]:
        package_id = text(params.get("package_id"), "package_id")
        if package_id in self.packages:
            raise ValueError("package is already loaded or loading")
        if len(self.packages) >= MAX_PACKAGES:
            raise ValueError("package limit reached")
        if self.loads >= MAX_LOADS:
            raise ValueError("host load limit reached; restart the idle language host")
        names = params.get("handlers")
        if not isinstance(names, list) or len(names) > 256:
            raise ValueError("handlers must be an array of at most 256 names")
        handlers = [text(name, "handler") for name in names]
        if len(set(handlers)) != len(handlers):
            raise ValueError("duplicate handler name")
        package = Package(f"_ejm_harness_extension_{uuid.uuid4().hex}")
        self.packages[package_id] = package
        self.loads += 1

        def import_entry() -> dict[str, Callable[..., Any]]:
            root = Path(text(params.get("root"), "root", MAX_FRAME)).resolve(strict=True)
            if not root.is_dir():
                raise ValueError("root must be a directory")
            entry = (root / text(params.get("entry"), "entry", MAX_FRAME)).resolve(strict=True)
            if not entry.is_relative_to(root) or not entry.is_file():
                raise ValueError("entry must be a file within the package root")
            if request.cancelled.is_set():
                raise RuntimeError("request cancelled")
            spec = importlib.util.spec_from_file_location(
                package.namespace, entry, submodule_search_locations=[str(root)]
            )
            if spec is None or spec.loader is None:
                raise ValueError("entry must be a Python source module")
            module = importlib.util.module_from_spec(spec)
            sys.modules[package.namespace] = module
            spec.loader.exec_module(module)
            exported = {}
            for name in handlers:
                handler = getattr(module, name, None)
                if not callable(handler):
                    raise TypeError(f"handler is not callable: {name}")
                exported[name] = handler
            return exported

        async def import_checked() -> dict[str, Callable[..., Any]]:
            exported = await asyncio.get_running_loop().run_in_executor(self.pool, import_entry)
            if request.cancelled.is_set():
                raise RuntimeError("request cancelled")
            return exported

        try:
            package.handlers = await import_checked()
        except BaseException:
            self.packages.pop(package_id, None)
            remove_namespace(package.namespace)
            raise
        return {"package_id": package_id, "handlers": handlers}

    def unload(self, params: dict[str, Any]) -> None:
        package_id = text(params.get("package_id"), "package_id")
        package = self.packages.get(package_id)
        if package is None:
            raise ValueError("package is not loaded")
        if package.handlers is None or package.active:
            raise ValueError("package is loading or has active requests")
        self.packages.pop(package_id)
        remove_namespace(package.namespace)

    async def invoke(self, params: dict[str, Any], request: Request) -> object:
        package = self.packages.get(text(params.get("package_id"), "package_id"))
        name = text(params.get("handler"), "handler")
        handler = package.handlers.get(name) if package and package.handlers is not None else None
        if handler is None or package is None:
            raise ValueError("package handler is not loaded")
        context = Context(object_value(params.get("context", {}), "context"), request, self.output)
        package.active += 1

        def call_sync() -> object:
            context.raise_if_cancelled()
            return handler(params.get("args"), context)

        try:
            context.raise_if_cancelled()
            if inspect.iscoroutinefunction(handler):
                result = handler(params.get("args"), context)
            else:
                result = await asyncio.get_running_loop().run_in_executor(self.pool, call_sync)
            if inspect.isawaitable(result):
                result = await result
            context.raise_if_cancelled()
            return result
        finally:
            request.running = False
            package.active -= 1

    async def close(self, exclude: str | None = None) -> None:
        if self.close_task is None:
            self.closing = True
            requests = [request for key, request in self.active.items() if key != exclude]
            for request in requests:
                request.cancelled.set()

            async def wait() -> None:
                tasks = [request.task for request in requests if request.task is not None]
                if tasks:
                    _, pending = await asyncio.wait(tasks, timeout=4)
                    if pending:
                        self.exit_code = 1
                        raise RuntimeError("active requests did not finish during shutdown")

            self.close_task = asyncio.create_task(wait())
        await self.close_task

    async def respond(self, frame: dict[str, Any], request: Request) -> dict[str, Any]:
        params = object_value(frame.get("params", {}), "params")
        if request.cancelled.is_set():
            raise RuntimeError("request cancelled")
        method = frame.get("method")
        result: object
        if method == "load":
            result = await self.load(params, request)
        elif method == "unload":
            self.unload(params)
            result = None
        elif method == "invoke":
            result = await self.invoke(params, request)
        else:
            raise ValueError("unknown method")
        response = {"id": request.id, "result": result}
        if len(json.dumps(response, ensure_ascii=True, allow_nan=False).encode()) > MAX_FRAME:
            raise ValueError("output frame exceeds 1 MiB")
        return response

    def start(self, frame: dict[str, Any]) -> asyncio.Task[None]:
        request = Request(frame["id"])
        self.active[request.id] = request

        async def run() -> None:
            try:
                try:
                    response = await self.respond(frame, request)
                except (Exception, asyncio.CancelledError) as error:  # noqa: BLE001 -- every failure is a protocol error response
                    response = {"id": request.id, "error": {"message": error_message(error)}}
                await self.output.send(response, terminal=True)
            finally:
                request.running = False
                self.active.pop(request.id, None)

        request.task = asyncio.create_task(run())

        def finished(task: asyncio.Task[None]) -> None:
            if not task.cancelled() and (error := task.exception()) is not None:
                self.on_failure(error)

        request.task.add_done_callback(finished)
        return request.task


async def main() -> None:
    host = Host()
    loop = asyncio.get_running_loop()
    incoming: asyncio.Queue[bytes | BaseException | None] = asyncio.Queue(maxsize=1)
    reader_stopped = threading.Event()

    def read() -> None:
        while not reader_stopped.is_set():
            value: bytes | BaseException | None
            try:
                line = sys.stdin.buffer.readline(MAX_FRAME + 2)
            except OSError as error:
                value = error
            else:
                if line and (len(line) > MAX_FRAME + 1 or not line.endswith(b"\n")):
                    value = ValueError("input frame exceeds 1 MiB or is incomplete")
                else:
                    value = line or None
            try:
                asyncio.run_coroutine_threadsafe(incoming.put(value), loop).result()
            except RuntimeError:
                return
            if value is None or isinstance(value, BaseException):
                return

    def stop(exit_code: int) -> None:
        host.exit_code = exit_code
        reader_stopped.set()
        while not incoming.empty():
            incoming.get_nowait()
        incoming.put_nowait(None)

    def fail(error: BaseException) -> None:
        sys.stderr.write(f"{error_message(error)}\n")
        stop(1)

    host.on_failure = fail

    for number in (signal.SIGINT, signal.SIGTERM):
        signal.signal(number, lambda number, _frame: loop.call_soon_threadsafe(stop, 128 + number))
    threading.Thread(target=read, name="ejm-harness-extension-input", daemon=True).start()

    async def serve() -> None:
        while True:
            value = await incoming.get()
            if value is None:
                break
            if isinstance(value, BaseException):
                raise value
            frame = object_value(json.loads(value, parse_constant=reject_constant), "request")
            if frame.get("method") == "cancel" and "id" not in frame:
                request_id = text(object_value(frame.get("params"), "params").get("id"), "id")
                if request_id in host.active:
                    host.active[request_id].cancelled.set()
                continue
            request_id = text(frame.get("id"), "id")
            if request_id in host.active:
                raise ValueError("duplicate active request id")
            if frame.get("method") == "shutdown":
                response: dict[str, Any]
                try:
                    await host.close()
                    response = {"id": request_id, "result": None}
                except Exception as error:  # noqa: BLE001 -- a failed shutdown is a protocol error response
                    response = {"id": request_id, "error": {"message": error_message(error)}}
                await host.output.send(response, terminal=True)
                break
            if host.closing or len(host.active) >= MAX_ACTIVE:
                await host.output.send(
                    {
                        "id": request_id,
                        "error": {"message": "request limit reached or host is closing"},
                    },
                    terminal=True,
                )
                continue
            host.start(frame)

    try:
        await serve()
    except Exception as error:  # noqa: BLE001 -- any loop failure ends the host with a nonzero exit
        host.exit_code = host.exit_code or 1
        sys.stderr.write(f"{error_message(error)}\n")
    finally:
        reader_stopped.set()
        try:
            await host.close()
        except Exception as error:  # noqa: BLE001 -- cleanup failure is reported, then the host exits
            sys.stderr.write(f"{error_message(error)}\n")
        host.pool.shutdown(wait=False, cancel_futures=True)
    # Exit after the bounded cleanup, without cancelling a still-running thread's owner task.
    os._exit(host.exit_code)


if __name__ == "__main__":
    asyncio.run(main())
