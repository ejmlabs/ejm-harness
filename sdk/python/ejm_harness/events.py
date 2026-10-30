"""The latest event sequence seen per session, and wakeups for iterators waiting on it."""

import asyncio
import contextlib

from .errors import HarnessError
from .protocol import Event

MAX_SESSIONS = 1000
WAKE_INTERVAL = 0.25


class EventFeed:
    def __init__(self) -> None:
        self._cursors: dict[str, int] = {}
        self._changes: dict[str, asyncio.Event] = {}

    def cursor(self, session_id: str) -> int:
        return self._cursors.get(session_id, 0)

    def record(self, event: Event) -> None:
        session_id = event["session_id"]
        if session_id not in self._cursors and len(self._cursors) >= MAX_SESSIONS:
            raise HarnessError("protocol", "session capacity exceeded")
        self._cursors[session_id] = max(self.cursor(session_id), event["sequence"])
        if session_id in self._changes:
            self._changes[session_id].set()

    async def wait(self, session_id: str) -> None:
        """Return on the next event for the session, or after one wake interval."""
        if session_id not in self._changes:
            if len(self._changes) >= MAX_SESSIONS:
                raise HarnessError("busy", "session iterator capacity exceeded")
            self._changes[session_id] = asyncio.Event()
        change = self._changes[session_id]
        with contextlib.suppress(TimeoutError):
            await asyncio.wait_for(change.wait(), timeout=WAKE_INTERVAL)
        change.clear()

    def wake_all(self) -> None:
        for change in self._changes.values():
            change.set()
