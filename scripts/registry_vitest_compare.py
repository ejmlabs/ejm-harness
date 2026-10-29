#!/usr/bin/env python3
"""Compare independent registry Vitest reports against a runtime inventory."""

import json
import os
import sys
from pathlib import Path
from typing import Any

MAX_BYTES = 4 * 1024 * 1024
MAX_TESTS = 20_000


def unique(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value = dict(pairs)
    if len(value) != len(pairs):
        raise ValueError("duplicate JSON field")
    return value


def read(path: Path) -> Any:  # noqa: ANN401 - JSON is untyped at this boundary
    if path.is_symlink() or not path.is_file() or path.stat().st_size > MAX_BYTES:
        raise ValueError("missing or oversized report")
    return json.loads(path.read_bytes(), object_pairs_hook=unique)


def identity(file: str, name: str, root: Path) -> str:
    if not file or len(file) > 1024 or not name or len(name) > 1024:
        raise ValueError("invalid test identity")
    relative = Path(file).resolve().relative_to(root.resolve()).as_posix()
    return json.dumps(
        [relative, name.replace(" > ", " ")], ensure_ascii=True, separators=(",", ":")
    )


def bounded_message(message: str) -> str:
    if len(message) <= 8192:
        return message
    return message[:4096] + "\n...\n" + message[-4096:]


def report(
    path: Path, root: Path
) -> tuple[dict[str, str], set[str], set[str], bool, dict[str, str] | None]:
    data = read(path)
    if not isinstance(data, dict) or not isinstance(data.get("testResults"), list):
        raise ValueError("invalid Vitest report")
    outcomes: dict[str, str] = {}
    ambiguous: set[str] = set()
    failures: set[str] = set()
    counts = {"passed": 0, "failed": 0, "pending": 0, "skipped": 0, "todo": 0}
    incomplete = False
    total = 0
    diagnostic = None
    for suite in data["testResults"]:
        if (
            not isinstance(suite, dict)
            or not isinstance(suite.get("name"), str)
            or not isinstance(suite.get("assertionResults"), list)
        ):
            incomplete = True
            continue
        suite_failed = False
        for case in suite["assertionResults"]:
            total += 1
            if not isinstance(case, dict) or not isinstance(case.get("fullName"), str):
                incomplete = True
                continue
            try:
                key = identity(suite["name"], case["fullName"], root)
            except (ValueError, OSError):
                incomplete = True
                continue
            if total > MAX_TESTS:
                incomplete = True
                if key not in outcomes and key not in ambiguous:
                    continue
            if not isinstance(case.get("status"), str) or case["status"] not in counts:
                incomplete = True
                outcomes.pop(key, None)
                ambiguous.add(key)
                continue
            state = case["status"]
            counts[state] += 1
            if state == "failed":
                failures.add(key)
                suite_failed = True
                if diagnostic is None:
                    messages = case.get("failureMessages")
                    message = (
                        next((item for item in messages if isinstance(item, str)), "")
                        if isinstance(messages, list)
                        else ""
                    )
                    diagnostic = {"test": key, "message": bounded_message(message)}
            if key in outcomes or key in ambiguous:
                outcomes.pop(key, None)
                ambiguous.add(key)
                incomplete = True
            else:
                outcomes[key] = state
            if state not in ("passed", "failed"):
                incomplete = True
        if suite.get("status") == "failed" and not suite_failed:
            incomplete = True
            if diagnostic is None:
                message = suite.get("message")
                if isinstance(message, str):
                    diagnostic = {
                        "test": suite["name"][:1024], "message": bounded_message(message)
                    }
    expected_counts = {
        "numTotalTests": total,
        "numPassedTests": counts["passed"],
        "numFailedTests": counts["failed"],
        "numPendingTests": counts["pending"] + counts["skipped"],
        "numTodoTests": counts["todo"],
    }
    if not total or any(
        type(data.get(key)) is not int or data[key] != value
        for key, value in expected_counts.items()
    ):
        incomplete = True
    if type(data.get("success")) is not bool or data["success"] != (counts["failed"] == 0):
        incomplete = True
    return outcomes, ambiguous, failures, incomplete, diagnostic


def compare(listing: Path, first: Path, second: Path, root: Path) -> dict[str, Any]:
    incomplete = False
    expected: set[str] | None = None
    try:
        cases = read(listing)
        if not isinstance(cases, list) or not cases or len(cases) > MAX_TESTS:
            raise ValueError("invalid inventory")
        expected = set()
        for case in cases:
            if (
                not isinstance(case, dict)
                or not isinstance(case.get("file"), str)
                or not isinstance(case.get("name"), str)
            ):
                raise ValueError("invalid inventory case")
            key = identity(case["file"], case["name"], root)
            if key in expected:
                raise ValueError("ambiguous inventory")
            expected.add(key)
    except (OSError, ValueError, TypeError, RecursionError):
        incomplete = True
    parsed = []
    for path in (first, second):
        try:
            parsed.append(report(path, root))
        except (OSError, ValueError, TypeError, RecursionError):
            parsed.append(({}, set(), set(), True, None))
    a, b = parsed
    incomplete |= a[3] or b[3] or expected is None or set(a[0]) != expected or set(b[0]) != expected
    common = a[0].keys() & b[0].keys() - a[1] - b[1]
    mixed = sorted(key for key in common if {a[0][key], b[0][key]} == {"passed", "failed"})
    return {
        "incomplete": incomplete,
        "mixed": mixed,
        "first_failed": sorted(a[2]),
        "second_failed": sorted(b[2]),
        "first_diagnostic": a[4],
        "second_diagnostic": b[4],
    }


def main() -> int:
    if len(sys.argv) != 6:
        print("Expected INVENTORY FIRST SECOND ROOT SUMMARY", file=sys.stderr)
        return 1
    listing, first, second, root, summary = (Path(arg) for arg in sys.argv[1:])
    result = compare(listing, first, second, root)
    for name, failures in (
        ("FIRST_OUTCOME", result["first_failed"]),
        ("SECOND_OUTCOME", result["second_failed"]),
    ):
        outcome = os.environ.get(name)
        if outcome is not None and (
            outcome not in ("success", "failure") or (outcome == "success") == bool(failures)
        ):
            result["incomplete"] = True
        if outcome is not None:
            result[name.lower()] = (
                outcome if outcome in ("success", "failure", "cancelled", "skipped") else "invalid"
            )
    encoded = json.dumps(result, ensure_ascii=True, separators=(",", ":")) + "\n"
    if len(encoded.encode("utf-8")) > MAX_BYTES:
        result["incomplete"] = True
        encoded = json.dumps(
            {
                "incomplete": True,
                "reason": "summary too large",
                "first_diagnostic": result["first_diagnostic"],
                "second_diagnostic": result["second_diagnostic"],
            },
            ensure_ascii=True,
        ) + "\n"
    summary.write_text(encoded, encoding="utf-8")
    for key in result["mixed"]:
        print("flaky-test:", key)
    return int(
        result["incomplete"] or bool(result["first_failed"]) or bool(result["second_failed"])
    )


if __name__ == "__main__":
    raise SystemExit(main())
