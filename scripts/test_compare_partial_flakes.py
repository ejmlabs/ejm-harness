import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path


class PartialFlakeComparisonTests(unittest.TestCase):
    def test_failed_producer_marks_existing_summary_incomplete(self) -> None:
        script = Path(__file__).with_name("compare_partial_flakes.py")
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            summary = output / "summary.json"
            summary.write_text(json.dumps({"incomplete": False, "mixed": [["suite", "case"]]}))
            result = subprocess.run(
                [sys.executable, "-I", str(script), str(output)],
                env={**os.environ, "PRODUCER_OUTCOME": "failure"},
                capture_output=True, timeout=10, check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            value = json.loads(summary.read_text())
            self.assertTrue(value["incomplete"])
            self.assertEqual(value["mixed"], [["suite", "case"]])

    def test_interrupted_producer_preserves_comparable_outcomes(self) -> None:
        script = Path(__file__).with_name("compare_partial_flakes.py")
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            producer = '''import json, sys, time
from pathlib import Path
p = Path(sys.argv[1])
for number, state in ((1, "failure"), (2, "passed")):
    (p / f"cargo-test-{number}.json").write_text(json.dumps([["unit", "cargo", state], ["doctest:lib", "doc", state]]))
    (p / f"doctest-{number}.json").write_text(json.dumps([["doctest:lib", "doc", state]]))
    case = "<failure/>" if state == "failure" else ""
    (p / f"attempt-{number}.xml").write_text('<testsuites><testsuite name="suite"><testcase classname="suite" name="nextest">' + case + '</testcase></testsuite></testsuites>')
(p / "ready").touch()
time.sleep(60)
'''
            child = subprocess.Popen([sys.executable, "-c", producer, str(output)])
            try:
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline and not (output / "ready").exists():
                    self.assertIsNone(child.poll())
                    time.sleep(0.01)
                self.assertTrue((output / "ready").exists())
            finally:
                child.terminate()
                child.wait(timeout=10)
            result = subprocess.run([sys.executable, "-I", str(script), str(output)],
                                    capture_output=True, text=True, timeout=10, check=False)
            self.assertNotEqual(result.returncode, 0)
            summary = json.loads((output / "summary.json").read_text())
            self.assertTrue(summary["incomplete"])
            self.assertEqual(summary["mixed"], [["cargo:unit", "cargo"],
                                                 ["doctest:lib", "doc"], ["suite", "nextest"]])
            self.assertIn(["suite", "nextest"], summary["first_failed"])


if __name__ == "__main__":
    unittest.main()
