#!/usr/bin/env python3
"""Compare independent nextest reports without turning a failure green."""

import importlib.util
import json
import os
import re
import subprocess
import sys
import threading
import xml.etree.ElementTree as ET
from collections import Counter
from collections.abc import Iterable
from pathlib import Path
from typing import Any, BinaryIO

_identity_spec = importlib.util.spec_from_file_location(
    "junit_stream", Path(__file__).with_name("junit_stream.py")
)
if _identity_spec is None or _identity_spec.loader is None:
    raise RuntimeError("Cannot load bounded identity filter")
_identity_module = importlib.util.module_from_spec(_identity_spec)
_identity_spec.loader.exec_module(_identity_module)
IdentityFilter = _identity_module.IdentityFilter


def bounded_summary(data: dict[str, Any], keys: tuple[str, ...]) -> tuple[str, bool]:
    limit = 4 * 1024 * 1024
    encoded = json.dumps(data, ensure_ascii=True, separators=(",", ":")) + "\n"
    if len(encoded.encode("utf-8")) <= limit:
        return encoded, False
    retained = {**data, "incomplete": True, "summary_truncated": True}
    for key in keys:
        retained[key] = []
    budget = limit - len(
        (json.dumps(retained, ensure_ascii=True, separators=(",", ":")) + "\n").encode("utf-8")
    )
    for index, key in enumerate(keys):
        quota = budget // (len(keys) - index)
        for item in data[key]:
            size = len(json.dumps(item, ensure_ascii=True, separators=(",", ":")).encode("utf-8"))
            size += bool(retained[key])
            if size <= quota:
                retained[key].append(item)
                quota -= size
                budget -= size
    return json.dumps(retained, ensure_ascii=True, separators=(",", ":")) + "\n", True


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
    if root.get("incomplete") == "true":
        raise ValueError("test report is incomplete")
    results = {}
    count = 0
    for suite in root.findall("testsuite"):
        suite_name = suite.get("name")
        if not suite_name:
            raise ValueError("test suite has no name")
        suite_results = []
        for case in suite.findall("testcase"):
            count += 1
            if count > 20_000:
                raise ValueError("too many cases")
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


def partial_outcomes(
    path: Path, priority: set[tuple[str, str]] | None = None
) -> tuple[dict[tuple[str, str], str], set[tuple[str, str]]]:
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 50 * 1024 * 1024:
        raise ValueError("invalid report")
    text = path.read_bytes()
    if re.search(rb"<!\s*(?:DOCTYPE|ENTITY)", text, re.IGNORECASE):
        raise ValueError("unsafe XML")
    try:
        root = ET.fromstring(text)
    except ET.ParseError:
        root = ET.Element("testsuites")
        parser = ET.XMLPullParser(events=("start", "end"))
        stack = []
        suite = None
        for offset in range(0, len(text), 8192):
            broken = False
            try:
                parser.feed(text[offset : offset + 8192])
            except ET.ParseError:
                broken = True
            try:
                for event, element in parser.read_events():
                    if event == "start":
                        stack.append(element.tag)
                        if stack == ["testsuites", "testsuite"]:
                            suite = ET.SubElement(root, "testsuite", element.attrib)
                    else:
                        if stack == ["testsuites", "testsuite", "testcase"] and suite is not None:
                            suite.append(element)
                        if stack:
                            stack.pop()
            except ET.ParseError:
                broken = True
            if broken:
                break
    if root.tag != "testsuites":
        raise ValueError("invalid report root")
    outcomes: dict[tuple[str, str], str] = {}
    identities = IdentityFilter()
    exact_identities: set[tuple[str, str]] = set()
    evictable: dict[tuple[str, str], None] = {}
    failures: set[tuple[str, str]] = set()
    for suite in root.findall("testsuite"):
        suite_name = suite.get("name")
        if not suite_name or len(suite_name) > 1024:
            continue
        for case in suite.findall("testcase"):
            name = case.get("name")
            if not name or len(name) > 1024:
                continue
            key = (suite_name, name)
            states = [child.tag for child in case if child.tag in ("failure", "error", "skipped")]
            if case.get("classname") == suite_name and "failure" in states and len(failures) < 20_000:
                failures.add(key)
            maybe_duplicate = identities.add(repr(key))
            if len(exact_identities) < 20_000:
                duplicate = key in exact_identities
                exact_identities.add(key)
            else:
                duplicate = maybe_duplicate
            if duplicate:
                outcomes.pop(key, None)
                evictable.pop(key, None)
                continue
            if (
                case.get("classname") != suite_name
                or len(states) > 1
                or any(child.tag.startswith(("flaky", "rerun")) for child in case)
            ):
                continue
            status = states[0] if states else "passed"
            preferred = status == "failure" or (priority is not None and key in priority)
            if len(outcomes) >= 20_000:
                if not preferred or not evictable:
                    continue
                victim = next(iter(evictable))
                evictable.pop(victim)
                outcomes.pop(victim)
            outcomes[key] = status
            if not preferred:
                evictable[key] = None
    return outcomes, failures


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


