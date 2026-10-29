"""Exercise the bounded evidence producer boundary."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


class BoundedReportTests(unittest.TestCase):
    def test_xml_overflow_retains_late_failure_and_matching_pass(self) -> None:
        script = Path(__file__).with_name("bounded_report.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            first, second = root / "first.xml", root / "second.xml"
            for index, (output, failed) in enumerate(((first, True), (second, False))):
                cases = ''.join(f'<testcase classname="suite" name="case{i}"/>' for i in range(30))
                state = '<failure/>' if failed else ''
                report = ('<testsuites><testsuite name="suite">' + cases
                          + f'<testcase classname="suite" name="late">{state}</testcase>'
                          + '</testsuite></testsuites>')
                code = "from pathlib import Path; import sys; Path(sys.argv[1]).write_text(sys.argv[2])"
                env = dict(os.environ)
                if index:
                    env["EJM_HARNESS_JUNIT_REFERENCE"] = str(first)
                else:
                    env.pop("EJM_HARNESS_JUNIT_REFERENCE", None)
                result = subprocess.run(
                    [sys.executable, str(script), "fifo-xml", str(output), "400", "--",
                     sys.executable, "-c", code, str(output) + ".fifo", report],
                    env=env, capture_output=True, timeout=10, check=False,
                )
                self.assertEqual(result.returncode, 1)
                self.assertLessEqual(output.stat().st_size, 400)
            inventory, summary = root / "inventory.json", root / "summary.json"
            inventory.write_text("{}")
            result = subprocess.run(
                [sys.executable, "-I", str(script.with_name("registry_rust_compare.py")),
                 str(inventory), str(first), str(second), str(summary)],
                capture_output=True, timeout=10, check=False,
            )
            evidence = json.loads(summary.read_text())
            self.assertEqual(result.returncode, 1)
            self.assertTrue(evidence["incomplete"])
            self.assertIn(["suite", "late"], evidence["mixed"])

    def test_captures_and_bounds_fifo_and_stdout(self) -> None:
        script = Path(__file__).with_name("bounded_report.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for mode in ("fifo", "stdout"):
                for size in (32, 33):
                    output = root / f"{mode}-{size}"
                    producer = (
                        "from pathlib import Path; import sys; "
                        "data = b'x' * int(sys.argv[2]); "
                        "Path(sys.argv[1]).write_bytes(data) if sys.argv[1] != '-' "
                        "else sys.stdout.buffer.write(data)"
                    )
                    destination = str(output) + ".fifo" if mode == "fifo" else "-"
                    result = subprocess.run(
                        [sys.executable, str(script), mode, str(output), "32", "--",
                         sys.executable, "-c", producer, destination, str(size)],
                        capture_output=True, timeout=10, check=False,
                    )
                    self.assertEqual(result.returncode, int(size > 32))
                    self.assertEqual(output.read_bytes(), b"x" * min(size, 32))

    def test_missing_or_replaced_fifo_fails_without_deadlock(self) -> None:
        script = Path(__file__).with_name("bounded_report.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for replacement in (False, True):
                output = root / str(replacement)
                fifo = str(output) + ".fifo"
                code = (
                    "import os,sys; os.unlink(sys.argv[1]); "
                    "open(sys.argv[1], 'wb').write(b'x')"
                    if replacement else "pass"
                )
                result = subprocess.run(
                    [sys.executable, str(script), "fifo", str(output), "32", "--",
                     sys.executable, "-c", code, fifo],
                    capture_output=True, timeout=10, check=False,
                )
                self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
