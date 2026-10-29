import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path


class RegistryUnittestEvidenceTests(unittest.TestCase):
    def test_first_fixture_error_diagnostic_survives_later_hang(self) -> None:
        script = Path(__file__).with_name("registry_unittest_evidence.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "tests").mkdir()
            (root / "tests/test_fixture_error.py").write_text('''
import os
import time
import unittest
from pathlib import Path

class First(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        raise RuntimeError("FIXTURE_SENTINEL")

    def test_unrun(self):
        pass

class Later(unittest.TestCase):
    def test_hang(self):
        Path(os.environ["HANG_STARTED"]).write_text("started")
        while True:
            time.sleep(1)
''', encoding="utf-8")
            report, started = root / "fixture.json", root / "fixture.started"
            child = subprocess.Popen(
                [sys.executable, "-I", str(script), "run", str(report), "test_*.py"],
                cwd=root, env={**os.environ, "HANG_STARTED": str(started)},
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
            try:
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline and not started.exists():
                    self.assertIsNone(child.poll(), "schema runner exited before hanging")
                    time.sleep(0.01)
                self.assertTrue(started.exists(), "fixture did not reach the later test")
                self.assertIsNone(child.poll())
                evidence = json.loads(report.read_text(encoding="utf-8"))
                self.assertTrue(evidence["incomplete"])
                self.assertIn("test_fixture_error.First.test_unrun", evidence["ineligible"])
                self.assertIn("setUpClass", evidence["first_diagnostic"]["test"])
                self.assertIn("FIXTURE_SENTINEL", evidence["first_diagnostic"]["message"])
                self.assertLessEqual(report.stat().st_size, 4 * 1024 * 1024)
            finally:
                child.terminate()
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=5)

    def test_first_diagnostic_survives_hang_for_failure_error_and_subtest(self) -> None:
        script = Path(__file__).with_name("registry_unittest_evidence.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "tests").mkdir()
            (root / "tests/test_diagnostic.py").write_text('''
import os
import time
import unittest
from pathlib import Path

class Case(unittest.TestCase):
    def test_a_failure(self):
        mode = os.environ["FAILURE_MODE"]
        message = "FIRST_SENTINEL " + "x" * 12000 + " END_SENTINEL"
        if mode == "error":
            raise RuntimeError(message)
        if mode == "failure":
            self.fail(message)
        with self.subTest(value=1):
            self.fail(message)
        Path(os.environ["HANG_STARTED"]).write_text("started")
        while True:
            time.sleep(1)

    def test_b_hang(self):
        with self.subTest(value=2):
            self.fail("SECOND_SENTINEL")
        Path(os.environ["HANG_STARTED"]).write_text("started")
        while True:
            time.sleep(1)
''', encoding="utf-8")
            for mode in ("failure", "error", "subtest"):
                report, started = root / f"{mode}.json", root / f"{mode}.started"
                child = subprocess.Popen(
                    [sys.executable, "-I", str(script), "run", str(report), "test_*.py"],
                    cwd=root,
                    env={**os.environ, "FAILURE_MODE": mode, "HANG_STARTED": str(started)},
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                )
                try:
                    deadline = time.monotonic() + 10
                    while time.monotonic() < deadline and not started.exists():
                        self.assertIsNone(child.poll(), f"{mode} runner exited before hanging")
                        time.sleep(0.01)
                    self.assertTrue(started.exists(), f"{mode} fixture did not hang")
                    self.assertIsNone(child.poll())
                    evidence = json.loads(report.read_text(encoding="utf-8"))
                    self.assertTrue(evidence["incomplete"])
                    diagnostic = evidence["first_diagnostic"]
                    self.assertTrue(diagnostic["test"].startswith("test_diagnostic.Case.test_a_failure"))
                    self.assertIn("FIRST_SENTINEL", diagnostic["message"])
                    self.assertNotIn("SECOND_SENTINEL", diagnostic["message"])
                    self.assertTrue(diagnostic["truncated"])
                    self.assertLessEqual(report.stat().st_size, 4 * 1024 * 1024)
                    if mode == "subtest":
                        self.assertNotIn("test_diagnostic.Case.test_a_failure", evidence["outcomes"])
                    else:
                        self.assertEqual(evidence["outcomes"]["test_diagnostic.Case.test_a_failure"], "failed")
                finally:
                    child.terminate()
                    try:
                        child.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        child.kill()
                        child.wait(timeout=5)

    def test_unfinished_parent_with_failed_subtest_is_not_comparable(self) -> None:
        script = Path(__file__).with_name("registry_unittest_evidence.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "tests").mkdir()
            (root / "tests/test_subtest.py").write_text('''
import os
import time
import unittest
from pathlib import Path

class Case(unittest.TestCase):
    def test_active(self):
        if os.environ["FAIL_SUBTEST"] == "1":
            with self.subTest(value=1):
                self.fail("unfinished subtest")
            Path(os.environ["HANG_STARTED"]).write_text("started")
            while True:
                time.sleep(1)
''', encoding="utf-8")
            first, second = root / "first.json", root / "second.json"
            started = root / "started"
            env = {**os.environ, "FAIL_SUBTEST": "1", "HANG_STARTED": str(started)}
            child = subprocess.Popen(
                [sys.executable, "-I", str(script), "run", str(first), "test_*.py"],
                cwd=root, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
            try:
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline and not started.exists():
                    self.assertIsNone(child.poll(), "schema runner exited before hanging")
                    time.sleep(0.01)
                self.assertTrue(started.exists(), "subtest fixture did not hang")
                evidence = json.loads(first.read_text(encoding="utf-8"))
                self.assertTrue(evidence["incomplete"])
                self.assertNotIn("test_subtest.Case.test_active", evidence["outcomes"])
            finally:
                child.terminate()
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=5)
            completed = subprocess.run(
                [sys.executable, "-I", str(script), "run", str(second), "test_*.py"],
                cwd=root, env={**env, "FAIL_SUBTEST": "0"},
                capture_output=True, timeout=10, check=False,
            )
            self.assertEqual(completed.returncode, 0)
            compared = subprocess.run(
                [sys.executable, "-I", str(script), "compare", str(first), str(second)],
                capture_output=True, text=True, timeout=10, check=False,
            )
            self.assertNotEqual(compared.returncode, 0)
            self.assertNotIn("flaky-test:", compared.stdout)

    def test_teardown_failure_invalidates_completed_case_before_hang(self) -> None:
        script = Path(__file__).with_name("registry_unittest_evidence.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "tests").mkdir()
            (root / "tests/test_fixture.py").write_text('''
import os
import time
import unittest
from pathlib import Path

class First(unittest.TestCase):
    def test_flip(self):
        if os.environ["FAIL_FLIP"] == "1":
            self.fail("first failure")

    @classmethod
    def tearDownClass(cls):
        raise RuntimeError("fixture teardown failed")

class Later(unittest.TestCase):
    def test_wait(self):
        if os.environ["HANG"] == "1":
            Path(os.environ["HANG_STARTED"]).write_text("started")
            while True:
                time.sleep(1)
''', encoding="utf-8")
            first, second = root / "first.json", root / "second.json"
            started = root / "started"
            env = {**os.environ, "FAIL_FLIP": "1", "HANG": "1", "HANG_STARTED": str(started)}
            child = subprocess.Popen(
                [sys.executable, "-I", str(script), "run", str(first), "test_*.py"],
                cwd=root, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
            try:
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline and not started.exists():
                    self.assertIsNone(child.poll(), "schema runner exited before hanging")
                    time.sleep(0.01)
                self.assertTrue(started.exists(), "fixture did not reach the later test")
                evidence = json.loads(first.read_text(encoding="utf-8"))
                self.assertTrue(evidence["incomplete"])
                self.assertEqual(evidence["outcomes"]["test_fixture.First.test_flip"], "failed")
                self.assertIn("test_fixture.First.test_flip", evidence["ineligible"])
            finally:
                child.terminate()
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=5)
            completed = subprocess.run(
                [sys.executable, "-I", str(script), "run", str(second), "test_*.py"],
                cwd=root, env={**env, "FAIL_FLIP": "0", "HANG": "0"},
                capture_output=True, timeout=10, check=False,
            )
            self.assertNotEqual(completed.returncode, 0)
            compared = subprocess.run(
                [sys.executable, "-I", str(script), "compare", str(first), str(second)],
                capture_output=True, text=True, timeout=10, check=False,
            )
            self.assertNotEqual(compared.returncode, 0)
            self.assertNotIn("flaky-test:", compared.stdout)

    def test_partial_results_survive_a_later_hanging_test(self) -> None:
        script = Path(__file__).with_name("registry_unittest_evidence.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "tests").mkdir()
            (root / "tests/test_partial.py").write_text('''
import os
import time
import unittest
from pathlib import Path

class Case(unittest.TestCase):
    def test_a_flip(self):
        if os.environ["FAIL_FLIP"] == "1":
            self.fail("first failure")

    def test_b_hang(self):
        Path(os.environ["HANG_STARTED"]).write_text("started")
        while True:
            time.sleep(1)
''', encoding="utf-8")
            reports = [root / "first.json", root / "second.json"]
            for number, report in enumerate(reports):
                started = root / f"started-{number}"
                env = {
                    **os.environ,
                    "FAIL_FLIP": "1" if number == 0 else "0",
                    "HANG_STARTED": str(started),
                    "EJM_REGISTRY_SCHEMA_REFERENCE": str(reports[0]) if number else "",
                }
                child = subprocess.Popen(
                    [sys.executable, "-I", str(script), "run", str(report), "test_*.py"],
                    cwd=root, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                )
                try:
                    deadline = time.monotonic() + 10
                    evidence = None
                    while time.monotonic() < deadline:
                        if started.exists() and report.is_file():
                            try:
                                candidate = json.loads(report.read_text(encoding="utf-8"))
                            except (OSError, ValueError):
                                candidate = None
                            if candidate and "test_partial.Case.test_a_flip" in candidate["outcomes"]:
                                evidence = candidate
                                break
                        self.assertIsNone(child.poll(), "schema runner exited before hanging")
                        time.sleep(0.01)
                    self.assertIsNotNone(evidence, "completed test evidence was not persisted")
                    self.assertIsNone(child.poll())
                    self.assertTrue(evidence["incomplete"])
                    self.assertEqual(
                        evidence["outcomes"]["test_partial.Case.test_a_flip"],
                        "failed" if number == 0 else "passed",
                    )
                    self.assertLessEqual(report.stat().st_size, 4 * 1024 * 1024)
                finally:
                    child.terminate()
                    try:
                        child.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        child.kill()
                        child.wait(timeout=5)
            compared = subprocess.run(
                [sys.executable, "-I", str(script), "compare", *(str(path) for path in reports)],
                capture_output=True, text=True, timeout=10, check=False,
            )
            self.assertNotEqual(compared.returncode, 0)
            self.assertIn("flaky-test: test_partial.Case.test_a_flip", compared.stdout)

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
        if (self.identity.startswith("0000") or self.identity == "zzzz-small") and os.environ.get("FAIL_FLIP") == "1":
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
                    cwd=root, env={**os.environ, "FAIL_FLIP": fail,
                                   "EJM_REGISTRY_SCHEMA_REFERENCE": str(reports[0]) if fail == "0" else ""},
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
            self.assertIn("flaky-test: zzzz-small", compared.stdout)


if __name__ == "__main__":
    unittest.main()
