#!/usr/bin/env python3
"""Compare independent nextest reports without turning a failure green."""

import json
import os
import re
import subprocess
import sys
import xml.etree.ElementTree as ET
from collections import Counter
from collections.abc import Iterable
from pathlib import Path


def outcomes(path: Path) -> dict[tuple[str, str], str]:
    """Return per-test results from a complete nextest JUnit report."""
    # shortcut: Reports over 50 MiB are refused; raise this bound if suites exceed it.
    if path.is_symlink() or path.stat().st_size > 50 * 1024 * 1024:
        raise ValueError("test report exceeds 50 MiB")
    data = path.read_bytes()
    if len(data) > 50 * 1024 * 1024 or re.search(rb"<!\s*(?:DOCTYPE|ENTITY)", data, re.IGNORECASE):
        raise ValueError("unsafe test report")
    root = ET.fromstring(data)
    if root.tag != "testsuites":
        raise ValueError("not a nextest JUnit report")
    results = {}
    for suite in root.findall("testsuite"):
        suite_name = suite.get("name")
        if not suite_name:
            raise ValueError("test suite has no name")
        suite_results = []
        for case in suite.findall("testcase"):
            name = case.get("name")
            if not name or case.get("classname") != suite_name:
                raise ValueError("test case has no matching identity")
            key = (suite_name, name)
            if key in results:
                raise ValueError(f"duplicate test result: {key}")
            states = [child.tag for child in case if child.tag in ("failure", "error", "skipped")]
            if len(states) > 1 or any(child.tag.startswith(("flaky", "rerun")) for child in case):
                raise ValueError(f"ambiguous test result: {key}")
            status = states[0] if states else "passed"
            results[key] = status
            suite_results.append(status)
        check_counts(suite, suite_results)
    if not results:
        raise ValueError("report contains no tests")
    check_counts(root, results.values())
    return results


def check_counts(element: ET.Element, statuses: Iterable[str]) -> None:
    counts = Counter(statuses)
    expected = {
        "tests": sum(counts.values()),
        "failures": counts["failure"],
        "errors": counts["error"],
        "skipped": counts["skipped"],
    }
    for key, value in expected.items():
        # Older nextest omits the skipped count when it emits no skipped cases.
        if key == "skipped" and key not in element.attrib and value == 0:
            continue
        try:
            actual = int(element.attrib[key])
        except (KeyError, ValueError) as error:
            raise ValueError(f"missing or invalid {key} count") from error
        if actual != value:
            raise ValueError(f"{key} count does not match reported tests")


def inventory(path: Path) -> set[tuple[str, str]]:
    listing = json.loads(path.read_text(encoding="utf-8"))
    results = set()
    count = 0
    for suite_name, suite in listing["rust-suites"].items():
        if suite["status"] != "listed" or suite["binary-id"] != suite_name:
            raise ValueError(f"test suite was not listed: {suite_name}")
        for name, case in suite["testcases"].items():
            count += 1
            key = (suite_name, name)
            if case["ignored"] or case["filter-match"]["status"] != "matches":
                raise ValueError(f"test was ignored or filtered: {key}")
            results.add(key)
    if count != listing["test-count"] or not results:
        raise ValueError("test inventory is empty or incomplete")
    return results


def mixed_tests(first: Path, second: Path, expected: set[tuple[str, str]]) -> list[tuple[str, str]]:
    """Compare complete reports; missing, skipped and execution errors are not passes."""
    a, b = outcomes(first), outcomes(second)
    if a.keys() != expected or b.keys() != expected:
        raise ValueError("test results do not match the inventory")
    if any(a[key] in ("error", "skipped") or b[key] in ("error", "skipped") for key in a):
        raise ValueError("test execution or skip status is incomplete")
    return sorted(key for key in a if {a[key], b[key]} == {"passed", "failure"})


