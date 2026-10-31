#!/usr/bin/env python3
"""Run and compare independent standard-library unittest outcomes."""

import json
import os
import sys
import unittest
from functools import partial
from pathlib import Path
from typing import Any


class EvidenceResult(unittest.TextTestResult):
    """Collect one outcome per test while preserving the standard runner output."""

    def __init__(self, *args: Any, report: Path, expected: set[str], **kwargs: Any) -> None:  # noqa: ANN401 - unittest constructs results with its own arguments
        super().__init__(*args, **kwargs)
        self.report = report
        self.expected = expected
        self.outcomes: dict[str, str] = {}
        self.started: set[str] = set()
        self.completed: set[str] = set()
        self.ineligible: set[str] = set()
        self.fixture_failures: set[str] = set()
        self.first_diagnostic: dict[str, str | bool] | None = None
        write_evidence(self, True)

    def startTest(self, test: unittest.case.TestCase) -> None:
        if test.id() in self.started:
            self.ineligible.add(test.id())
            write_evidence(self, True)
        self.started.add(test.id())
        super().startTest(test)

    def stopTest(self, test: unittest.case.TestCase) -> None:
        super().stopTest(test)
        self.completed.add(test.id())
        write_evidence(self, True)

    def fixture_failure(self, identity: str) -> None:
        self.fixture_failures.add(identity)
        scope = identity.partition(" (")[2].removesuffix(")")
        self.ineligible.update(item for item in self.expected if item.startswith(scope + "."))
        write_evidence(self, True)

    def record(self, test: unittest.case.TestCase, state: str) -> None:
        identity = getattr(test, "test_case", test).id()
        previous = self.outcomes.get(identity)
        if state == "skipped":
            self.ineligible.add(identity)
        if previous == "failed" or state == "failed":
            self.outcomes[identity] = "failed"
        else:
            self.outcomes[identity] = state

    def diagnostic(self, test: unittest.case.TestCase, message: str) -> bool:
        if self.first_diagnostic is not None:
            return False
        identity = test.id()
        truncated = len(identity) > 1024 or len(message) > 8192
        self.first_diagnostic = {
            "test": identity[:1024],
            "message": message
            if len(message) <= 8192
            else message[:4096] + "\n...\n" + message[-4096:],
            "truncated": truncated,
        }
        return True

    def failure(self, test: unittest.case.TestCase, err: Any) -> None:  # noqa: ANN401 - unittest callback
        identity = test.id()
        if identity not in self.started:
            self.fixture_failure(identity)
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
        captured = self.diagnostic(test, self.failures[-1][1])
        self.failure(test, err)
        if captured:
            write_evidence(self, True)

    def addError(self, test: unittest.case.TestCase, err: Any) -> None:  # noqa: ANN401 - unittest callback
        super().addError(test, err)
        captured = self.diagnostic(test, self.errors[-1][1])
        self.failure(test, err)
        if captured:
            write_evidence(self, True)

    def addSkip(self, test: unittest.case.TestCase, reason: str) -> None:
        super().addSkip(test, reason)
        if test.id() not in self.started and not hasattr(test, "test_case"):
            self.fixture_failure(test.id())
            return
        self.record(test, "skipped")

    def addExpectedFailure(self, test: unittest.case.TestCase, err: Any) -> None:  # noqa: ANN401 - unittest callback
        super().addExpectedFailure(test, err)
        self.record(test, "skipped")

    def addUnexpectedSuccess(self, test: unittest.case.TestCase) -> None:
        super().addUnexpectedSuccess(test)
        self.record(test, "failed")
        if self.diagnostic(test, "Unexpected success"):
            write_evidence(self, True)

    def addSubTest(
        self,
        test: unittest.case.TestCase,
        subtest: unittest.case.TestCase,
        err: Any,  # noqa: ANN401 - unittest callback
    ) -> None:
        failures = len(self.failures)
        super().addSubTest(test, subtest, err)
        if err is not None:
            self.record(test, "failed")
            message = self.failures[-1][1] if len(self.failures) > failures else self.errors[-1][1]
            if self.diagnostic(subtest, message):
                write_evidence(self, True)


