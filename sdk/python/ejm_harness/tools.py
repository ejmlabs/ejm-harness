"""Host tools: the effect grant and the context each call receives."""

from __future__ import annotations

import asyncio
import threading
from collections.abc import Awaitable, Callable
from dataclasses import dataclass, field
from typing import Generic, TypeVar

from .errors import HarnessError
from .protocol import ToolDefinition, ToolOutput

T = TypeVar("T")


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


@dataclass(frozen=True)
class Tool(Generic[T]):
    """A host-owned effect grant with a JSON Schema and a sync or async handler."""

    definition: ToolDefinition
    execute: Callable[[T, ToolContext], ToolOutput | str | Awaitable[ToolOutput | str]]
