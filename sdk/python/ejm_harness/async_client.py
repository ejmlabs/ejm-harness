"""Async hosts: one worker and governor shared by every session."""

from __future__ import annotations

from collections.abc import AsyncIterator, Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, cast

from .errors import HarnessError
from .frames import as_object
from .protocol import (
    ConfigureParams,
    Event,
    GovernorSnapshot,
    HarnessSnapshot,
    PackageInfo,
    PackagePage,
    RunStarted,
    RunSummary,
)
from .protocol import (
    Session as SessionInfo,
)
from .tools import Tool
from .transport import Transport

PROTOCOL_VERSION = 1
MAX_PACKAGES = 128
PACKAGE_PAGE_SIZE = 16


class AsyncHarness(Transport):
    """One worker and governor shared by every session in this host."""

    def __init__(
        self,
        *,
        config: ConfigureParams | None = None,
        tools: Sequence[Tool[Any]] = (),
        worker_path: str | Path | None = None,
        request_timeout: float = 30,
    ) -> None:
        super().__init__(tools=tools, worker_path=worker_path, request_timeout=request_timeout)
        self._config = dict(config or {})
        if self._config.get("tools"):
            raise ValueError("register callback definitions and handlers together through tools")

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

    async def __aenter__(self) -> AsyncHarness:
        if self._process is not None:
            self._assert_open()
            return self
        try:
            await self._start()
            await self._handshake()
            await self._request(
                "configure",
                {**self._config, "tools": [tool.definition for tool in self._tools.values()]},
            )
            return self
        except BaseException:
            await self.close()
            raise

    async def __aexit__(self, *_exc: object) -> None:
        await self.close()

    async def create_session(self, cwd: str | Path | None = None, title: str = "") -> AsyncSession:
        info = cast(
            "SessionInfo",
            await self._request("session.create", {"cwd": str(cwd or Path.cwd()), "title": title}),
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
                "PackagePage",
                await self._request("package.list", {"after": after, "limit": PACKAGE_PAGE_SIZE}),
            )
            packages.extend(page["packages"])
            if len(packages) > MAX_PACKAGES:
                raise HarnessError("protocol", "package list exceeds 128 entries")
            following = page.get("next")
            if following is None:
                return packages
            if following <= after or following > MAX_PACKAGES:
                raise HarnessError("protocol", "invalid package page cursor")
            after = following

    async def reload_package(self, package: str) -> None:
        """Reload idle code while retaining the configured manifest and grants."""
        await self._request("package.reload", {"package": package})

    async def invoke_package_command(
        self, package: str, name: str, session_id: str, arguments: object = None
    ) -> object:
        return await self._package_call("package.command", package, name, session_id, arguments)

    async def render_package_widget(
        self, package: str, name: str, session_id: str, arguments: object = None
    ) -> object:
        """Return widget data for the host to render."""
        return await self._package_call("package.widget", package, name, session_id, arguments)

    async def events(self, session_id: str, after: int = 0, limit: int = 64) -> list[Event]:
        return cast(
            "list[Event]",
            await self._request(
                "events.list", {"session_id": session_id, "after": after, "limit": limit}
            ),
        )

    async def _handshake(self) -> None:
        """Check the worker speaks this protocol and supports what the configuration needs."""
        hello = as_object(await self._request("hello", {"protocol": PROTOCOL_VERSION}))
        if hello.get("protocol") != PROTOCOL_VERSION:
            raise HarnessError("protocol", "worker protocol 1 is required")
        provider = self._config.get("provider")
        packages_requested = self._config.get("packages") or (
            isinstance(provider, dict) and provider.get("kind") == "package"
        )
        capabilities = hello.get("capabilities")
        if self._config.get("require_shell_process_tree_cancellation") and (
            not isinstance(capabilities, list)
            or "shell_process_tree_cancellation" not in capabilities
        ):
            raise HarnessError(
                "protocol",
                "update ejm-harness-worker to a version that supports strict shell cancellation",
            )
        if packages_requested and (
            not isinstance(capabilities, list) or "packages" not in capabilities
        ):
            raise HarnessError(
                "protocol", "update ejm-harness-worker to a version that supports packages"
            )

    async def _package_call(
        self, method: str, package: str, name: str, session_id: str, arguments: object
    ) -> object:
        return await self._request(
            method,
            {
                "package": package,
                "name": name,
                "session_id": session_id,
                "arguments": {} if arguments is None else arguments,
            },
        )


@dataclass
class AsyncSession:
    _harness: AsyncHarness = field(repr=False)
    info: SessionInfo

    @property
    def id(self) -> str:
        return self.info["id"]

    async def start(self, prompt: str) -> AsyncRun:
        after = self._harness._cursor(self.id)
        started = cast(
            "RunStarted",
            await self._harness._request("run.start", {"session_id": self.id, "prompt": prompt}),
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
                await self._harness._changed(self.session_id)
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
            await self._harness._request("run.cancel", self._target())

    async def steer(self, text: str) -> None:
        await self._harness._request("run.steer", {**self._target(), "text": text})

    async def queue(self, text: str) -> None:
        await self._harness._request("run.queue", {**self._target(), "text": text})

    def _target(self) -> dict[str, str]:
        return {"session_id": self.session_id, "run_id": self.id}