def write_evidence(result: EvidenceResult, incomplete: bool) -> bool:
    outcomes = {key: value for key, value in result.outcomes.items() if key in result.completed}
    incomplete |= bool(result.first_diagnostic and result.first_diagnostic["truncated"])
    payload: dict[str, Any] = {
        "outcomes": outcomes,
        "ineligible": sorted(result.ineligible),
        "incomplete": incomplete,
    }
    if result.first_diagnostic is not None:
        payload["first_diagnostic"] = result.first_diagnostic
    data = json.dumps(payload, sort_keys=True)
    if (
        len(data.encode("utf-8")) > 4 * 1024 * 1024
        or len(outcomes) > 20_000
        or any(len(identity) > 4096 for identity in outcomes)
    ):
        incomplete = True
        retained: dict[str, str] = {}
        base = {**payload, "outcomes": {}, "ineligible": [], "incomplete": True}
        budget = 4 * 1024 * 1024 - len(json.dumps(base, sort_keys=True).encode("utf-8")) - 128
        reference: set[str] = set()
        reference_path = os.environ.get("EJM_REGISTRY_SCHEMA_REFERENCE")
        if reference_path:
            try:
                path = Path(reference_path)
                if (
                    path.is_file()
                    and not path.is_symlink()
                    and path.stat().st_size <= 4 * 1024 * 1024
                ):
                    previous = json.loads(path.read_bytes())
                    reference = {
                        key for key, value in previous["outcomes"].items() if value == "failed"
                    }
            except (OSError, ValueError, KeyError, TypeError, AttributeError):
                reference.clear()
        ordered = sorted(
            outcomes.items(),
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
        data = json.dumps(
            {
                **base,
                "outcomes": retained,
                "ineligible": sorted(result.ineligible.intersection(retained)),
            },
            sort_keys=True,
        )
    temporary = result.report.with_name(result.report.name + ".tmp")
    temporary.write_text(data, encoding="utf-8")
    temporary.replace(result.report)
    return incomplete


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
    factory = partial(EvidenceResult, report=report, expected=expected)
    result = unittest.TextTestRunner(verbosity=2, resultclass=factory).run(suite)
    if not isinstance(result, EvidenceResult):
        raise TypeError("Unittest produced no evidence")
    incomplete = (
        not expected
        or result.started != expected
        or result.completed != expected
        or set(result.outcomes) != expected
        or bool(result.fixture_failures or result.ineligible)
    )
    incomplete = write_evidence(result, incomplete)
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
        record_incomplete = False
        if isinstance(record, dict) and set(record) in (
            {"outcomes", "ineligible", "incomplete"},
            {"outcomes", "ineligible", "incomplete", "first_diagnostic"},
        ):
            if (
                not isinstance(record["ineligible"], list)
                or not all(isinstance(key, str) for key in record["ineligible"])
                or not isinstance(record["incomplete"], bool)
            ):
                raise ValueError("Invalid unittest completeness")
            if "first_diagnostic" in record:
                diagnostic = record["first_diagnostic"]
                if (
                    not isinstance(diagnostic, dict)
                    or set(diagnostic) != {"test", "message", "truncated"}
                    or not isinstance(diagnostic["test"], str)
                    or len(diagnostic["test"]) > 1024
                    or not isinstance(diagnostic["message"], str)
                    or len(diagnostic["message"]) > 8197
                    or not isinstance(diagnostic["truncated"], bool)
                    or (diagnostic["truncated"] and not record["incomplete"])
                ):
                    raise ValueError("Invalid unittest diagnostic")
            ineligible.update(record["ineligible"])
            record_incomplete = record["incomplete"]
            incomplete |= record_incomplete
            record = record["outcomes"]
        if (
            not isinstance(record, dict)
            or (not record and not record_incomplete)
            or len(record) > 20_000
        ):
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
