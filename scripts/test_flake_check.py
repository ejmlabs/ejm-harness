import json
import os
import subprocess
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Any

from flake_check import inventory, mixed_tests, outcomes


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


class FlakeCheckTests(unittest.TestCase):
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
