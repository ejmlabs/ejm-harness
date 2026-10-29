import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


class NextestEvidenceTests(unittest.TestCase):
    def test_pinned_nextest_reports_comparable_failures(self) -> None:
        root = Path(__file__).resolve().parents[1]
        version = subprocess.run(
            ["cargo", "nextest", "--version"], capture_output=True, text=True, check=True
        )
        self.assertIn("0.9.124", version.stdout)
        with tempfile.TemporaryDirectory(dir=root) as directory:
            fixture = Path(directory)
            (fixture / "src").mkdir()
            (fixture / "Cargo.toml").write_text(
                '[package]\nname = "nextest-evidence-fixture"\nversion = "0.1.0"\n'
                'edition = "2021"\n\n[workspace]\n', encoding="utf-8"
            )
            (fixture / "Cargo.lock").write_text(
                'version = 4\n\n[[package]]\nname = "nextest-evidence-fixture"\n'
                'version = "0.1.0"\n', encoding="utf-8"
            )
            (fixture / "src/lib.rs").write_text(
                '#[test] fn flip() { assert_ne!(std::env::var("FAIL_FLIP").as_deref(), Ok("1"), "first failure evidence"); }\n'
                '#[test] fn fixed_failure() { panic!("first failure evidence"); }\n',
                encoding="utf-8",
            )
            env = {**os.environ, "CARGO_BUILD_JOBS": "2", "CARGO_INCREMENTAL": "0"}
            for name in ("FIRST_OUTCOME", "SECOND_OUTCOME"):
                env.pop(name, None)
            inventory = fixture / "inventory.json"
            listing = subprocess.run(
                ["cargo", "nextest", "list", "--locked", "--ignore-default-filter",
                 "--message-format", "json", "--user-config-file", "none"],
                cwd=fixture, env=env, capture_output=True, timeout=120, check=False,
            )
            self.assertEqual(listing.returncode, 0, listing.stderr.decode(errors="replace"))
            inventory.write_bytes(listing.stdout)
            reports = [fixture / "first.xml", fixture / "second.xml"]
            for report, fail in zip(reports, ("1", "0"), strict=True):
                config = fixture / (report.stem + ".toml")
                config.write_text(
                    '[profile.ci.junit]\npath = ' + json.dumps(str(report) + ".fifo")
                    + '\nstore-failure-output = true\n', encoding="utf-8",
                )
                result = subprocess.run(
                    [sys.executable, "-I", str(root / "scripts/bounded_report.py"),
                     "fifo-xml", str(report), "1048576", "--", "cargo", "nextest", "run",
                     "--locked", "--ignore-default-filter", "--no-fail-fast", "--retries", "0",
                     "--profile", "ci", "--config-file", str(config),
                     "--user-config-file", "none", "--no-tests", "fail"],
                    cwd=fixture, env={**env, "FAIL_FLIP": fail}, capture_output=True,
                    timeout=120, check=False,
                )
                self.assertEqual(result.returncode, 100, result.stderr.decode(errors="replace"))
            summary = fixture / "summary.json"
            result = subprocess.run(
                [sys.executable, "-I", str(root / "scripts/registry_rust_compare.py"),
                 str(inventory), *(str(report) for report in reports), str(summary)],
                cwd=fixture, env=env, capture_output=True, timeout=30, check=False,
            )
            self.assertEqual(result.returncode, 1)
            evidence = json.loads(summary.read_text(encoding="utf-8"))
            self.assertFalse(evidence["incomplete"])
            self.assertTrue(any(case[1] == "flip" for case in evidence["mixed"]))
            self.assertIn("first failure evidence", evidence["first_diagnostic"]["message"])

    def test_pinned_nextest_retains_late_failure_after_overflow(self) -> None:
        root = Path(__file__).resolve().parents[1]
        version = subprocess.run(
            ["cargo", "nextest", "--version"], capture_output=True, text=True, check=True
        )
        self.assertIn("0.9.124", version.stdout)
        with tempfile.TemporaryDirectory(dir=root) as directory:
            fixture = Path(directory)
            (fixture / "src").mkdir()
            (fixture / "Cargo.toml").write_text(
                '[package]\nname = "nextest-overflow-fixture"\nversion = "0.1.0"\n'
                'edition = "2021"\n\n[workspace]\n', encoding="utf-8"
            )
            (fixture / "Cargo.lock").write_text(
                'version = 4\n\n[[package]]\nname = "nextest-overflow-fixture"\n'
                'version = "0.1.0"\n', encoding="utf-8"
            )
            tests = ''.join(f'#[test] fn case_{index:04d}_'+ 'x' * 80 + '() {}\n' for index in range(100))
            (fixture / "src/lib.rs").write_text(
                tests + '#[test] fn zz_flip() { assert_ne!(std::env::var("FAIL_FLIP").as_deref(), Ok("1"), "late failure evidence"); }\n',
                encoding="utf-8",
            )
            env = {**os.environ, "CARGO_BUILD_JOBS": "2", "CARGO_INCREMENTAL": "0"}
            for name in ("FIRST_OUTCOME", "SECOND_OUTCOME", "EJM_HARNESS_JUNIT_REFERENCE"):
                env.pop(name, None)
            listing = subprocess.run(
                ["cargo", "nextest", "list", "--locked", "--ignore-default-filter",
                 "--message-format", "json", "--user-config-file", "none"],
                cwd=fixture, env=env, capture_output=True, timeout=120, check=False,
            )
            self.assertEqual(listing.returncode, 0, listing.stderr.decode(errors="replace"))
            inventory = fixture / "inventory.json"
            inventory.write_bytes(listing.stdout)
            reports = [fixture / "first.xml", fixture / "second.xml"]
            for index, (report, fail) in enumerate(zip(reports, ("1", "0"), strict=True)):
                config = fixture / (report.stem + ".toml")
                config.write_text(
                    '[profile.ci.junit]\npath = ' + json.dumps(str(report) + ".fifo")
                    + '\nstore-failure-output = true\n', encoding="utf-8",
                )
                attempt_env = {**env, "FAIL_FLIP": fail}
                if index:
                    attempt_env["EJM_HARNESS_JUNIT_REFERENCE"] = str(reports[0])
                result = subprocess.run(
                    [sys.executable, "-I", str(root / "scripts/bounded_report.py"),
                     "fifo-xml", str(report), "2048", "--", "cargo", "nextest", "run",
                     "--locked", "--ignore-default-filter", "--no-fail-fast", "--retries", "0",
                     "--profile", "ci", "--config-file", str(config),
                     "--user-config-file", "none", "--no-tests", "fail"],
                    cwd=fixture, env=attempt_env, capture_output=True, timeout=120, check=False,
                )
                self.assertEqual(result.returncode, 1, result.stderr.decode(errors="replace"))
                self.assertLessEqual(report.stat().st_size, 2048)
            summary = fixture / "summary.json"
            result = subprocess.run(
                [sys.executable, "-I", str(root / "scripts/registry_rust_compare.py"),
                 str(inventory), *(str(report) for report in reports), str(summary)],
                cwd=fixture, env=env, capture_output=True, timeout=30, check=False,
            )
            evidence = json.loads(summary.read_text(encoding="utf-8"))
            self.assertEqual(result.returncode, 1)
            self.assertTrue(evidence["incomplete"])
            self.assertTrue(any(case[1] == "zz_flip" for case in evidence["mixed"]))
            self.assertIn("late failure evidence", evidence["first_diagnostic"]["message"])


if __name__ == "__main__":
    unittest.main()
