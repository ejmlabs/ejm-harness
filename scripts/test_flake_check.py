import json
import os
import signal
import subprocess
import sys
import tempfile
import time
import unittest
import xml.etree.ElementTree as ET
from collections import Counter
from pathlib import Path
from typing import Any

from flake_check import bounded_summary, doctest_inventory, doctest_outcomes, inventory, mixed_tests, outcomes, partial_outcomes


def report(cases: list[tuple[str, str]]) -> str:
    counts = (
        f'tests="{len(cases)}" failures="{sum("<failure" in state for _, state in cases)}" '
        f'errors="{sum("<error" in state for _, state in cases)}" '
        f'skipped="{sum("<skipped" in state for _, state in cases)}"'
    )
    return (
        f'<testsuites {counts}><testsuite name="pkg::suite" {counts}>'
        + "".join(
            f'<testcase classname="pkg::suite" name="{name}">{state}</testcase>'
            for name, state in cases
        )
        + "</testsuite></testsuites>"
    )


def fake_rustc(root: Path) -> None:
    rustc = root / "rustc"
    rustc.write_text("#!/bin/sh\nprintf '%s\\n' fake-host\n", encoding="utf-8")
    rustc.chmod(0o755)


def fake_doctests(cargo: Path) -> None:
    source = cargo.read_text(encoding="utf-8")
    if source.startswith("#!/usr/bin/env python3\n"):
        preamble = '''import json, os, sys
if sys.argv[1] == "metadata":
    print(json.dumps({"workspace_members": ["pkg"], "packages": [{"id": "pkg",
          "targets": [{"name": "pkg", "kind": ["lib"], "doctest": True}]}]}))
    raise SystemExit(0)
if sys.argv[1] == "test":
    if "--list" in sys.argv:
        print("   Doc-tests pkg\\n\\nsrc/lib.rs - example (line 1): test\\n1 test, 0 benchmarks")
        raise SystemExit(0)
    status = os.environ.get("FAKE_DOCTEST_STATUS" if "--doc" in sys.argv else "FAKE_BASELINE_DOCTEST_STATUS", "ok")
    print("   Doc-tests pkg\\n\\nrunning 1 test\\ntest src/lib.rs - example (line 1) ... " + status)
    print("test result: ok. " + str(int(status == "ok")) + " passed; 0 failed; "
          + str(int(status == "ignored")) + " ignored; 0 measured; 0 filtered out; finished in 0.00s")
    if "--doc" in sys.argv:
        raise SystemExit(0)
'''
        cargo.write_text(source.replace("\n", "\n" + preamble, 1), encoding="utf-8")


