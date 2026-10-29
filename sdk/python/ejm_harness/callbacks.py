"""Host tools the worker calls back: one supervised task per call id."""

from __future__ import annotations

import asyncio
import contextlib
import inspect
import time
from collections.abc import Callable, Coroutine, Mapping
from concurrent.futures import ThreadPoolExecutor
from typing import Any

from .errors import HarnessError
from .limits import CLEANUP_TIMEOUT, MAX_PENDING, MAX_PROGRESS_BYTES
from .protocol import ToolInvocation
from .tools import Tool, ToolContext

Send = Callable[[object], Coroutine[Any, Any, None]]
Track = Callable[[Coroutine[Any, Any, None]], asyncio.Task[None]]


class ToolCallbacks:
    """Runs sync handlers on a four thread pool and async handlers on the event loop.

    `fail` poisons the transport when a protocol or cleanup bound is broken, and `track` keeps
    each task where the transport can cancel it on close.
    """

    def __init__(
        self,
        tools: Mapping[str, Tool[Any]],
        send: Send,
        fail: Callable[[Exception], None],
        track: Track,
    ) -> None:
        self._tools = tools
        self._send = send
        self._fail = fail
        self._track = track
        self._active: dict[str, tuple[ToolContext, asyncio.Task[None]]] = {}
        self._pool = ThreadPoolExecutor(max_workers=4, thread_name_prefix="ejm-harness-tool")

    def start(self, request_id: str, invocation: ToolInvocation) -> None:
        call_id = invocation["call_id"]
        if len(self._active) >= MAX_PENDING or call_id in self._active:
            raise HarnessError("busy", "callback capacity or unique call_id limit reached")
        context = self._context(invocation)
        task = self._track(self._invoke(request_id, invocation, context))
        self._active[call_id] = context, task
        self._track(self._enforce_cancellation(context, task))

    def cancel(self, call_id: str) -> None:
        callback = self._active.get(call_id)
        if callback:
            callback[0]._cancel()

    def abort_all(self) -> None:
        for context, task in list(self._active.values()):
            context._cancel()
            task.cancel()

    def shutdown(self) -> None:
        self._pool.shutdown(wait=False, cancel_futures=True)

    def _context(self, invocation: ToolInvocation) -> ToolContext:
        async def progress(message: str) -> None:
            if len(message.encode()) > MAX_PROGRESS_BYTES:
                raise HarnessError("frame_limit", "progress exceeds 16 KiB")
            await self._send(
                {
                    "method": "tool.progress",
                    "params": {"call_id": invocation["call_id"], "message": message},
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

    async def _invoke(
        self, request_id: str, invocation: ToolInvocation, context: ToolContext
    ) -> None:
        # After a failure _send raises and nothing is written.
        try:
            result = await self._execute(invocation, context)
            await self._send(
                {
                    "id": request_id,
                    "result": {"content": result} if isinstance(result, str) else result,
                }
            )
        except (Exception, asyncio.CancelledError) as error:
            if isinstance(error, asyncio.CancelledError):
                context._cancel()
            try:
                await self._send(
                    {"id": request_id, "error": {"code": "tool", "message": str(error)[:4096]}}
                )
            except Exception as send_error:
                self._fail(send_error)
        finally:
            self._active.pop(invocation["call_id"], None)

    async def _execute(self, invocation: ToolInvocation, context: ToolContext) -> object:
        tool = self._tools.get(invocation["name"])
        if tool is None:
            raise HarnessError("tool", f"unregistered tool {invocation['name']}")
        if context.deadline_ms / 1000 <= time.time() or context.cancelled:
            raise HarnessError("timeout", "tool deadline exceeded before execution")
        result: object
        if inspect.iscoroutinefunction(tool.execute):
            result = tool.execute(invocation["arguments"], context)
        else:

            def execute_sync() -> object:
                if context.cancelled or time.time() * 1000 >= context.deadline_ms:
                    raise HarnessError("cancelled", "tool cancelled before execution")
                return tool.execute(invocation["arguments"], context)

            result = await asyncio.get_running_loop().run_in_executor(self._pool, execute_sync)
        if inspect.isawaitable(result):
            result = await result
        return result

    async def _enforce_cancellation(self, context: ToolContext, task: asyncio.Task[None]) -> None:
        """Cancel the handler at its deadline, and poison the transport if it will not stop."""
        cancelled = asyncio.create_task(context.wait_cancelled())
        try:
            remaining = max(0, context.deadline_ms / 1000 - time.time())
            await asyncio.wait(
                {task, cancelled}, timeout=remaining, return_when=asyncio.FIRST_COMPLETED
            )
            if task.done():
                return
            context._cancel()
            done, _ = await asyncio.wait({task}, timeout=CLEANUP_TIMEOUT)
            if not done:
                self._fail(
                    HarnessError("cleanup_timeout", "host callback did not stop within 5 seconds")
                )
        finally:
            cancelled.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await cancelled
