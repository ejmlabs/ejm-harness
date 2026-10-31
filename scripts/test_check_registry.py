import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Any

ROOT = Path(__file__).absolute().parent.parent


class RegistryGateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        bin_dir = self.directory / "bin"
        bin_dir.mkdir()
        stub = f"""#!{sys.executable}
import json, os, sys
from pathlib import Path
program = Path(sys.argv[0]).name
args = sys.argv[1:]
if program == 'rustup':
    print(str(Path(sys.argv[0]).parent / 'rustc'))
    raise SystemExit(0)
with open(os.environ['TRACE_PATH'], 'a', encoding='utf-8') as stream:
    stream.write(json.dumps({{'program': program, 'args': args, 'cwd': os.getcwd()}}) + '\\n')
failure = os.environ.get('FAIL_STEP')
if program == 'cargo' and args == [
    'test', '--manifest-path', 'registry/Cargo.toml', '--lib', '--locked'
]:
    print('registry cargo test output')
    if failure == 'cargo':
        raise SystemExit(25)
if failure == 'schema' and (program == 'python3' or
                            args == ['--prefix', 'registry', 'run', 'test:schema']):
    raise SystemExit(23)
if failure == 'vitest' and program == 'npm' and args[:3] == ['--prefix', 'registry', 'test']:
    raise SystemExit(24)
"""
        for name in ("rustup", "node", "cargo", "npm", "python3"):
            executable = bin_dir / name
            executable.write_text(stub, encoding="utf-8")
            executable.chmod(0o755)
        self.trace = self.directory / "trace.jsonl"
        self.env = {**os.environ, "PATH": f"{bin_dir}{os.pathsep}{os.environ.get('PATH', '')}"}
        self.env["TRACE_PATH"] = str(self.trace)
        for name in (
            "EJM_REGISTRY_SCHEMA_REPORT",
            "EJM_UNITTEST_RUNNER",
            "EJM_REGISTRY_VITEST_REPORT",
            "EJM_REGISTRY_CARGO_LOG",
        ):
            self.env.pop(name, None)

    def run_gate(self, **env: str) -> tuple[int, list[dict[str, Any]]]:
        self.trace.unlink(missing_ok=True)
        result = subprocess.run(
            ["bash", "scripts/check-registry"],
            cwd=ROOT,
            env={**self.env, **env},
            capture_output=True,
            text=True,
            check=False,
        )
        self.output = result.stdout
        calls = [json.loads(line) for line in self.trace.read_text(encoding="utf-8").splitlines()]
        return result.returncode, calls

    def test_cargo_log_preserves_output_and_command(self) -> None:
        cargo_args = ["test", "--manifest-path", "registry/Cargo.toml", "--lib", "--locked"]
        log = self.directory / "cargo test log.txt"
        for options in (
            {},
            {"EJM_REGISTRY_CARGO_LOG": ""},
            {"EJM_REGISTRY_CARGO_LOG": str(log)},
        ):
            with self.subTest(options=options):
                status, calls = self.run_gate(**options)
                self.assertEqual(status, 0)
                self.assertEqual(
                    sum(
                        call["program"] == "cargo" and call["args"] == cargo_args for call in calls
                    ),
                    1,
                )
                self.assertIn("registry cargo test output\n", self.output)
                if options.get("EJM_REGISTRY_CARGO_LOG"):
                    self.assertEqual(
                        log.read_text(encoding="utf-8"), "registry cargo test output\n"
                    )
                else:
                    self.assertFalse(log.exists())

    def test_cargo_and_log_failures_stop_the_gate(self) -> None:
        cargo_args = ["test", "--manifest-path", "registry/Cargo.toml", "--lib", "--locked"]
        log = self.directory / "failed cargo log.txt"
        for options in ({}, {"EJM_REGISTRY_CARGO_LOG": str(log)}):
            with self.subTest(options=options):
                status, calls = self.run_gate(**options, FAIL_STEP="cargo")
                self.assertEqual(status, 25)
                self.assertEqual(calls[-1]["args"], cargo_args)
                self.assertIn("registry cargo test output\n", self.output)
                if options:
                    self.assertEqual(
                        log.read_text(encoding="utf-8"), "registry cargo test output\n"
                    )
        status, calls = self.run_gate(EJM_REGISTRY_CARGO_LOG=str(self.directory))
        self.assertNotEqual(status, 0)
        self.assertEqual(calls[-1]["args"], cargo_args)

    def test_default_and_partial_schema_options_use_existing_commands(self) -> None:
        for env in (
            {},
            {"EJM_REGISTRY_SCHEMA_REPORT": str(self.directory / "schema.json")},
            {"EJM_UNITTEST_RUNNER": str(self.directory / "runner.py")},
        ):
            with self.subTest(env=env):
                status, calls = self.run_gate(**env)
                self.assertEqual(status, 0)
                self.assertIn(
                    {
                        "program": "npm",
                        "args": ["--prefix", "registry", "run", "test:schema"],
                        "cwd": str(ROOT),
                    },
                    calls,
                )
                self.assertIn(
                    {
                        "program": "npm",
                        "args": ["--prefix", "registry", "test"],
                        "cwd": str(ROOT),
                    },
                    calls,
                )
                self.assertFalse(any(call["program"] == "python3" for call in calls))

    def test_report_options_preserve_paths_and_literal_pattern(self) -> None:
        runner = str(self.directory / "runner path.py")
        schema = str(self.directory / "schema report.json")
        vitest = str(self.directory / "vitest report.json")
        status, calls = self.run_gate(
            EJM_UNITTEST_RUNNER=runner,
            EJM_REGISTRY_SCHEMA_REPORT=schema,
            EJM_REGISTRY_VITEST_REPORT=vitest,
        )
        self.assertEqual(status, 0)
        self.assertIn(
            {
                "program": "python3",
                "args": ["-I", runner, "run", schema, "test_*.py"],
                "cwd": str(ROOT / "registry"),
            },
            calls,
        )
        self.assertIn(
            {
                "program": "npm",
                "args": [
                    "--prefix",
                    "registry",
                    "test",
                    "--",
                    "--reporter=default",
                    "--reporter=json",
                    "--outputFile.json",
                    vitest,
                ],
                "cwd": str(ROOT),
            },
            calls,
        )
        self.assertFalse(
            any(call["args"] == ["--prefix", "registry", "run", "test:schema"] for call in calls)
        )

    def test_report_options_are_independent(self) -> None:
        runner = str(self.directory / "runner.py")
        schema = str(self.directory / "schema.json")
        vitest = str(self.directory / "vitest.json")
        status, calls = self.run_gate(EJM_UNITTEST_RUNNER=runner, EJM_REGISTRY_SCHEMA_REPORT=schema)
        self.assertEqual(status, 0)
        self.assertTrue(any(call["program"] == "python3" for call in calls))
        self.assertEqual(calls[-1]["args"], ["--prefix", "registry", "test"])
        status, calls = self.run_gate(EJM_REGISTRY_VITEST_REPORT=vitest)
        self.assertEqual(status, 0)
        self.assertFalse(any(call["program"] == "python3" for call in calls))
        self.assertTrue(
            any(call["args"] == ["--prefix", "registry", "run", "test:schema"] for call in calls)
        )
        self.assertEqual(calls[-1]["args"][-2:], ["--outputFile.json", vitest])

    def test_failures_propagate_in_default_and_report_modes(self) -> None:
        reports = {
            "EJM_UNITTEST_RUNNER": str(self.directory / "runner.py"),
            "EJM_REGISTRY_SCHEMA_REPORT": str(self.directory / "schema.json"),
            "EJM_REGISTRY_VITEST_REPORT": str(self.directory / "vitest.json"),
        }
        for options in ({}, reports):
            for step, expected in (("schema", 23), ("vitest", 24)):
                with self.subTest(options=options, step=step):
                    status, calls = self.run_gate(**options, FAIL_STEP=step)
                    self.assertEqual(status, expected)
                    if step == "schema":
                        self.assertFalse(
                            any(
                                call["args"][:3] == ["--prefix", "registry", "test"]
                                for call in calls
                            )
                        )


if __name__ == "__main__":
    unittest.main()
