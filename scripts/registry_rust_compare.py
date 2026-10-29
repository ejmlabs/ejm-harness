#!/usr/bin/env python3
"""Compare two registry nextest reports without hiding incomplete evidence."""

import importlib.util
import json
import os
import re
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


def partial_outcomes(path: Path) -> tuple[dict[tuple[str, str], str], set[tuple[str, str]]]:
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 50 * 1024 * 1024:
        raise ValueError("invalid report")
    text = path.read_text(encoding="utf-8")
    if re.search(r"<!\s*(?:DOCTYPE|ENTITY)", text, re.IGNORECASE):
        raise ValueError("unsafe XML")
    root = ET.fromstring(text)
    if root.tag != "testsuites":
        raise ValueError("invalid report root")
    outcomes: dict[tuple[str, str], str] = {}
    ambiguous: set[tuple[str, str]] = set()
    failures: set[tuple[str, str]] = set()
    count = 0
    for suite in root.findall("testsuite"):
        suite_name = suite.get("name")
        if not suite_name or len(suite_name) > 1024:
            continue
        for case in suite.findall("testcase"):
            count += 1
            name = case.get("name")
            if count > 20_000:
                raise ValueError("too many cases")
            if not name or len(name) > 1024 or case.get("classname") != suite_name:
                continue
            key = (suite_name, name)
            states = [child.tag for child in case if child.tag in ("failure", "error", "skipped")]
            if "failure" in states:
                failures.add(key)
            if (
                key in outcomes
                or key in ambiguous
                or len(states) > 1
                or any(child.tag.startswith(("flaky", "rerun")) for child in case)
            ):
                outcomes.pop(key, None)
                ambiguous.add(key)
            else:
                outcomes[key] = states[0] if states else "passed"
    return outcomes, failures


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
                record, failures = partial_outcomes(path)
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
    }
    for name in ("FIRST_OUTCOME", "SECOND_OUTCOME"):
        outcome = os.environ.get(name)
        if outcome is not None:
            result[name.lower()] = (
                outcome if outcome in ("success", "failure", "cancelled", "skipped") else "invalid"
            )
    encoded = json.dumps(result, ensure_ascii=True, separators=(",", ":")) + "\n"
    if len(encoded.encode("utf-8")) > 4 * 1024 * 1024:
        incomplete = True
        encoded = '{"incomplete":true,"reason":"summary too large"}\n'
    summary.write_text(encoded, encoding="utf-8")
    for suite, name in mixed:
        print(f"flaky-test: {suite} :: {name}")
    return int(incomplete or failed)


if __name__ == "__main__":
    raise SystemExit(main())
