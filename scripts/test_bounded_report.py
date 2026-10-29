"""Exercise the bounded evidence producer boundary."""

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


class BoundedReportTests(unittest.TestCase):
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
