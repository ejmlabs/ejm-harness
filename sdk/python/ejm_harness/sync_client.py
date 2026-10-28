"""Synchronous hosts. A private event loop keeps callback transport responsive."""

from __future__ import annotations

import asyncio
import inspect
import threading
from collections.abc import Awaitable, Iterator, Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, TypeVar

from .async_client import AsyncHarness, AsyncRun, AsyncSession
from .errors import HarnessError
from .protocol import (
    ConfigureParams,
    Event,
    GovernorSnapshot,
    HarnessSnapshot,
    PackageInfo,
    RunSummary,
)
from .protocol import (
    Session as SessionInfo,
)
from .tools import Tool

T = TypeVar("T")


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
            self._stop_loop()
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

    def _stop_loop(self) -> None:
        self._loop.call_soon_threadsafe(self._loop.stop)
        self._thread.join()
        self._loop.close()

    def __enter__(self) -> Harness:
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
            self._stop_loop()


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
        return Run(self._harness, self._harness._call(self._async.start(prompt)))

    def events(self, after: int = 0, limit: int = 64) -> list[Event]:
        return self._harness._call(self._async.events(after, limit))

    def invoke_package_command(self, package: str, name: str, arguments: object = None) -> object:
        return self._harness._call(self._async.invoke_package_command(package, name, arguments))

    def render_package_widget(self, package: str, name: str, arguments: object = None) -> object:
        return self._harness._call(self._async.render_package_widget(package, name, arguments))


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
                yield self._harness._call(iterator.__anext__())
            except StopAsyncIteration:
                return

    def wait(self) -> RunSummary:
        return self._harness._call(self._async.wait())

    def cancel(self) -> None:
        self._harness._call(self._async.cancel())

    def steer(self, text: str) -> None:
        self._harness._call(self._async.steer(text))

    def queue(self, text: str) -> None:
        self._harness._call(self._async.queue(text))
