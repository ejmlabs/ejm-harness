import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


class RegistryUnittestEvidenceTests(unittest.TestCase):
    def test_oversized_evidence_keeps_comparable_outcomes(self) -> None:
        script = Path(__file__).with_name("registry_unittest_evidence.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "tests").mkdir()
            (root / "tests/test_many.py").write_text('''
import os
import unittest

class Case(unittest.TestCase):
    def __init__(self, identity):
        super().__init__("runTest")
        self.identity = identity
    def id(self):
        return self.identity
    def runTest(self):
        if self.identity.startswith("0000") and os.environ.get("FAIL_FLIP") == "1":
            self.fail("first failure")

def load_tests(loader, tests, pattern):
    return unittest.TestSuite(
        [Case("!" + "z" * 5000)] +
        [Case(f"{index:04d}" + "x" * 2000) for index in range(2400)] +
        [Case("zzzz-small")]
    )
''')
            reports = [root / "first.json", root / "second.json"]
            for report, fail in zip(reports, ("1", "0"), strict=True):
                result = subprocess.run(
                    [sys.executable, "-I", str(script), "run", str(report), "test_*.py"],
                    cwd=root, env={**os.environ, "FAIL_FLIP": fail},
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertLessEqual(report.stat().st_size, 4 * 1024 * 1024)
                data = json.loads(report.read_bytes())
                self.assertTrue(data["incomplete"])
                self.assertIn("0000" + "x" * 2000, data["outcomes"])
                self.assertIn("zzzz-small", data["outcomes"])
            compared = subprocess.run(
                [sys.executable, "-I", str(script), "compare", *(str(path) for path in reports)],
                capture_output=True, text=True, check=False,
            )
            self.assertNotEqual(compared.returncode, 0)
            self.assertIn("flaky-test: 0000", compared.stdout)


if __name__ == "__main__":
    unittest.main()