class FlakeCheckTests(unittest.TestCase):
    def test_bounded_summary_retains_mixed_and_failures(self) -> None:
        records = [f"{index:05}" + "x" * 900 for index in range(2500)]
        encoded, truncated = bounded_summary({
            "incomplete": False, "mixed": records, "first_failed": records,
            "second_failed": records, "first_diagnostic": {"message": "failure"},
        }, ("mixed", "first_failed", "second_failed"))
        result = json.loads(encoded)
        self.assertTrue(truncated)
        self.assertLessEqual(len(encoded.encode()), 4 * 1024 * 1024)
        self.assertTrue(result["incomplete"] and result["summary_truncated"])
        self.assertTrue(result["mixed"] and result["first_failed"] and result["second_failed"])
        self.assertEqual(result["first_diagnostic"]["message"], "failure")

    def write_reports(self, root: Path, first: str, second: str) -> tuple[Path, Path]:
        a, b = root / "first.xml", root / "second.xml"
        a.write_text(first, encoding="utf-8")
        b.write_text(second, encoding="utf-8")
        return a, b

    def test_inventory_checks_discovery(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "inventory.json"
            listing: dict[str, Any] = {
                "test-count": 1,
                "rust-suites": {
                    "pkg::suite": {
                        "binary-id": "pkg::suite",
                        "status": "listed",
                        "testcases": {
                            "x": {"ignored": False, "filter-match": {"status": "matches"}},
                        },
                    },
                },
            }
            path.write_text(json.dumps(listing), encoding="utf-8")
            self.assertEqual(inventory(path), {("pkg::suite", "x")})
            listing["rust-suites"]["pkg::suite"]["testcases"]["x"]["ignored"] = True
            path.write_text(json.dumps(listing), encoding="utf-8")
            with self.assertRaises(ValueError):
                inventory(path)
            listing["rust-suites"]["pkg::suite"]["testcases"]["x"]["ignored"] = False
            match = listing["rust-suites"]["pkg::suite"]["testcases"]["x"]["filter-match"]
            match["status"] = "mismatch"
            path.write_text(json.dumps(listing), encoding="utf-8")
            with self.assertRaises(ValueError):
                inventory(path)
            match["status"] = "matches"
            listing["test-count"] = 2
            path.write_text(json.dumps(listing), encoding="utf-8")
            with self.assertRaises(ValueError):
                inventory(path)

    def test_doctest_results_require_all_targets_and_executed_cases(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "doctest.log"
            passing = (
                "   Doc-tests pkg\n\nrunning 1 test\n"
                "test src/lib.rs - example (line 1) ... ok\n"
                "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n"
            )
            path.write_text(passing, encoding="utf-8")
            self.assertEqual(doctest_outcomes(path, Counter({"pkg": 1}))[1], True)
            ignored = passing.replace("... ok", "... ignored").replace("1 passed; 0 failed; 0 ignored", "0 passed; 0 failed; 1 ignored")
            for value, targets in [
                (ignored, Counter({"pkg": 1})),
                (passing, Counter({"pkg": 1, "missing": 1})),
                (passing.replace("test src/lib.rs - example (line 1) ... ok\n", ""), Counter({"pkg": 1})),
                (passing.replace("0 filtered out", "1 filtered out"), Counter({"pkg": 1})),
            ]:
                path.write_text(value, encoding="utf-8")
                self.assertFalse(doctest_outcomes(path, targets)[1])

    def test_real_ignored_doctest_is_incomplete(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "src").mkdir()
            (root / "Cargo.toml").write_text(
                '[package]\nname = "doctest-fixture"\nversion = "0.1.0"\nedition = "2021"\n',
                encoding="utf-8",
            )
            (root / "src/lib.rs").write_text(
                '/// ```\n/// assert!(true);\n/// ```\n/// ```ignore\n/// assert!(true);\n/// ```\npub fn sample() {}\n',
                encoding="utf-8",
            )
            result = subprocess.run(
                ["cargo", "test", "--offline", "--doc", "--color", "never"],
                cwd=root,
                env={**os.environ, "CARGO_TARGET_DIR": str(root / "target")},
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stdout)
            log = root / "doctest.log"
            log.write_text(result.stdout, encoding="utf-8")
            results, complete = doctest_outcomes(log, Counter({"doctest_fixture": 1}))
            self.assertEqual(set(results.values()), {"passed", "skipped"})
            self.assertFalse(complete)
            listing = subprocess.run(
                ["cargo", "test", "--offline", "--doc", "--color", "never", "--", "--list"],
                cwd=root,
                env={**os.environ, "CARGO_TARGET_DIR": str(root / "target")},
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                check=False,
            )
            self.assertEqual(listing.returncode, 0, listing.stdout)
            log.write_text(listing.stdout, encoding="utf-8")
            self.assertEqual(doctest_inventory(log, Counter({"doctest_fixture": 1})), set(results))

    def test_identical_omissions_and_skips_are_incomplete(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            expected = {("pkg::suite", "x"), ("pkg::suite", "y")}
            for observed in [report([("x", "")]), report([("x", ""), ("y", "<skipped/>")])]:
                paths = self.write_reports(root, observed, observed)
                with self.assertRaises(ValueError):
                    mixed_tests(*paths, expected)

    def test_compares_tests_even_when_both_runs_fail(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            paths = self.write_reports(
                Path(directory),
                report([("first", "<failure/>"), ("second", "")]),
                report([("first", ""), ("second", "<failure/>")]),
            )
            self.assertEqual(
                mixed_tests(*paths, {("pkg::suite", "first"), ("pkg::suite", "second")}),
                [("pkg::suite", "first"), ("pkg::suite", "second")],
            )

    def test_skips_errors_missing_and_duplicate_tests_are_not_flakes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            good = report([("x", ""), ("y", "")])
            for other in [
                report([("x", "<skipped/>"), ("y", "")]),
                report([("x", "<error/>"), ("y", "")]),
                report([("x", "<failure/>")]),
                report([("x", ""), ("x", "<failure/>")]),
                good.replace('tests="2"', 'tests="3"', 1),
                "<testsuites />",
                "<testsuites>",
            ]:
                paths = self.write_reports(root, good, other)
                with self.assertRaises((ValueError, ET.ParseError)):
                    mixed_tests(*paths, {("pkg::suite", "x"), ("pkg::suite", "y")})

    def test_partial_results_preserve_unambiguous_mixed_outcomes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            first, second = self.write_reports(
                Path(directory),
                report([("flip", "<failure/>"), ("duplicate", ""), ("duplicate", "")]),
                report([("flip", ""), ("duplicate", "")]).replace('tests="2"', 'tests="3"', 1),
            )
            for path in (first, second):
                with self.assertRaises(ValueError):
                    outcomes(path)
            a, _ = partial_outcomes(first)
            b, _ = partial_outcomes(second)
            self.assertEqual(
                [key for key in a.keys() & b.keys() if {a[key], b[key]} == {"failure", "passed"}],
                [("pkg::suite", "flip")],
            )
            self.assertNotIn(("pkg::suite", "duplicate"), a)

    def test_log_capture_is_bounded_and_preserves_first_output(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = Path(__file__).with_name("flake_check.py")
            program = (
                "import sys; sys.path.insert(0, sys.argv[1]); from flake_check import run; "
                "from pathlib import Path; "
                "print(run([sys.executable, '-c', "
                "\"import sys; sys.stdout.write('first failure\\\\n' + 'x' * 5000000)\"], "
                "Path(sys.argv[2])))"
            )
            result = subprocess.run(
                [sys.executable, "-c", program, str(script.parent), str(root / "log")],
                capture_output=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0)
            self.assertTrue((root / "log").read_bytes().startswith(b"first failure\n"))
            self.assertEqual((root / "log").stat().st_size, 4 * 1024 * 1024)
            self.assertIn(b"Output exceeded its limit", result.stderr)
            self.assertTrue(result.stdout.endswith(b"None\n"))

    def test_running_child_exposes_partial_logs_and_results(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = Path(__file__).with_name("flake_check.py")
            for overflow in (False, True):
                log = root / f"{overflow}.log"
                results = root / f"{overflow}.json"
                pid_file = root / f"{overflow}.pid"
                child = (
                    "import os,sys,time; from pathlib import Path; "
                    "Path(sys.argv[1]).write_text(str(os.getpid())); "
                    "sys.stdout.write('Running unittests src/lib.rs (target/test)\\n'"
                    " + 'running 1 test\\ntest broken ... FAILED\\nError: live detail\\n'); "
                    "sys.stdout.flush(); "
                    + (
                        "sys.stdout.write('x' * 4200000 + '\\nTAIL_SENTINEL\\n'); "
                        "sys.stdout.flush(); " if overflow else ""
                    )
                    + "time.sleep(60)"
                )
                driver = (
                    "import sys; sys.path.insert(0, sys.argv[1]); from flake_check import run; "
                    "from pathlib import Path; "
                    "run([sys.executable, '-c', sys.argv[4], sys.argv[5]], "
                    "Path(sys.argv[2]), cargo_results=Path(sys.argv[3]))"
                )
                runner = subprocess.Popen(
                    [
                        sys.executable, "-c", driver, str(script.parent), str(log), str(results),
                        child, str(pid_file),
                    ],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                )
                try:
                    deadline = time.monotonic() + 10
                    while time.monotonic() < deadline:
                        failure = log.with_name(log.name + ".failure.log")
                        tail = log.with_name(log.name + ".tail.log")
                        partial = results.with_name(results.name + ".partial")
                        if (
                            pid_file.exists() and log.exists() and results.exists()
                            and failure.exists() and partial.exists()
                            and b"live detail" in failure.read_bytes()
                            and (not overflow or (
                                tail.exists() and b"TAIL_SENTINEL" in tail.read_bytes()
                            ))
                        ):
                            break
                        self.assertIsNone(runner.poll(), "capture exited before the child")
                        time.sleep(0.01)
                    else:
                        self.fail("live evidence was not persisted while the child was running")
                    self.assertIsNone(runner.poll())
                    self.assertIn(b"test broken ... FAILED", log.read_bytes())
                    records = json.loads(results.read_text(encoding="utf-8"))
                    self.assertEqual(records[0][1:], ["broken", "failure"])
                finally:
                    if pid_file.exists():
                        try:
                            os.kill(int(pid_file.read_text()), signal.SIGTERM)
                        except ProcessLookupError:
                            pass
                    try:
                        runner.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        runner.kill()
                        runner.wait(timeout=10)
                self.assertTrue(partial.exists())
            for code in (0, 101):
                results = root / f"completed-{code}.json"
                completed = subprocess.run(
                    [
                        sys.executable, "-c", driver, str(script.parent),
                        str(root / f"completed-{code}.log"), str(results),
                        f"print('running 0 tests'); raise SystemExit({code})",
                        str(root / "unused.pid"),
                    ],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10, check=False,
                )
                self.assertEqual(completed.returncode, 0)
                self.assertFalse(results.with_name(results.name + ".partial").exists())

    def test_log_preserves_first_panic_after_excess_output(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = Path(__file__).with_name("flake_check.py")
            child_program = (
                "import sys; sys.stdout.write('x' * 5000000 + "
                "\"\\nthread 'first' panicked at fixture.rs:1:\\nassertion detail\\n\" + "
                "'y' * 5000000 + \"\\nthread 'second' panicked at fixture.rs:2:\\nother\\n\")"
            )
            program = (
                "import sys; sys.path.insert(0, sys.argv[1]); from flake_check import run; "
                "from pathlib import Path; "
                f"run([sys.executable, '-c', {child_program!r}], "
                "Path(sys.argv[2]))"
            )
            result = subprocess.run(
                [sys.executable, "-c", program, str(script.parent), str(root / "log")],
                capture_output=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0)
            evidence = (root / "log.failure.log").read_bytes()
            self.assertIn(b"assertion detail", evidence)
            self.assertNotIn(b"thread 'second'", evidence)
            self.assertLessEqual(len(evidence), 65536)

    def test_log_retains_nonpanic_error_after_excess_output(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            child = (
                "import sys; sys.stdout.write('x' * 5000000 + "
                "'\\ntest first ... FAILED\\n' + 'y' * 100000 + "
                "'\\nError: first failure detail\\n' + 'z' * 100000)"
            )
            driver = (
                "import sys; sys.path.insert(0, sys.argv[1]); from flake_check import run; "
                "from pathlib import Path; "
                f"run([sys.executable, '-c', {child!r}], Path(sys.argv[2]))"
            )
            result = subprocess.run(
                [sys.executable, "-c", driver, str(Path(__file__).parent), str(root / "log")],
                capture_output=True, check=False,
            )
            self.assertEqual(result.returncode, 0)
            self.assertIn(b"Error: first failure detail", (root / "log.failure.log").read_bytes())
            self.assertEqual((root / "log.tail.log").stat().st_size, 65536)

    def test_small_cargo_run_uses_exact_identities_despite_filter_collisions(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            child = (
                "print('Running unittests src/lib.rs (target/test)'); "
                "print('running 2 tests'); print('test first ... ok'); print('test second ... ok')"
            )
            driver = (
                "import sys; sys.path.insert(0, sys.argv[1]); import flake_check; "
                "from pathlib import Path; "
                "flake_check.IdentityFilter = type('Collisions', (), {'add': lambda self, value: True}); "
                "print(flake_check.run([sys.executable, '-c', sys.argv[4]], "
                "Path(sys.argv[2]), cargo_results=Path(sys.argv[3])))"
            )
            result = subprocess.run(
                [sys.executable, "-c", driver, str(Path(__file__).parent),
                 str(root / "log"), str(root / "results.json"), child],
                capture_output=True, text=True, timeout=10, check=False,
            )
            self.assertEqual(result.returncode, 0)
            self.assertTrue(result.stdout.endswith("0\n"))
            outcomes = json.loads((root / "results.json").read_text(encoding="utf-8"))
            self.assertEqual([record[1] for record in outcomes], ["first", "second"])

    def test_real_cargo_shared_process_outcomes_are_captured(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "src").mkdir()
            (root / "Cargo.toml").write_text(
                '[package]\nname = "cargo-fixture"\nversion = "0.1.0"\nedition = "2021"\n',
                encoding="utf-8",
            )
            (root / "src/lib.rs").write_text(
                '#[cfg(test)] mod tests {\n'
                '#[test] fn first() { assert_ne!(std::env::var("CASE_RUN").unwrap(), "1"); }\n'
                '#[test] fn second() { assert_ne!(std::env::var("CASE_RUN").unwrap(), "2"); }\n'
                '}\n', encoding="utf-8",
            )
            script_dir = str(Path(__file__).parent)
            driver = (
                "import os,sys; sys.path.insert(0, sys.argv[1]); from flake_check import run; "
                "from pathlib import Path; os.environ['CASE_RUN']=sys.argv[2]; "
                "run(['cargo','test','--offline','--lib','--color','never'], "
                "Path(sys.argv[3]), cargo_results=Path(sys.argv[4]))"
            )
            observed = []
            for number in (1, 2):
                log, results = root / f"{number}.log", root / f"{number}.json"
                subprocess.run(
                    [sys.executable, "-c", driver, script_dir, str(number), str(log), str(results)],
                    cwd=root, env={**os.environ, "CARGO_TARGET_DIR": str(root / "target")},
                    capture_output=True, check=True,
                )
                observed.append({name: status for _, name, status in json.loads(results.read_text())})
            self.assertEqual(observed, [
                {"tests::first": "failure", "tests::second": "passed"},
                {"tests::first": "passed", "tests::second": "failure"},
            ])

    def test_late_cargo_and_doctest_failures_survive_outcome_overflow(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            driver = (
                "import sys; sys.path.insert(0, sys.argv[1]); "
                "from flake_check import run, failure_keys; from pathlib import Path; "
                "previous = Path(sys.argv[3]); "
                "priority = failure_keys([previous]); "
                "result = run([sys.executable, '-c', sys.argv[4], sys.argv[5]], "
                "Path(sys.argv[2]), cargo_results=Path(sys.argv[6]), priority=priority); "
                "print('capture-result:', result)"
            )
            child = (
                "import sys; status = sys.argv[1]; "
                "print('x' * 5000000); "
                "print('     Running unittests src/lib.rs (target/debug/deps/example)'); "
                "print('running 40004 tests'); "
                "[print('test case_%05d ... ok' % n) for n in range(40002)]; "
                "print('test case_20001 ... FAILED'); "
                "print('test last ... ' + status); "
                "print('test result: FAILED. 40002 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out;'); "
                "print('   Doc-tests example'); print('running 1 test'); "
                "print('test src/lib.rs - last ... ' + status); "
                "print('test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out;')"
            )
            outcomes = []
            for number, state in ((1, "FAILED"), (2, "ok")):
                path = root / f"{number}.json"
                result = subprocess.run(
                    [sys.executable, "-c", driver, str(Path(__file__).parent),
                     str(root / f"{number}.log"), str(root / "1.json"), child, state, str(path)],
                    capture_output=True, text=True, check=True,
                )
                self.assertIn("capture-result: None", result.stdout)
                self.assertLessEqual(path.stat().st_size, 3 * 1024 * 1024 + 2)
                outcomes.append({(suite, name): status for suite, name, status in json.loads(path.read_text())})
            for key in (("Running unittests src/lib.rs (target/debug/deps/example)", "last"),
                        ("doctest:example", "src/lib.rs - last")):
                self.assertEqual([record[key] for record in outcomes], ["failure", "passed"])
            duplicate = ("Running unittests src/lib.rs (target/debug/deps/example)", "case_20001")
            self.assertTrue(all(duplicate not in record for record in outcomes))

    def test_captured_output_cannot_spoof_a_test_result(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            a, b = self.write_reports(
                Path(directory),
                report([("x", "<failure><system-out>test evil ... ok</system-out></failure>")]),
                report([("x", "")]),
            )
            self.assertEqual(outcomes(a), {("pkg::suite", "x"): "failure"})
            self.assertEqual(mixed_tests(a, b, {("pkg::suite", "x")}), [("pkg::suite", "x")])

    def test_second_run_does_not_hide_the_initial_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fake_rustc(root)
            cargo = root / "cargo"
            cargo.write_text(
                """#!/usr/bin/env python3
import os, sys, tomllib
from pathlib import Path
if sys.argv[1] == "build":
    assert sys.argv[sys.argv.index("--target") + 1] == "fake-host"
    raise SystemExit(0)
if sys.argv[1:3] == ["nextest", "list"]:
    helper = str(Path(os.environ["CARGO_TARGET_DIR"])
                 / "fake-host/debug/examples/native_owner_helper")
    assert os.environ["EJM_NATIVE_STARTUP_HELPER"] == helper
    assert os.environ["EJM_NATIVE_OCCUPANCY_HELPER"] == helper
    print('{"test-count": 1, "rust-suites": {"pkg::suite": {"binary-id": "pkg::suite",'
          ' "status": "listed", "testcases": {"write": {"ignored": false,'
          ' "filter-match": {"status": "matches"}}}}}}')
    raise SystemExit(0)
if sys.argv[1] == "nextest":
    marker = Path("ran")
    first = not marker.exists()
    marker.touch()
    config = Path(sys.argv[sys.argv.index("--config-file") + 1])
    output = Path(tomllib.loads(config.read_text())["profile"]["ci"]["junit"]["path"])
    state = "<failure/>" if first else ""
    count = "1" if first else "0"
    attrs = " tests='1' failures='" + count + "' errors='0' skipped='0'"
    output.write_text("<testsuites" + attrs + "><testsuite name='pkg::suite'" + attrs
                      + "><testcase classname='pkg::suite' name='write'>" + state
                      + "</testcase></testsuite></testsuites>")
    raise SystemExit(100 if first else 0)
raise SystemExit(0)
""",
                encoding="utf-8",
            )
            fake_doctests(cargo)
            cargo.chmod(0o755)
            env = {
                **os.environ,
                "PATH": str(root) + os.pathsep + os.environ.get("PATH", ""),
                "CARGO_TARGET_DIR": str(root / "custom-target"),
                "CARGO_BUILD_TARGET": "different-target",
            }
            script = Path(__file__).with_name("flake_check.py")
            result = subprocess.run(
                [sys.executable, str(script), str(root / "evidence")],
                cwd=root,
                env=env,
                capture_output=True,
                text=True,
            )
            summary = json.loads((root / "evidence/summary.json").read_text())
            self.assertEqual(result.returncode, 1)
            self.assertEqual(summary["cargo_test_exit_codes"], [0, 0])
            self.assertEqual(summary["exit_codes"], [100, 0])
            self.assertEqual(summary["doctest_exit_codes"], [0, 0])
            self.assertFalse(summary["incomplete"])
            self.assertEqual(summary["mixed"], [["pkg::suite", "write"]])
            env["FAKE_DOCTEST_STATUS"] = "ignored"
            result = subprocess.run(
                [sys.executable, str(script), str(root / "ignored-evidence")],
                cwd=root,
                env=env,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            ignored = json.loads((root / "ignored-evidence/summary.json").read_text())
            self.assertTrue(ignored["incomplete"])
            del env["FAKE_DOCTEST_STATUS"]
            env["FAKE_BASELINE_DOCTEST_STATUS"] = "ignored"
            result = subprocess.run(
                [sys.executable, str(script), str(root / "ignored-baseline-evidence")],
                cwd=root,
                env=env,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            ignored = json.loads((root / "ignored-baseline-evidence/summary.json").read_text())
            self.assertTrue(ignored["incomplete"])

    def test_cli_rejects_a_test_omitted_from_both_reports(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fake_rustc(root)
            cargo = root / "cargo"
            cargo.write_text(
                """#!/usr/bin/env python3
import json, sys, tomllib
from pathlib import Path
if sys.argv[1:3] == ["nextest", "list"]:
    case = {"ignored": False, "filter-match": {"status": "matches"}}
    print(json.dumps({"test-count": 2, "rust-suites": {"pkg::suite": {
        "binary-id": "pkg::suite", "status": "listed",
        "testcases": {"write": case, "missing": case}}}}))
elif sys.argv[1] == "nextest":
    config = Path(sys.argv[sys.argv.index("--config-file") + 1])
    output = Path(tomllib.loads(config.read_text())["profile"]["ci"]["junit"]["path"])
    marker = Path('attempted')
    failed = not marker.exists()
    marker.touch()
    attrs = " tests='1' failures='" + str(int(failed)) + "' errors='0' skipped='0'"
    output.write_text("<testsuites" + attrs + "><testsuite name='pkg::suite'" + attrs
                      + "><testcase classname='pkg::suite' name='write'>"
                      + ("<failure/>" if failed else "")
                      + "</testcase></testsuite></testsuites>")
    raise SystemExit(100 if failed else 0)
""",
                encoding="utf-8",
            )
            fake_doctests(cargo)
            cargo.chmod(0o755)
            env = {**os.environ, "PATH": str(root) + os.pathsep + os.environ.get("PATH", "")}
            result = subprocess.run(
                [
                    sys.executable,
                    str(Path(__file__).with_name("flake_check.py")),
                    str(root / "evidence"),
                ],
                cwd=root,
                env=env,
                capture_output=True,
                text=True,
            )
            summary = json.loads((root / "evidence/summary.json").read_text())
            self.assertNotEqual(result.returncode, 0)
            self.assertTrue(summary["incomplete"])
            self.assertEqual(summary["mixed"], [["pkg::suite", "write"]])

    def test_failed_cargo_gate_stays_failed_when_nextest_passes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fake_rustc(root)
            cargo = root / "cargo"
            cargo.write_text(
                """#!/usr/bin/env python3
import sys, tomllib
from pathlib import Path
if sys.argv[1] == "build":
    raise SystemExit(0)
if sys.argv[1:3] == ["nextest", "list"]:
    print('{"test-count": 1, "rust-suites": {"pkg::suite": {"binary-id": "pkg::suite",'
          ' "status": "listed", "testcases": {"write": {"ignored": false,'
          ' "filter-match": {"status": "matches"}}}}}}')
    raise SystemExit(0)
if sys.argv[1] == "nextest":
    config = Path(sys.argv[sys.argv.index("--config-file") + 1])
    output = Path(tomllib.loads(config.read_text())["profile"]["ci"]["junit"]["path"])
    attrs = " tests='1' failures='0' errors='0' skipped='0'"
    output.write_text("<testsuites" + attrs + "><testsuite name='pkg::suite'" + attrs
                      + "><testcase classname='pkg::suite' name='write'/></testsuite></testsuites>")
    raise SystemExit(0)
if "--doc" not in sys.argv:
    marker = Path("cargo-ran")
    if not marker.exists():
        marker.touch()
        print("shared-process test failed")
        raise SystemExit(101)
""",
                encoding="utf-8",
            )
            fake_doctests(cargo)
            cargo.chmod(0o755)
            env = {**os.environ, "PATH": str(root) + os.pathsep + os.environ.get("PATH", "")}
            result = subprocess.run(
                [
                    sys.executable,
                    str(Path(__file__).with_name("flake_check.py")),
                    str(root / "evidence"),
                ],
                cwd=root,
                env=env,
                capture_output=True,
                text=True,
            )
            summary = json.loads((root / "evidence/summary.json").read_text())
            self.assertEqual(result.returncode, 1)
            self.assertEqual(summary["cargo_test_exit_codes"], [101, 0])
            self.assertTrue(summary["cargo_test_inconsistent"])
            self.assertEqual(summary["exit_codes"], [0, 0])
            self.assertEqual(summary["mixed"], [])
            self.assertIn(
                "shared-process test failed", (root / "evidence/cargo-test-1.log").read_text()
            )

    def test_cli_compares_cargo_cases_when_both_runs_fail(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fake_rustc(root)
            cargo = root / "cargo"
            cargo.write_text(
                '''#!/usr/bin/env python3
import sys, tomllib
from pathlib import Path
if sys.argv[1:3] == ["nextest", "list"]:
    print('{"test-count":1,"rust-suites":{"pkg::suite":{"binary-id":"pkg::suite","status":"listed","testcases":{"x":{"ignored":false,"filter-match":{"status":"matches"}}}}}}')
    raise SystemExit(0)
if sys.argv[1] == "nextest":
    config = Path(sys.argv[sys.argv.index("--config-file") + 1])
    output = Path(tomllib.loads(config.read_text())["profile"]["ci"]["junit"]["path"])
    attrs = " tests='1' failures='0' errors='0' skipped='0'"
    output.write_text("<testsuites" + attrs + "><testsuite name='pkg::suite'" + attrs
                      + "><testcase classname='pkg::suite' name='x'/></testsuite></testsuites>")
    raise SystemExit(0)
if sys.argv[1] == "test" and "--doc" not in sys.argv:
    marker = Path("baseline-ran")
    first = not marker.exists()
    marker.touch()
    print("     Running unittests src/lib.rs (target/debug/deps/example-123)")
    print("running 2 tests")
    print("test tests::a ... " + ("FAILED" if first else "ok"))
    print("test tests::b ... " + ("ok" if first else "FAILED"))
    print("test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out")
    raise SystemExit(101)
''', encoding="utf-8",
            )
            fake_doctests(cargo)
            cargo.chmod(0o755)
            env = {**os.environ, "PATH": str(root) + os.pathsep + os.environ.get("PATH", "")}
            result = subprocess.run(
                [sys.executable, str(Path(__file__).with_name("flake_check.py")), str(root / "evidence")],
                cwd=root, env=env, capture_output=True, check=False,
            )
            summary = json.loads((root / "evidence/summary.json").read_text())
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(summary["cargo_test_exit_codes"], [101, 101])
            self.assertTrue(summary["cargo_test_inconsistent"])
            self.assertEqual(
                [name for suite, name in summary["mixed"] if suite.startswith("cargo:")],
                ["tests::a", "tests::b"],
            )

    def test_missing_reports_and_build_failures_are_incomplete(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fake_rustc(root)
            cargo = root / "cargo"
            cargo.write_text(
                "#!/bin/sh\n"
                'if [ "$1" = nextest ]; then echo "test fake ... ok"; exit 101; fi\n'
                "exit 0\n",
                encoding="utf-8",
            )
            cargo.chmod(0o755)
            env = {**os.environ, "PATH": str(root) + os.pathsep + os.environ.get("PATH", "")}
            result = subprocess.run(
                [
                    sys.executable,
                    str(Path(__file__).with_name("flake_check.py")),
                    str(root / "evidence"),
                ],
                cwd=root,
                env=env,
                capture_output=True,
                text=True,
            )
            summary = json.loads((root / "evidence/summary.json").read_text())
            self.assertNotEqual(result.returncode, 0)
            self.assertTrue(summary["incomplete"])
            self.assertEqual(summary["mixed"], [])

    def test_old_nextest_missing_skipped_counts_requires_no_skipped_cases(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            good = report([("x", "")]).replace(' skipped="0"', "")
            a, b = self.write_reports(root, good, good)
            self.assertEqual(mixed_tests(a, b, {("pkg::suite", "x")}), [])
            b.write_text(report([("x", "<skipped/>")]).replace(' skipped="1"', ""))
            with self.assertRaises(ValueError):
                mixed_tests(a, b, {("pkg::suite", "x")})
            b.write_text(report([("y", "")]).replace(' skipped="0"', ""))
            with self.assertRaises(ValueError):
                mixed_tests(a, b, {("pkg::suite", "x")})


if __name__ == "__main__":
    unittest.main()
