#!/usr/bin/env python3
"""Run and compare independent standard-library unittest outcomes."""

import json
import os
import sys
import unittest
from pathlib import Path
from typing import Any


class EvidenceResult(unittest.TextTestResult):
    """Collect one outcome per test while preserving the standard runner output."""

    def __init__(self, *args: Any, **kwargs: Any) -> None:  # noqa: ANN401 - unittest constructs results with its own arguments
        super().__init__(*args, **kwargs)
        self.outcomes: dict[str, str] = {}
        self.started: set[str] = set()
        self.ineligible: set[str] = set()
        self.fixture_failures: set[str] = set()

    def startTest(self, test: unittest.case.TestCase) -> None:
        if test.id() in self.started:
            self.ineligible.add(test.id())
        self.started.add(test.id())
        super().startTest(test)

    def record(self, test: unittest.case.TestCase, state: str) -> None:
        identity = getattr(test, "test_case", test).id()
        previous = self.outcomes.get(identity)
        if state == "skipped":
            self.ineligible.add(identity)
        if previous == "failed" or state == "failed":
            self.outcomes[identity] = "failed"
        else:
            self.outcomes[identity] = state

    def failure(self, test: unittest.case.TestCase, err: Any) -> None:  # noqa: ANN401 - unittest callback
        identity = test.id()
        if identity not in self.started:
            self.fixture_failures.add(identity)
            return
        traceback = err[2]
        while traceback:
            if traceback.tb_frame.f_code.co_name in ("_callSetUp", "_callTearDown", "_callCleanup"):
                self.ineligible.add(identity)
            traceback = traceback.tb_next
        self.record(test, "failed")

    def addSuccess(self, test: unittest.case.TestCase) -> None:
        super().addSuccess(test)
        self.record(test, "passed")

    def addFailure(self, test: unittest.case.TestCase, err: Any) -> None:  # noqa: ANN401 - unittest callback
        super().addFailure(test, err)
        self.failure(test, err)

    def addError(self, test: unittest.case.TestCase, err: Any) -> None:  # noqa: ANN401 - unittest callback
        super().addError(test, err)
        self.failure(test, err)

    def addSkip(self, test: unittest.case.TestCase, reason: str) -> None:
        super().addSkip(test, reason)
        if test.id() not in self.started and not hasattr(test, "test_case"):
            self.fixture_failures.add(test.id())
            return
        self.record(test, "skipped")

    def addExpectedFailure(self, test: unittest.case.TestCase, err: Any) -> None:  # noqa: ANN401 - unittest callback
        super().addExpectedFailure(test, err)
        self.record(test, "skipped")

    def addUnexpectedSuccess(self, test: unittest.case.TestCase) -> None:
        super().addUnexpectedSuccess(test)
        self.record(test, "failed")

    def addSubTest(
        self,
        test: unittest.case.TestCase,
        subtest: unittest.case.TestCase,
        err: Any,  # noqa: ANN401 - unittest callback
    ) -> None:
        super().addSubTest(test, subtest, err)
        if err is not None:
            self.record(test, "failed")


def run(report: Path, pattern: str = "test*.py") -> int:
    if pattern not in ("test*.py", "test_*.py"):
        raise ValueError("Unsupported unittest discovery pattern")
    tests = Path.cwd() / "tests"
    sys.path.insert(0, str(tests))
    suite = unittest.defaultTestLoader.discover(str(tests), pattern=pattern)

    def identities(group: unittest.TestSuite) -> list[str]:
        return [
            identity
            for item in group
            for identity in (
                identities(item) if isinstance(item, unittest.TestSuite) else [item.id()]
            )
        ]

    expected = set(identities(suite))
    result = unittest.TextTestRunner(verbosity=2, resultclass=EvidenceResult).run(suite)
    if not isinstance(result, EvidenceResult):
        raise TypeError("Unittest produced no evidence")
    for fixture in result.fixture_failures:
        scope = fixture.partition(" (")[2].removesuffix(")")
        result.ineligible.update(
            identity for identity in expected if identity.startswith(scope + ".")
        )
    incomplete = (
        not expected
        or result.started != expected
        or set(result.outcomes) != expected
        or bool(result.fixture_failures or result.ineligible)
    )
    data = json.dumps(
        {
            "outcomes": result.outcomes,
            "ineligible": sorted(result.ineligible),
            "incomplete": incomplete,
        },
        sort_keys=True,
    )
    if (
        len(data.encode("utf-8")) > 4 * 1024 * 1024
        or len(result.outcomes) > 20_000
        or any(len(identity) > 4096 for identity in result.outcomes)
    ):
        incomplete = True
        retained: dict[str, str] = {}
        budget = 4 * 1024 * 1024 - 128
        reference: set[str] = set()
        reference_path = os.environ.get("EJM_REGISTRY_SCHEMA_REFERENCE")
        if reference_path:
            try:
                path = Path(reference_path)
                if path.is_file() and not path.is_symlink() and path.stat().st_size <= 4 * 1024 * 1024:
                    previous = json.loads(path.read_bytes())
                    reference = {key for key, value in previous["outcomes"].items() if value == "failed"}
            except (OSError, ValueError, KeyError, TypeError, AttributeError):
                reference.clear()
        ordered = sorted(
            result.outcomes.items(),
            key=lambda item: (item[1] != "failed", item[0] not in reference, item[0]),
        )
        for identity, outcome in ordered:
            encoded = json.dumps(identity).encode("utf-8")
            cost = 2 * len(encoded) + 24
            if len(identity) > 4096 or cost > budget:
                continue
            if len(retained) == 20_000:
                break
            retained[identity] = outcome
            budget -= cost
        data = json.dumps({
            "outcomes": retained,
            "ineligible": sorted(result.ineligible.intersection(retained)),
            "incomplete": True,
        }, sort_keys=True)
    report.write_text(data, encoding="utf-8")
    return 0 if result.wasSuccessful() and not incomplete else 1


