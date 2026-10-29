"""Retain bounded JUnit testcase identities from a producer stream."""

import hashlib
import heapq
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Any
from xml.parsers import expat


class JunitRecords:
    def __init__(self, limit: int, reference: str | None) -> None:
        self.limit = max(0, limit - 64)
        self.stack: list[str] = []
        self.suite = ""
        self.case: dict[str, str] | None = None
        self.states: list[str] = []
        self.invalid = False
        self.pending = 0
        self.seen: set[tuple[str, str]] = set()
        self.ambiguous: set[tuple[str, str]] = set()
        self.records: list[tuple[tuple[int, int], tuple[str, str], bytes]] = []
        self.size = 0
        self.first_failure = False
        self.reference: set[tuple[str, str]] = set()
        if reference:
            try:
                path = Path(reference)
                if path.is_file() and path.stat().st_size <= 50 * 1024 * 1024:
                    root = ET.parse(path).getroot()
                    for suite in root.findall("testsuite"):
                        for case in suite.findall("testcase"):
                            if case.find("failure") is not None:
                                self.reference.add((suite.get("name", ""), case.get("name", "")))
            except (OSError, ET.ParseError):
                self.reference.clear()
        self.parser: Any = expat.ParserCreate()
        self.parser.StartElementHandler = self.start
        self.parser.EndElementHandler = self.end
        self.parser.CharacterDataHandler = self.characters
        self.parser.StartDoctypeDeclHandler = self.reject
        self.parser.EntityDeclHandler = self.reject
        self.parser.ExternalEntityRefHandler = self.reject

    def reject(self, *_args: object) -> None:
        raise ValueError("unsafe XML")

    def start(self, name: str, attrs: dict[str, str]) -> None:
        self.pending = 0
        self.stack.append(name)
        if len(self.stack) > 16:
            raise ValueError("XML nesting limit exceeded")
        if self.stack == ["testsuites", "testsuite"]:
            self.suite = attrs.get("name", "")
        elif self.stack == ["testsuites", "testsuite", "testcase"]:
            self.case = attrs
            self.states = []
        elif self.case is not None and len(self.stack) == 4:
            if name in ("failure", "error", "skipped") or name.startswith(("flaky", "rerun")):
                if len(self.states) < 2:
                    self.states.append(name)
                else:
                    self.invalid = True

    def end(self, name: str) -> None:
        self.pending = 0
        if self.stack == ["testsuites", "testsuite", "testcase"] and self.case is not None:
            self.record()
            self.case = None
        self.stack.pop()

    def characters(self, _data: str) -> None:
        self.pending = 0

    def record(self) -> None:
        assert self.case is not None
        name = self.case.get("name", "")
        key = (self.suite, name)
        if not self.suite or not name or len(self.suite) > 1024 or len(name) > 1024:
            self.invalid = True
            return
        if key in self.seen:
            self.ambiguous.add(key)
            self.invalid = True
            return
        if len(self.seen) >= 20_000:
            self.invalid = True
            return
        self.seen.add(key)
        if self.case.get("classname") != self.suite or len(self.states) > 1 or any(
            state.startswith(("flaky", "rerun")) for state in self.states
        ):
            self.ambiguous.add(key)
            self.invalid = True
            return
        state = self.states[0] if self.states else "passed"
        suite = ET.Element("testsuite", {"name": self.suite})
        case = ET.SubElement(suite, "testcase", {"classname": self.suite, "name": name})
        if state != "passed":
            ET.SubElement(case, state)
        raw = ET.tostring(suite, encoding="utf-8")
        first = state == "failure" and not self.first_failure
        self.first_failure |= state == "failure"
        priority = 3 if first else 2 if state == "failure" else 1 if key in self.reference else 0
        digest = int.from_bytes(hashlib.sha256(repr(key).encode()).digest(), "big")
        rank = (priority, -digest)
        if len(raw) > self.limit:
            self.invalid = True
            return
        while self.size + len(raw) > self.limit:
            if not self.records or self.records[0][0] >= rank:
                self.invalid = True
                return
            _, _, removed = heapq.heappop(self.records)
            self.size -= len(removed)
            self.invalid = True
        heapq.heappush(self.records, (rank, key, raw))
        self.size += len(raw)

    def add(self, chunk: bytes) -> None:
        if self.parser is not None:
            try:
                self.pending += len(chunk)
                self.parser.Parse(chunk, False)
                if self.pending > 1024 * 1024:
                    raise ValueError("XML token limit exceeded")
            except (expat.ExpatError, ValueError):
                self.invalid = True
                self.parser = None

    def finish(self) -> bytes:
        if self.parser is not None:
            try:
                self.parser.Parse(b"", True)
            except expat.ExpatError:
                self.invalid = True
        return b'<testsuites incomplete="true">' + b"".join(
            raw for _, key, raw in sorted(self.records, key=lambda item: (-item[0][0], item[1]))
            if key not in self.ambiguous
        ) + b"</testsuites>"
