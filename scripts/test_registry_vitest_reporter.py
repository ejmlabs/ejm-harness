import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from registry_vitest_compare import compare


class RegistryVitestReporterTests(unittest.TestCase):
    def test_real_vitest_keeps_completed_failure_before_overflow(self) -> None:
        root = Path(__file__).resolve().parents[1]
        registry = root / "registry"
        with tempfile.TemporaryDirectory(dir=registry) as directory:
            fixture = Path(directory)
            test_file = fixture / "fixture.test.mjs"
            test_file.write_text('''
import { it } from "vitest";
it("flip", () => { if (process.env.FAIL_FLIP === "1") throw new Error("first failure"); });
it("oversized", () => { throw new Error("x".repeat(10000)); });
''')
            config = fixture / "vitest.config.mjs"
            config.write_text('export default { test: { include: ["*.test.mjs"] } };\n')
            inventory = fixture / "inventory.json"
            inventory.write_text(json.dumps([{"file": str(test_file), "name": "flip"}]))
            reports = [fixture / "first.json", fixture / "second.json"]
            for report, fail in zip(reports, ("1", "0"), strict=True):
                result = subprocess.run(
                    [sys.executable, "-I", str(root / "scripts/bounded_report.py"),
                     "fifo", str(report), "2048", "--", "node",
                     str(registry / "node_modules/vitest/vitest.mjs"), "run", "--config",
                     str(config), "--retry=0", "--reporter=" + str(root / "scripts/registry_vitest_reporter.mjs")],
                    cwd=fixture, env={**os.environ, "CI": "1", "FAIL_FLIP": fail,
                                      "EJM_REGISTRY_VITEST_EVIDENCE": str(report) + ".fifo"},
                    capture_output=True, text=True, timeout=60, check=False,
                )
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertLessEqual(report.stat().st_size, 2048)
            result = compare(inventory, *reports, fixture)
            self.assertTrue(result["incomplete"])
            self.assertEqual(result["mixed"], ['["fixture.test.mjs","flip"]'])
            self.assertIn("first failure", result["first_diagnostic"]["message"])


if __name__ == "__main__":
    unittest.main()
