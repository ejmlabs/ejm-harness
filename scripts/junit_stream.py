"""Retain bounded JUnit testcase identities from a producer stream."""

import hashlib
import heapq
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Any
from xml.parsers import expat


class IdentityFilter:
    def __init__(self) -> None:
        self.bits = bytearray(256 * 1024)

    def add(self, identity: str) -> bool:
        digest = hashlib.sha256(identity.encode("utf-8")).digest()
        positions = [int.from_bytes(digest[i : i + 4], "big") % (len(self.bits) * 8) for i in (0, 4, 8, 12)]
        present = all(self.bits[position // 8] & (1 << (position % 8)) for position in positions)
        for position in positions:
            self.bits[position // 8] |= 1 << (position % 8)
        return present


class JunitRecords:
    def __init__(self, limit: int, reference: str | None) -> None:
        self.limit = max(0, limit - 64)
        self.stack: list[str] = []
        self.suite = ""
        self.case: dict[str, str] | None = None
        self.states: list[str] = []
        self.invalid = False
        self.pending = 0
        self.seen = IdentityFilter()
        self.exact: set[tuple[str, str]] = set()
        self.retained: set[tuple[str, str]] = set()
        self.ambiguous: set[tuple[str, str]] = set()
        self.records: list[tuple[tuple[int, int], tuple[str, str], bytes]] = []
        self.size = 0
        self.first_failure = False
        self.diagnostic: str | None = None
        self.diagnostic_tail = ""
        self.diagnostic_case = False
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
            self.diagnostic_case = False
        elif self.stack == ["testsuites", "first-diagnostic"]:
            self.diagnostic = ""
            self.diagnostic_tail = ""
        elif self.case is not None and len(self.stack) == 4:
            if name in ("failure", "error") and self.diagnostic is None:
                self.diagnostic = f"{self.suite[:1024]} :: {self.case.get('name', '')[:1024]} :: {name}"
                self.diagnostic_case = True
                if attrs.get("message"):
                    self.add_diagnostic("\n" + attrs["message"])
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

    def add_diagnostic(self, data: str) -> None:
        assert self.diagnostic is not None
        self.diagnostic = (self.diagnostic + data)[:4096]
        self.diagnostic_tail = (self.diagnostic_tail + data)[-4096:]

    def characters(self, data: str) -> None:
        self.pending = 0
        if (self.stack == ["testsuites", "first-diagnostic"] or
                (self.diagnostic_case and len(self.stack) == 4 and
                 self.stack[-1] in ("failure", "error", "system-out", "system-err"))):
            self.add_diagnostic(data)

    def first_diagnostic(self) -> str | None:
        if self.diagnostic is None:
            return None
        if self.diagnostic_tail and not self.diagnostic.endswith(self.diagnostic_tail):
            return self.diagnostic + "\n...\n" + self.diagnostic_tail
        return self.diagnostic

    def record(self) -> None:
        assert self.case is not None
        name = self.case.get("name", "")
        key = (self.suite, name)
        if not self.suite or not name or len(self.suite) > 1024 or len(name) > 1024:
            self.invalid = True
            return
        maybe_duplicate = self.seen.add(repr(key))
        if len(self.exact) < 20_000:
            duplicate = key in self.exact
            self.exact.add(key)
        else:
            duplicate = maybe_duplicate
        if duplicate:
            if key in self.retained:
                self.ambiguous.add(key)
            self.invalid = True
            return
        if self.case.get("classname") != self.suite or len(self.states) > 1 or any(
            state.startswith(("flaky", "rerun")) for state in self.states
        ):
            self.invalid = True
            return
        state = self.states[0] if self.states else "passed"
        suite = ET.Element("testsuite", {"name": self.suite})
        case = ET.SubElement(suite, "testcase", {"classname": self.suite, "name": name})
        if state != "passed":
            ET.SubElement(case, state)
        raw = ET.tostring(suite, encoding="utf-8")
        first = state in ("failure", "error") and not self.first_failure
        self.first_failure |= state in ("failure", "error")
        priority = 3 if first else 2 if state in ("failure", "error") else 1 if key in self.reference else 0
        digest = int.from_bytes(hashlib.sha256(repr(key).encode()).digest(), "big")
        rank = (priority, -digest)
        if len(raw) > self.limit:
            self.invalid = True
            return
        evicted = []
        while self.size + len(raw) > self.limit or len(self.records) >= 20_000:
            if not self.records or self.records[0][0] >= rank:
                for item in evicted:
                    heapq.heappush(self.records, item)
                    self.size += len(item[2])
                self.invalid = True
                return
            item = heapq.heappop(self.records)
            evicted.append(item)
            self.size -= len(item[2])
            self.invalid = True
        for _, victim, _ in evicted:
            self.retained.remove(victim)
            self.ambiguous.discard(victim)
        heapq.heappush(self.records, (rank, key, raw))
        self.retained.add(key)
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
        return self.snapshot()

    def snapshot(self) -> bytes:
        diagnostic = self.first_diagnostic()
        evidence = b""
        if diagnostic is not None:
            element = ET.Element("first-diagnostic")
            element.text = diagnostic
            evidence = ET.tostring(element, encoding="utf-8")
            budget = min(self.limit, 16_384)
            while len(evidence) > budget and element.text:
                if len(element.text) > 12:
                    half = len(element.text) // 4
                    element.text = element.text[:half] + "\n...\n" + element.text[-half:]
                else:
                    element.text = element.text[: len(element.text) // 2]
                evidence = ET.tostring(element, encoding="utf-8")
            if len(evidence) > budget:
                evidence = b""
        remaining = self.limit - len(evidence)
        retained = []
        for _, key, raw in sorted(self.records, key=lambda item: (-item[0][0], item[1])):
            if key not in self.ambiguous and len(raw) <= remaining:
                retained.append(raw)
                remaining -= len(raw)
        return b'<testsuites incomplete="true">' + evidence + b"".join(retained) + b"</testsuites>"
