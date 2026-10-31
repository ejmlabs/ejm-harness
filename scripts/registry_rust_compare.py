#!/usr/bin/env python3
"""Compare two registry nextest reports without hiding incomplete evidence."""

import importlib.util
import os
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "flake_check", Path(__file__).with_name("flake_check.py")
)
if spec is None or spec.loader is None:
    raise RuntimeError("Cannot load the Rust test reporter")
reporter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reporter)


def main() -> int:
    if len(sys.argv) != 5:
        print("Expected INVENTORY FIRST SECOND SUMMARY", file=sys.stderr)
        return 1
    listing, first, second, summary = (Path(arg) for arg in sys.argv[1:])
    incomplete = False
    try:
        expected = reporter.inventory(listing)
    except (OSError, ValueError, KeyError, TypeError, AttributeError):
        expected = None
        incomplete = True
    results = []
    known_failures = []
    for path in (first, second):
        try:
            record = reporter.outcomes(path)
            results.append(record)
            known_failures.append({key for key, status in record.items() if status == "failure"})
        except (OSError, ValueError, ET.ParseError):
            incomplete = True
            try:
                record, failures = reporter.partial_outcomes(
                    path, known_failures[0] if known_failures else None
                )
                results.append(record)
                known_failures.append(failures)
            except (OSError, ValueError, ET.ParseError):
                results.append({})
                known_failures.append(set())
    a, b = results
    mixed = sorted(key for key in a.keys() & b.keys() if {a[key], b[key]} == {"passed", "failure"})
    incomplete |= expected is None or set(a) != expected or set(b) != expected
    incomplete |= any(
        status in ("error", "skipped") for result in results for status in result.values()
    )
    failed = any(known_failures)
    for name, record in zip(("FIRST_OUTCOME", "SECOND_OUTCOME"), results, strict=True):
        outcome = os.environ.get(name)
        has_failures = "failure" in record.values()
        if outcome is not None and (
            outcome not in ("success", "failure") or (outcome == "success") == has_failures
        ):
            incomplete = True
    result = {
        "incomplete": incomplete,
        "mixed": mixed,
        "first_failed": sorted(known_failures[0]),
        "second_failed": sorted(known_failures[1]),
        "first_diagnostic": reporter.first_junit_diagnostic(first),
        "second_diagnostic": reporter.first_junit_diagnostic(second),
    }
    for name in ("FIRST_OUTCOME", "SECOND_OUTCOME"):
        outcome = os.environ.get(name)
        if outcome is not None:
            result[name.lower()] = (
                outcome if outcome in ("success", "failure", "cancelled", "skipped") else "invalid"
            )
    encoded, truncated = reporter.bounded_summary(
        result, ("mixed", "first_failed", "second_failed")
    )
    incomplete |= truncated
    summary.write_text(encoded, encoding="utf-8")
    for suite, name in mixed:
        print(f"flaky-test: {suite} :: {name}")
    return int(incomplete or failed)


if __name__ == "__main__":
    raise SystemExit(main())