def compare(first: Path, second: Path, original: Path | None = None) -> int:
    def unique_pairs(pairs: list[tuple[str, str]]) -> dict[str, str]:
        record = dict(pairs)
        if len(record) != len(pairs):
            raise ValueError("Duplicate unittest identity")
        return record

    records = []
    ineligible: set[str] = set()
    incomplete = False
    for path in (first, second) if original is None else (first, second, original):
        if path.is_symlink() or not path.is_file() or path.stat().st_size > 4 * 1024 * 1024:
            raise ValueError("Invalid unittest evidence file")
        record = json.loads(path.read_bytes(), object_pairs_hook=unique_pairs)
        if isinstance(record, dict) and set(record) == {"outcomes", "ineligible", "incomplete"}:
            if (
                not isinstance(record["ineligible"], list)
                or not all(isinstance(key, str) for key in record["ineligible"])
                or not isinstance(record["incomplete"], bool)
            ):
                raise ValueError("Invalid unittest completeness")
            ineligible.update(record["ineligible"])
            incomplete |= record["incomplete"]
            record = record["outcomes"]
        if not isinstance(record, dict) or not record or len(record) > 20_000:
            raise ValueError("Invalid unittest evidence")
        if any(
            not isinstance(key, str)
            or not key
            or len(key) > 4096
            or value not in ("passed", "failed", "skipped")
            for key, value in record.items()
        ):
            raise ValueError("Invalid unittest outcome")
        records.append(record)
    a = records[0]
    common = set(a).intersection(*(record.keys() for record in records[1:]))
    for key in sorted(
        {key for record in records for key, value in record.items() if value == "failed"}
    ):
        print("test-failure:", key)
    mixed = sorted(
        key
        for key in common - ineligible
        if {records[0][key], records[1][key]} == {"passed", "failed"}
    )
    for key in mixed:
        print("flaky-test:", key)
    if original is not None:
        changed = sorted(
            key
            for key in common - ineligible
            if records[0][key] == records[1][key]
            and {records[0][key], records[2][key]} == {"passed", "failed"}
        )
        for key in changed:
            print(
                "recovered-original-test-failure:"
                if records[2][key] == "failed"
                else "original-test-outcome-changed:",
                key,
            )
    if (
        incomplete
        or ineligible
        or any(a.keys() != record.keys() or "skipped" in record.values() for record in records)
    ):
        raise ValueError("Unittest runs did not execute the same test set")
    print(f"Compared {len(a)} unittest outcomes; {len(mixed)} inconsistent test(s).")
    return int(bool(mixed) or any("failed" in record.values() for record in records))


def main() -> int:
    try:
        if len(sys.argv) in (3, 4) and sys.argv[1] == "run":
            return run(Path(sys.argv[2]), sys.argv[3] if len(sys.argv) == 4 else "test*.py")
        if len(sys.argv) in (4, 5) and sys.argv[1] == "compare":
            return compare(
                Path(sys.argv[2]),
                Path(sys.argv[3]),
                Path(sys.argv[4]) if len(sys.argv) == 5 else None,
            )
        raise ValueError("Expected run REPORT or compare FIRST SECOND")
    except (OSError, TypeError, ValueError, RecursionError) as error:
        print(f"Unittest evidence failed: {type(error).__name__}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