def run(command: list[str], log: Path) -> int | None:
    with log.open("w", encoding="utf-8") as stream:
        try:
            process = subprocess.Popen(
                command,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                encoding="utf-8",
                errors="replace",
            )
        except OSError as error:
            print(f"Could not start {command[0]}: {error}", file=stream)
            return None
        assert process.stdout is not None
        for line in process.stdout:
            sys.stdout.write(line)
            sys.stdout.flush()
            stream.write(line)
        return process.wait()


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: python3 scripts/flake_check.py OUTPUT_DIRECTORY")
    output = Path(sys.argv[1]).resolve()
    output.mkdir()
    helper_build: int | None = 0
    if os.name != "nt":
        target_dir = Path(os.environ.get("CARGO_TARGET_DIR", "target")).resolve()
        os.environ.pop("EJM_NATIVE_STARTUP_HELPER", None)
        os.environ.pop("EJM_NATIVE_OCCUPANCY_HELPER", None)
        try:
            host = subprocess.check_output(["rustc", "--print", "host-tuple"], text=True).strip()
            if not host or "/" in host or "\\" in host or "\n" in host:
                raise ValueError("invalid Rust host tuple")
            helper_build = run(
                [
                    "cargo",
                    "build",
                    "--locked",
                    "-p",
                    "ejm-harness-exec",
                    "--example",
                    "native_owner_helper",
                    "--target-dir",
                    str(target_dir),
                    "--target",
                    host,
                ],
                output / "native-helper-build.log",
            )
            if helper_build == 0:
                helper = str(target_dir / host / "debug/examples/native_owner_helper")
                os.environ["EJM_NATIVE_STARTUP_HELPER"] = helper
                os.environ["EJM_NATIVE_OCCUPANCY_HELPER"] = helper
        except (OSError, subprocess.CalledProcessError, ValueError) as error:
            print(f"Cannot build native test helper: {error}", file=sys.stderr)
            helper_build = None
    inventory_path = output / "inventory.json"
    with (
        inventory_path.open("w", encoding="utf-8") as listing,
        (output / "inventory.log").open("w", encoding="utf-8") as log,
    ):
        try:
            inventory_code = subprocess.run(
                [
                    "cargo",
                    "nextest",
                    "list",
                    "--locked",
                    "--workspace",
                    "--ignore-default-filter",
                    "--message-format",
                    "json",
                    "--user-config-file",
                    "none",
                ],
                stdout=listing,
                stderr=log,
                check=False,
            ).returncode
        except OSError as error:
            print(f"Could not list tests: {error}", file=log)
            inventory_code = None
    baseline = []
    attempts = []
    docs = []
    for number in (1, 2):
        baseline.append(
            run(
                ["cargo", "test", "--locked", "--workspace", "--no-fail-fast", "--color", "never"],
                output / f"cargo-test-{number}.log",
            )
        )
        report = output / f"attempt-{number}.xml"
        config = output / f"attempt-{number}.toml"
        config.write_text(
            "[profile.ci.junit]\npath = "
            + json.dumps(str(report))
            + "\nstore-failure-output = false\n",
            encoding="utf-8",
        )
        attempts.append(
            run(
                [
                    "cargo",
                    "nextest",
                    "run",
                    "--locked",
                    "--workspace",
                    "--ignore-default-filter",
                    "--no-fail-fast",
                    "--retries",
                    "0",
                    "--color",
                    "never",
                    "--profile",
                    "ci",
                    "--config-file",
                    str(config),
                    "--user-config-file",
                    "none",
                    "--no-tests",
                    "fail",
                ],
                output / f"attempt-{number}.log",
            )
        )
        # nextest does not run doctests; keep their logs and exit codes separately.
        docs.append(
            run(
                ["cargo", "test", "--locked", "--workspace", "--doc", "--color", "never"],
                output / f"doctest-{number}.log",
            )
        )
    incomplete = (
        helper_build != 0
        or any(code is None for code in baseline)
        or any(code not in (0, 100) for code in attempts)
        or any(code is None for code in docs)
    )
    mixed = []
    try:
        first = outcomes(output / "attempt-1.xml")
        second = outcomes(output / "attempt-2.xml")
        mixed = sorted(
            key
            for key in first.keys() & second.keys()
            if {first[key], second[key]} == {"passed", "failure"}
        )
    except (OSError, ET.ParseError, ValueError) as error:
        print(f"Cannot compare test outcomes: {error}", file=sys.stderr)
        incomplete = True
    try:
        if inventory_code != 0 or any(code not in (0, 100) for code in attempts):
            raise ValueError("nextest did not complete both test runs")
        expected = inventory(inventory_path)
        mixed_tests(output / "attempt-1.xml", output / "attempt-2.xml", expected)
        for number, code in enumerate(attempts, 1):
            has_failures = "failure" in outcomes(output / f"attempt-{number}.xml").values()
            if has_failures != (code == 100):
                raise ValueError("nextest exit code does not match its report")
    except (OSError, ET.ParseError, ValueError, KeyError, TypeError, AttributeError) as error:
        print(f"Cannot compare test results: {error}", file=sys.stderr)
        incomplete = True
    baseline_differed = all(code is not None for code in baseline) and (baseline[0] == 0) != (
        baseline[1] == 0
    )
    docs_differed = all(code is not None for code in docs) and (docs[0] == 0) != (docs[1] == 0)
    summary = {
        "cargo_test_exit_codes": baseline,
        "cargo_test_inconsistent": baseline_differed,
        "exit_codes": attempts,
        "inventory_exit_code": inventory_code,
        "native_helper_build_exit_code": helper_build,
        "doctest_exit_codes": docs,
        "doctest_inconsistent": docs_differed,
        "mixed": mixed,
        "incomplete": incomplete,
    }
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    for suite, name in mixed:
        print(f"Flaky test: {suite} :: {name}", file=sys.stderr)
    if baseline_differed:
        print("Cargo test suite outcomes differed; inspect cargo-test logs.", file=sys.stderr)
    if docs_differed:
        print("Doctest suite outcomes differed; inspect doctest logs.", file=sys.stderr)
    print(f"Test evidence: {output}")
    return int(incomplete or any(code != 0 for code in baseline + attempts + docs))


if __name__ == "__main__":
    sys.exit(main())