def run(
    command: list[str],
    log: Path,
    output: Path | None = None,
    cargo_results: Path | None = None,
    priority: set[tuple[str, str]] | None = None,
) -> int | None:
    partial = (
        None if cargo_results is None else cargo_results.with_name(cargo_results.name + ".partial")
    )
    if partial is not None:
        partial.write_text("incomplete\n", encoding="utf-8")
    try:
        process = subprocess.Popen(
            command,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE if output else subprocess.STDOUT,
        )
    except OSError as error:
        log.write_text(f"Could not start {command[0]}: {error}\n", encoding="utf-8")
        return None

    truncated: list[Path] = []

    def capture(pipe: BinaryIO, path: Path, limit: int, echo: bool) -> None:
        written = 0
        search_tail = b""
        failure: bytes | None = None
        diagnostic: bytes | None = None
        tail = b""
        line_buffer = b""
        dropping_line = False
        suite = None
        remaining = 0
        seen: dict[tuple[str, str], str] = {}
        identities = IdentityFilter()
        exact_identities: set[tuple[str, str]] = set()
        evictable: dict[tuple[str, str], None] = {}
        result_bytes = 0
        results_incomplete = False
        results_dirty = False

        def persist(target: Path, data: bytes) -> None:
            temporary = target.with_name(target.name + ".tmp")
            temporary.write_bytes(data)
            temporary.replace(target)

        def persist_partial(diagnostics_changed: bool = False) -> None:
            nonlocal results_dirty
            if diagnostics_changed and (failure is not None or diagnostic is not None):
                details = (failure, diagnostic)
                if failure is not None and diagnostic is not None and diagnostic in failure:
                    details = (failure, None)
                persist(
                    path.with_name(path.name + ".failure.log"),
                    b"\n".join(part for part in details if part is not None),
                )
            if echo and path in truncated:
                persist(path.with_name(path.name + ".tail.log"), tail)
            if cargo_results is not None and echo and results_dirty:
                persist(
                    cargo_results,
                    json.dumps(
                        [[*key, state] for key, state in seen.items()], ensure_ascii=False
                    ).encode("utf-8"),
                )
                results_dirty = False

        with path.open("wb") as stream:
            while chunk := os.read(pipe.fileno(), 65536):
                previous_details = (failure, diagnostic)
                if echo:
                    tail = (tail + chunk)[-65536:]
                if echo and failure is None:
                    candidate = search_tail + chunk
                    marker = re.search(
                        rb"(?m)^(?:thread '[^\r\n]{0,512}'(?: \([^\r\n]{0,64}\))? panicked at|"
                        rb"Error:|error: test failed|[ \t]*FAIL \[|test [^\r\n]{1,1024} \.\.\. FAILED|"
                        rb"---- [^\r\n]{1,1024} stdout ----|Caused by:|process didn't exit successfully)",
                        candidate,
                    )
                    if marker:
                        failure = candidate[max(0, marker.start() - 1024) :][:65536]
                elif failure is not None and len(failure) < 65536:
                    failure += chunk[: 65536 - len(failure)]
                if echo and diagnostic is None:
                    candidate = search_tail + chunk
                    marker = re.search(
                        rb"(?m)^(?:thread '[^\r\n]{0,512}'(?: \([^\r\n]{0,64}\))? panicked at|"
                        rb"Error:|Caused by:|error:|process didn't exit successfully|"
                        rb"[^\r\n]{0,512}(?:signal:|timed out|timeout))",
                        candidate,
                    )
                    if marker:
                        diagnostic = candidate[max(0, marker.start() - 1024) :][:65536]
                elif diagnostic is not None and len(diagnostic) < 65536:
                    diagnostic += chunk[: 65536 - len(diagnostic)]
                if echo:
                    search_tail = (search_tail + chunk)[-2048:]
                if cargo_results is not None and echo:
                    line_buffer += chunk
                    while b"\n" in line_buffer:
                        line, line_buffer = line_buffer.split(b"\n", 1)
                        if dropping_line or len(line) > 4096:
                            if (
                                not dropping_line
                                and line.startswith(b"test ")
                                and suite is not None
                                and remaining
                            ):
                                results_incomplete = True
                            dropping_line = False
                            continue
                        heading = re.fullmatch(rb"\s*Running (?:unittests |tests/).+ \(.+\)", line)
                        doc_heading = re.fullmatch(rb"\s*Doc-tests (\S+)\s*", line)
                        count = re.fullmatch(rb"running (\d+) tests?", line)
                        case = re.fullmatch(rb"test (.+) \.\.\. (ok|FAILED|ignored)", line)
                        if heading:
                            suite = line.strip().decode("utf-8", "replace")
                            remaining = 0
                        elif doc_heading:
                            suite = "doctest:" + doc_heading[1].decode("utf-8", "replace")
                            remaining = 0
                        elif count and suite is not None:
                            remaining = int(count[1])
                        elif case and suite is not None and remaining:
                            remaining -= 1
                            results_dirty = True
                            name = case[1].decode("utf-8", "replace")
                            key = (suite, name)
                            state = {
                                b"ok": "passed",
                                b"FAILED": "failure",
                                b"ignored": "skipped",
                            }[case[2]]
                            if len(name) > 1024 or len(suite) > 2048:
                                results_incomplete = True
                                continue
                            filtered = identities.add(repr(key))
                            if len(exact_identities) < 20_000:
                                duplicate = key in exact_identities
                                exact_identities.add(key)
                            else:
                                duplicate = filtered
                            if duplicate:
                                previous = seen.pop(key, None)
                                if previous is not None:
                                    result_bytes -= len(
                                        json.dumps([suite, name, previous], ensure_ascii=False).encode()
                                    ) + 2
                                    evictable.pop(key, None)
                                results_incomplete = True
                            else:
                                size = len(
                                    json.dumps([suite, name, state], ensure_ascii=False).encode()
                                ) + 2
                                preferred = state == "failure" or (
                                    priority is not None and key in priority
                                )
                                if preferred:
                                    while (
                                        len(seen) >= 20_000
                                        or result_bytes + size + 2 > 3 * 1024 * 1024
                                    ):
                                        if not evictable:
                                            break
                                        victim = next(iter(evictable))
                                        evictable.pop(victim)
                                        old = seen.pop(victim)
                                        result_bytes -= len(
                                            json.dumps([*victim, old], ensure_ascii=False).encode()
                                        ) + 2
                                        results_incomplete = True
                                if len(seen) < 20_000 and result_bytes + size + 2 <= 3 * 1024 * 1024:
                                    seen[key] = state
                                    result_bytes += size
                                    if not preferred:
                                        evictable[key] = None
                                else:
                                    results_incomplete = True
                        elif line.startswith(b"test result:"):
                            suite = None
                            remaining = 0
                    if len(line_buffer) > 4096:
                        if line_buffer.startswith(b"test ") and suite is not None and remaining:
                            results_incomplete = True
                        line_buffer = b""
                        dropping_line = True
                kept = chunk[: max(0, limit - written)]
                stream.write(kept)
                stream.flush()
                if echo and kept:
                    sys.stdout.buffer.write(kept)
                    sys.stdout.buffer.flush()
                written += len(kept)
                if len(kept) != len(chunk) and path not in truncated:
                    truncated.append(path)
                persist_partial(previous_details != (failure, diagnostic))
        if cargo_results is not None and echo:
            results_dirty = True
            persist_partial()
            if results_incomplete:
                truncated.append(cargo_results)

    assert process.stdout is not None
    threads = []
    if output:
        assert process.stderr is not None
        thread = threading.Thread(target=capture, args=(process.stderr, log, 4 * 1024 * 1024, True))
        thread.start()
        threads.append(thread)
        capture(process.stdout, output, 50 * 1024 * 1024, False)
    else:
        capture(process.stdout, log, 4 * 1024 * 1024, True)
    code = process.wait()
    for thread in threads:
        thread.join()
    if truncated:
        print(f"Output exceeded its limit: {truncated[0]}", file=sys.stderr)
        return None
    if partial is not None and code in (0, 101):
        partial.unlink()
    return code


