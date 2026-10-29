#!/usr/bin/env python3
"""Compare persisted reliability evidence after an interrupted run."""

import importlib.util
import json
import os
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

spec = importlib.util.spec_from_file_location("flake_check", Path(__file__).with_name("flake_check.py"))
if spec is None or spec.loader is None:
    raise RuntimeError("Cannot load reliability reporter")
reporter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reporter)


def records(path: Path) -> dict[tuple[str, str], str]:
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 4 * 1024 * 1024:
        return {}
    try:
        data = json.loads(path.read_bytes())
        if not isinstance(data, list) or len(data) > 20_000:
            return {}
        result: dict[tuple[str, str], str] = {}
        ambiguous: set[tuple[str, str]] = set()
        for item in data:
            if not isinstance(item, list) or len(item) != 3:
                continue
            suite, name, state = item
            if (not isinstance(suite, str) or not suite or not isinstance(name, str) or not name
                    or len(suite) > 2048 or len(name) > 1024
                    or state not in ("passed", "failure", "skipped")):
                continue
            key = (suite, name)
            if key in result or key in ambiguous:
                result.pop(key, None)
                ambiguous.add(key)
            else:
                result[key] = state
        return result
    except (OSError, ValueError, TypeError, RecursionError):
        return {}


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: compare_partial_flakes.py OUTPUT_DIRECTORY")
    output = Path(sys.argv[1])
    existing = output / "summary.json"
    prior = None
    try:
        if not existing.is_symlink() and existing.stat().st_size <= 4 * 1024 * 1024:
            value = json.loads(existing.read_bytes())
            if isinstance(value, dict) and isinstance(value.get("incomplete"), bool) and isinstance(value.get("mixed"), list):
                if (not value["incomplete"]
                        and all(type(value.get(key)) is int and value[key] == 0
                                for key in ("inventory_exit_code", "native_helper_build_exit_code"))
                        and not value.get("summary_truncated", False)
                        and all(type(value.get(key)) is bool for key in
                                ("cargo_test_inconsistent", "doctest_inconsistent"))
                        and all(isinstance(value.get(key), list) and len(value[key]) == 2
                                and all(type(code) is int for code in value[key])
                                for key in ("cargo_test_exit_codes", "exit_codes", "doctest_exit_codes"))
                        and all(isinstance(item, list) and len(item) == 2
                                and all(isinstance(part, str) and part for part in item)
                                for item in value["mixed"])
                        and all(code in (0, 100) for code in value["exit_codes"])):
                    return int(os.environ.get("PRODUCER_OUTCOME") != "success"
                               or bool(value["mixed"])
                               or value["cargo_test_inconsistent"]
                               or value["doctest_inconsistent"]
                               or any(code != 0 for key in ("cargo_test_exit_codes", "exit_codes",
                                                            "doctest_exit_codes") for code in value[key]))
                prior = value
    except (OSError, ValueError, TypeError, RecursionError):
        pass
    output.mkdir(parents=True, exist_ok=True)
    mixed: set[tuple[str, str]] = set()
    failures: list[set[tuple[str, str]]] = [set(), set()]
    junit: list[dict[tuple[str, str], str]] = []
    for index in (0, 1):
        try:
            found, failed = reporter.partial_outcomes(
                output / f"attempt-{index + 1}.xml", failures[0] if index else None
            )
            junit.append(found)
            failures[index].update(failed)
        except (OSError, ValueError, ET.ParseError):
            junit.append({})
    mixed.update(key for key in junit[0].keys() & junit[1].keys()
                 if {junit[0][key], junit[1][key]} == {"passed", "failure"})
    cargo = [records(output / f"cargo-test-{number}.json") for number in (1, 2)]
    for index, attempt in enumerate(cargo):
        failures[index].update((f"cargo:{suite}", name) for (suite, name), state in attempt.items()
                               if state == "failure" and not suite.startswith("doctest:"))
    mixed.update((f"cargo:{suite}", name) for suite, name in cargo[0].keys() & cargo[1].keys()
                 if not suite.startswith("doctest:")
                 and {cargo[0][suite, name], cargo[1][suite, name]} == {"passed", "failure"})
    docs = [records(output / f"doctest-{number}.json") for number in (1, 2)]
    by_case: dict[tuple[str, str], set[str]] = {}
    for index in (0, 1):
        for attempt in (cargo[index], docs[index]):
            for key, state in attempt.items():
                if key[0].startswith("doctest:"):
                    by_case.setdefault(key, set()).add(state)
                    if state == "failure":
                        failures[index].add(key)
    mixed.update(key for key, states in by_case.items() if {"passed", "failure"} <= states)
    summary = {"incomplete": True, "mixed": sorted(mixed),
               "first_failed": sorted(failures[0]), "second_failed": sorted(failures[1]),
               "first_diagnostic": reporter.first_junit_diagnostic(output / "attempt-1.xml"),
               "second_diagnostic": reporter.first_junit_diagnostic(output / "attempt-2.xml")}
    if prior is not None:
        for name in ("first_diagnostic", "second_diagnostic"):
            if summary[name] is None and isinstance(prior.get(name), dict):
                summary[name] = prior[name]
        summary["mixed"] = sorted({tuple(item) for item in prior["mixed"]
                                   if isinstance(item, (list, tuple)) and len(item) == 2
                                   and all(isinstance(part, str) for part in item)} | mixed)
        for name in ("first_failed", "second_failed"):
            if isinstance(prior.get(name), list):
                summary[name] = sorted({tuple(item) for item in prior[name]
                                        if isinstance(item, (list, tuple)) and len(item) == 2
                                        and all(isinstance(part, str) for part in item)} | set(summary[name]))
        summary = {**prior, **summary}
    encoded, _ = reporter.bounded_summary(summary, ("mixed", "first_failed", "second_failed"))
    temporary = output / "summary.json.tmp"
    temporary.write_text(encoded, encoding="utf-8")
    temporary.replace(output / "summary.json")
    for suite, name in sorted(mixed):
        print(f"Flaky test: {suite} :: {name}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
