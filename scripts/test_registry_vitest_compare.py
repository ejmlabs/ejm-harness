"""Test registry report comparison with incomplete and mixed evidence."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from registry_vitest_compare import compare


class RegistryVitestCompareTests(unittest.TestCase):
    def test_failed_attempt_cannot_be_reported_as_complete(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            file = str(root / "worker.test.ts")
            listing = root / "inventory.json"
            listing.write_text(json.dumps([{"file": file, "name": "case"}]))
            reports = [root / "first.json", root / "second.json"]
            report = {
                "testResults": [
                    {"name": file, "assertionResults": [{"fullName": "case", "status": "passed"}]}
                ],
                "numTotalTests": 1,
                "numPassedTests": 1,
                "numFailedTests": 0,
                "numPendingTests": 0,
                "numTodoTests": 0,
                "success": True,
            }
            for path in reports:
                path.write_text(json.dumps(report))
            summary = root / "summary.json"
            result = subprocess.run(
                [
                    sys.executable,
                    "-I",
                    str(Path(__file__).with_name("registry_vitest_compare.py")),
                    str(listing),
                    *(str(path) for path in reports),
                    str(root),
                    str(summary),
                ],
                env={**os.environ, "FIRST_OUTCOME": "failure", "SECOND_OUTCOME": "success"},
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 1)
            self.assertTrue(json.loads(summary.read_text())["incomplete"])

    def test_incomplete_cases_do_not_hide_comparable_flakes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            file = str(root / "worker.test.ts")
            listing = root / "inventory.json"
            listing.write_text(
                json.dumps(
                    [
                        {"file": file, "name": "flip"},
                        {"file": file, "name": "skip"},
                        {"file": file, "name": "missing"},
                    ]
                )
            )
            reports = [root / "first.json", root / "second.json"]
            for path, state in zip(reports, ("failed", "passed"), strict=True):
                cases = [
                    {"fullName": "flip", "status": state},
                    {"fullName": "skip", "status": "skipped"},
                ]
                path.write_text(
                    json.dumps(
                        {
                            "testResults": [
                                {
                                    "name": file,
                                    "status": "failed" if state == "failed" else "passed",
                                    "assertionResults": cases,
                                }
                            ],
                            "numTotalTests": 2,
                            "numPassedTests": int(state == "passed"),
                            "numFailedTests": int(state == "failed"),
                            "numPendingTests": 1,
                            "numTodoTests": 0,
                            "success": state == "passed",
                        }
                    )
                )
            result = compare(listing, reports[0], reports[1], root)
            self.assertTrue(result["incomplete"])
            self.assertEqual(result["mixed"], ['["worker.test.ts","flip"]'])
            self.assertEqual(result["first_failed"], ['["worker.test.ts","flip"]'])

    def test_duplicate_failure_is_preserved_without_a_flake(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            file = str(root / "worker.test.ts")
            listing = root / "inventory.json"
            listing.write_text(json.dumps([{"file": file, "name": "case"}]))
            reports = [root / "first.json", root / "second.json"]
            for path, cases in zip(
                reports,
                (
                    [
                        {"fullName": "case", "status": "failed"},
                        {"fullName": "case", "status": "passed"},
                    ],
                    [{"fullName": "case", "status": "passed"}],
                ),
                strict=True,
            ):
                path.write_text(
                    json.dumps(
                        {
                            "testResults": [{"name": file, "assertionResults": cases}],
                            "numTotalTests": len(cases),
                            "numPassedTests": 1,
                            "numFailedTests": len(cases) - 1,
                            "numPendingTests": 0,
                            "numTodoTests": 0,
                            "success": len(cases) == 1,
                        }
                    )
                )
            result = compare(listing, reports[0], reports[1], root)
            self.assertTrue(result["incomplete"])
            self.assertEqual(result["mixed"], [])
            self.assertEqual(result["first_failed"], ['["worker.test.ts","case"]'])


if __name__ == "__main__":
    unittest.main()