def doctest_outcomes(path: Path, expected: Counter[str]) -> tuple[dict[tuple[str, str], str], bool]:
    results: dict[tuple[str, str], str] = {}
    ambiguous: set[tuple[str, str]] = set()
    suites: Counter[str] = Counter()
    complete = True
    suite = None
    running = None
    statuses: list[str] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        heading = re.fullmatch(r"\s*Doc-tests (\S+)\s*", line)
        count = re.fullmatch(r"running (\d+) tests?", line)
        case = re.fullmatch(r"test (.+) \.\.\. (ok|FAILED|ignored)", line)
        summary = re.match(
            r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; "
            r"(\d+) ignored; (\d+) measured; (\d+) filtered out;",
            line,
        )
        if heading:
            if suite is not None:
                complete = False
            suite = heading[1]
            suites[suite] += 1
            running = None
            statuses = []
        elif count and suite is not None:
            if running is not None:
                complete = False
            running = int(count[1])
        elif case and suite is not None:
            key = (suite, case[1])
            status = {"ok": "passed", "FAILED": "failure", "ignored": "skipped"}[case[2]]
            statuses.append(status)
            if key in results or key in ambiguous:
                results.pop(key, None)
                ambiguous.add(key)
                complete = False
            else:
                results[key] = status
        elif summary and suite is not None:
            passed, failed, ignored, measured, filtered = map(int, summary.groups())
            counts = Counter(statuses)
            if (
                running is None
                or running != len(statuses)
                or (passed, failed, ignored) != (counts["passed"], counts["failure"], counts["skipped"])
                or ignored or measured or filtered
            ):
                complete = False
            suite = None
    return results, complete and suite is None and suites == expected


