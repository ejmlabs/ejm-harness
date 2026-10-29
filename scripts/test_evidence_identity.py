import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Any

from junit_stream import IdentityFilter
from registry_vitest_compare import compare


class EvidenceIdentityTests(unittest.TestCase):
    def test_filter_is_bounded_and_never_forgets_a_recorded_identity(self) -> None:
        identities = IdentityFilter()
        self.assertFalse(identities.add("recorded"))
        for number in range(40_000):
            identities.add(f"case-{number}")
        self.assertEqual(len(identities.bits), 256 * 1024)
        self.assertTrue(identities.add("recorded"))
        identities.bits[:] = b"\xff" * len(identities.bits)
        self.assertTrue(identities.add("unseen-collision"))

    def test_vitest_stream_keeps_failure_after_identity_limit(self) -> None:
        script = Path(__file__).with_name("bounded_report.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            inventory = root / "inventory.json"
            inventory.write_text("[]")
            first, second = root / "first.json", root / "second.json"
            for number, output in enumerate((first, second)):
                code = """import json, sys
from pathlib import Path
path, status = sys.argv[1:]
file = str(Path.cwd() / "suite.test.ts")
with open(path, "w") as stream:
    def write(value): stream.write(json.dumps(value, separators=(",", ":")) + "\\n")
    write({"kind": "start"})
    write({"kind": "case", "file": file, "name": "duplicate", "status": "passed"})
    write({"kind": "case", "file": file, "name": "duplicate", "status": "passed"})
    for index in range(20000):
        write({"kind": "case", "file": file, "name": f"case-{index}", "status": "passed"})
    write({"kind": "case", "file": file, "name": "late", "status": status,
           "message": "late diagnostic"})
    write({"kind": "end", "incomplete": False})
"""
                env = dict(os.environ)
                env["EJM_REGISTRY_VITEST_REFERENCE"] = str(first) if number else ""
                result = subprocess.run(
                    [
                        sys.executable,
                        str(script),
                        "fifo-jsonl",
                        str(output),
                        "4194304",
                        "--",
                        sys.executable,
                        "-c",
                        code,
                        str(output) + ".fifo",
                        "failed" if not number else "passed",
                    ],
                    cwd=root,
                    env=env,
                    capture_output=True,
                    timeout=30,
                    check=False,
                )
                self.assertEqual(result.returncode, 1, result.stderr.decode(errors="replace"))
                self.assertLessEqual(output.stat().st_size, 4 * 1024 * 1024)
            evidence = compare(inventory, first, second, root)
            self.assertTrue(evidence["incomplete"])
            self.assertEqual(evidence["mixed"], ['["suite.test.ts","late"]'])
            self.assertNotIn('["suite.test.ts","duplicate"]', evidence["mixed"])
            self.assertIn("late diagnostic", evidence["first_diagnostic"]["message"])

    def test_junit_stream_and_raw_reports_keep_late_failure(self) -> None:
        script = Path(__file__).with_name("bounded_report.py")
        compare_script = script.with_name("registry_rust_compare.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            inventory = root / "inventory.json"
            inventory.write_text("{}")
            for streaming in (False, True):
                first = root / f"first-{streaming}.xml"
                second = root / f"second-{streaming}.xml"
                for number, output in enumerate((first, second)):
                    code = """import sys
path, status = sys.argv[1:]
with open(path, "w") as stream:
    stream.write('<testsuites><testsuite name="suite">')
    for index in range(20000):
        stream.write(f'<testcase classname="suite" name="case-{index}"/>')
    stream.write('<testcase classname="suite" name="duplicate"/>')
    stream.write('<testcase classname="suite" name="duplicate"><failure/></testcase>')
    stream.write('<testcase classname="suite" name="late">'
                 + ('<failure/>' if status == 'failed' else '') + '</testcase>')
    stream.write('</testsuite></testsuites>')
"""
                    env = dict(os.environ)
                    env["EJM_HARNESS_JUNIT_REFERENCE"] = str(first) if number else ""
                    command = [
                        sys.executable,
                        "-c",
                        code,
                        str(output) + (".fifo" if streaming else ""),
                        "failed" if not number else "passed",
                    ]
                    if streaming:
                        command = [
                            sys.executable,
                            str(script),
                            "fifo-xml",
                            str(output),
                            "250000",
                            "--",
                            *command,
                        ]
                    result = subprocess.run(
                        command, cwd=root, env=env, capture_output=True, timeout=30, check=False
                    )
                    self.assertEqual(
                        result.returncode, int(streaming), result.stderr.decode(errors="replace")
                    )
                summary = root / f"summary-{streaming}.json"
                result = subprocess.run(
                    [
                        sys.executable,
                        "-I",
                        str(compare_script),
                        str(inventory),
                        str(first),
                        str(second),
                        str(summary),
                    ],
                    cwd=root,
                    capture_output=True,
                    timeout=30,
                    check=False,
                )
                evidence = json.loads(summary.read_text())
                self.assertEqual(result.returncode, 1)
                self.assertTrue(evidence["incomplete"])
                self.assertIn(["suite", "late"], evidence["mixed"])
                self.assertNotIn(["suite", "duplicate"], evidence["mixed"])

    def test_raw_vitest_report_keeps_late_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            inventory = root / "inventory.json"
            inventory.write_text("[]")
            reports = [root / "first.json", root / "second.json"]
            for number, report in enumerate(reports):
                cases: list[dict[str, Any]] = [
                    {"fullName": f"case-{index}", "status": "passed"} for index in range(20_000)
                ]
                cases.append(
                    {
                        "fullName": "late",
                        "status": "failed" if not number else "passed",
                        "failureMessages": ["late diagnostic"],
                    }
                )
                report.write_text(
                    json.dumps(
                        {
                            "testResults": [
                                {"name": str(root / "suite.test.ts"), "assertionResults": cases}
                            ]
                        }
                    )
                )
            result = compare(inventory, reports[0], reports[1], root)
            self.assertTrue(result["incomplete"])
            self.assertEqual(result["mixed"], ['["suite.test.ts","late"]'])
            self.assertIn("late diagnostic", result["first_diagnostic"]["message"])


if __name__ == "__main__":
    unittest.main()