def doctest_inventory(path: Path, expected: Counter[str]) -> set[tuple[str, str]]:
    suites: Counter[str] = Counter()
    results: set[tuple[str, str]] = set()
    suite = None
    count = 0
    for line in path.read_text(encoding="utf-8").splitlines():
        heading = re.fullmatch(r"\s*Doc-tests (\S+)\s*", line)
        case = re.fullmatch(r"(.+): test", line)
        summary = re.fullmatch(r"(\d+) tests?, 0 benchmarks?", line)
        if heading:
            if suite is not None:
                raise ValueError("missing doctest inventory summary")
            suite = heading[1]
            suites[suite] += 1
            count = 0
        elif case and suite is not None:
            key = (suite, case[1])
            if key in results:
                raise ValueError("duplicate doctest inventory entry")
            results.add(key)
            count += 1
        elif summary and suite is not None:
            if int(summary[1]) != count:
                raise ValueError("incomplete doctest inventory")
            suite = None
    if suite is not None or suites != expected:
        raise ValueError("missing doctest inventory target")
    return results


def failure_keys(paths: Iterable[Path]) -> set[tuple[str, str]]:
    failures = set()
    for path in paths:
        try:
            failures.update(
                (suite, name)
                for suite, name, state in json.loads(path.read_text(encoding="utf-8"))
                if state == "failure"
            )
        except (OSError, ValueError, TypeError):
            continue
    return failures


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
    inventory_code = run(
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
        output / "inventory.log",
        inventory_path,
    )
    baseline = []
    attempts = []
    docs = []
    try:
        metadata_path = output / "metadata.json"
        if run(
            ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
            output / "metadata.log",
            metadata_path,
        ) != 0:
            raise ValueError("cargo metadata did not complete")
        metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
        members = set(metadata["workspace_members"])
        if any(
            not target["doctest"]
            for package in metadata["packages"] if package["id"] in members
            for target in package["targets"]
            if any(kind in ("lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro") for kind in target["kind"])
        ):
            raise ValueError("workspace library disables doctests")
        doc_targets = Counter(
            target["name"].replace("-", "_")
            for package in metadata["packages"] if package["id"] in members
            for target in package["targets"] if target["doctest"]
        )
    except (OSError, subprocess.CalledProcessError, ValueError, KeyError, TypeError) as error:
        print(f"Cannot inventory doctest targets: {error}", file=sys.stderr)
        doc_targets = None
    doc_inventory_code = run(
        ["cargo", "test", "--locked", "--workspace", "--doc", "--no-fail-fast", "--color", "never", "--", "--list"],
        output / "doctest-inventory.log",
    )
    for number in (1, 2):
        prior = failure_keys(
            output / f"{kind}-{n}.json"
            for kind, n in (("cargo-test", 1), ("doctest", 1))
            if n < number
        )
        baseline.append(
            run(
                ["cargo", "test", "--locked", "--workspace", "--no-fail-fast", "--color", "never"],
                output / f"cargo-test-{number}.log",
                cargo_results=output / f"cargo-test-{number}.json",
                priority=prior,
            )
        )
        report = output / f"attempt-{number}.xml"
        config = output / f"attempt-{number}.toml"
        config.write_text(
            "[profile.ci.junit]\npath = "
            + json.dumps(str(report) + ".fifo")
            + "\nstore-failure-output = false\n",
            encoding="utf-8",
        )
        os.environ["EJM_HARNESS_JUNIT_REFERENCE"] = str(output / "attempt-1.xml") if number == 2 else ""
        attempts.append(
            run(
                [
                    sys.executable,
                    str(Path(__file__).with_name("bounded_report.py")),
                    "fifo-xml",
                    str(report),
                    str(50 * 1024 * 1024),
                    "--",
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
        os.environ.pop("EJM_HARNESS_JUNIT_REFERENCE", None)
        # nextest does not run doctests; keep their logs and exit codes separately.
        docs.append(
            run(
                ["cargo", "test", "--locked", "--workspace", "--doc", "--no-fail-fast", "--color", "never"],
                output / f"doctest-{number}.log",
                cargo_results=output / f"doctest-{number}.json",
                priority=prior | failure_keys([output / f"cargo-test-{number}.json"]),
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
        first, failures = partial_outcomes(output / "attempt-1.xml")
        second, _ = partial_outcomes(output / "attempt-2.xml", failures)
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
    try:
        cargo_a, cargo_b = (
            {
                (suite, name): status
                for suite, name, status in json.loads(
                    (output / f"cargo-test-{number}.json").read_text(encoding="utf-8")
                )
            }
            for number in (1, 2)
        )
        cargo_mixed = sorted(
            (f"cargo:{suite}", name)
            for suite, name in cargo_a.keys() & cargo_b.keys()
            if not suite.startswith("doctest:")
            if {cargo_a[suite, name], cargo_b[suite, name]} == {"passed", "failure"}
        )
        mixed.extend(cargo_mixed)
        baseline_differed |= bool(cargo_mixed)
    except (OSError, ValueError, TypeError):
        incomplete = True
    docs_differed = all(code is not None for code in docs) and (docs[0] == 0) != (docs[1] == 0)
    doc_results = []
    try:
        if doc_targets is None or doc_inventory_code != 0:
            raise ValueError("doctest inventory unavailable")
        expected_docs = doctest_inventory(output / "doctest-inventory.log", doc_targets)
    except (OSError, UnicodeError, ValueError) as error:
        print(f"Cannot inventory doctests: {error}", file=sys.stderr)
        expected_docs = None
        incomplete = True
    for name in ("cargo-test-1", "cargo-test-2", "doctest-1", "doctest-2"):
        try:
            if doc_targets is None:
                raise ValueError("doctest inventory unavailable")
            results, complete = doctest_outcomes(output / f"{name}.log", doc_targets)
            doc_results.append(results)
            if not complete or expected_docs is None or results.keys() != expected_docs:
                raise ValueError("doctest results are incomplete")
        except (OSError, UnicodeError, ValueError) as error:
            print(f"Cannot verify doctests in {name}: {error}", file=sys.stderr)
            incomplete = True
    doc_keys = set().union(*(results.keys() for results in doc_results))
    mixed.extend(
        (f"doctest:{suite}", name)
        for suite, name in sorted(doc_keys)
        if {"passed", "failure"} <= {results.get((suite, name)) for results in doc_results}
    )
    streamed_docs: dict[tuple[str, str], set[str]] = {}
    for kind in ("cargo-test", "doctest"):
        for number in (1, 2):
            try:
                records = json.loads(
                    (output / f"{kind}-{number}.json").read_text(encoding="utf-8")
                )
                for suite, name, state in records:
                    if suite.startswith("doctest:"):
                        streamed_docs.setdefault((suite, name), set()).add(state)
            except (OSError, ValueError, TypeError):
                incomplete = True
    mixed = sorted(
        set(mixed)
        | {key for key, states in streamed_docs.items() if {"passed", "failure"} <= states}
    )
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
    encoded, truncated = bounded_summary(summary, ("mixed",))
    incomplete |= truncated
    (output / "summary.json").write_text(encoded, encoding="utf-8")
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
