"""Exercise the bounded evidence producer boundary."""

import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path
from unittest.mock import patch


class BoundedReportTests(unittest.TestCase):
    def test_junit_diagnostic_fits_small_bounds_and_retains_tail(self) -> None:
        from junit_stream import JunitRecords

        report = (
            b'<testsuites><testsuite name="suite"><testcase classname="suite" name="case">'
            + b"<failure>"
            + b"x" * 20000
            + b"TAIL</failure></testcase></testsuite></testsuites>"
        )
        for limit in (64, 128, 512):
            collector = JunitRecords(limit, None)
            collector.add(report)
            output = collector.finish()
            self.assertLessEqual(len(output), limit)
            root = ET.fromstring(output)
            self.assertEqual(root.get("incomplete"), "true")
            if limit >= 128:
                self.assertIn("TAIL", root.findtext("first-diagnostic", ""))

    def test_ambiguous_junit_failure_keeps_diagnostic_without_becoming_mixed(self) -> None:
        script = Path(__file__).with_name("bounded_report.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for kind in ("failure", "error"):
                for failure_first in (False, True):
                    for overflow in (False, True):
                        first, second = root / "first.xml", root / "second.xml"
                        failed = (
                            f'<testcase classname="suite" name="duplicate">'
                            f'<{kind} message="duplicate sentinel">first detail</{kind}></testcase>'
                        )
                        passed = '<testcase classname="suite" name="duplicate"/>'
                        duplicate = failed + passed if failure_first else passed + failed
                        padding = " " * 8192 if overflow else ""
                        first_xml = (
                            f'<testsuites><testsuite name="suite">{padding}{duplicate}'
                            '<testcase classname="suite" name="stable"><failure/></testcase>'
                            "</testsuite></testsuites>"
                        )
                        second_xml = (
                            '<testsuites><testsuite name="suite">'
                            '<testcase classname="suite" name="duplicate"/>'
                            '<testcase classname="suite" name="stable"/>'
                            "</testsuite></testsuites>"
                        )
                        for output, report in ((first, first_xml), (second, second_xml)):
                            code = (
                                "from pathlib import Path; import sys; "
                                "Path(sys.argv[1]).write_text(sys.argv[2])"
                            )
                            subprocess.run(
                                [
                                    sys.executable,
                                    str(script),
                                    "fifo-xml",
                                    str(output),
                                    "4096",
                                    "--",
                                    sys.executable,
                                    "-c",
                                    code,
                                    str(output) + ".fifo",
                                    report,
                                ],
                                capture_output=True,
                                timeout=10,
                                check=False,
                            )
                            self.assertLessEqual(output.stat().st_size, 4096)
                        inventory, summary = root / "inventory.json", root / "summary.json"
                        inventory.write_text("{}")
                        result = subprocess.run(
                            [
                                sys.executable,
                                "-I",
                                str(script.with_name("registry_rust_compare.py")),
                                str(inventory),
                                str(first),
                                str(second),
                                str(summary),
                            ],
                            capture_output=True,
                            timeout=10,
                            check=False,
                        )
                        evidence = json.loads(summary.read_text())
                        self.assertNotEqual(result.returncode, 0)
                        self.assertTrue(evidence["incomplete"])
                        self.assertNotIn(["suite", "duplicate"], evidence["mixed"])
                        self.assertIn(["suite", "stable"], evidence["mixed"])
                        self.assertIn("duplicate sentinel", evidence["first_diagnostic"]["message"])

    def test_interrupted_snapshot_retains_last_complete_evidence(self) -> None:
        script = Path(__file__).with_name("bounded_report.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for mode in ("fifo-jsonl", "fifo-xml"):
                output = root / mode
                if mode == "fifo-jsonl":
                    first = (
                        b'{"kind":"start"}\n{"kind":"case","file":"case.rs",'
                        b'"name":"early","status":"failed"}\n'
                    )
                    second = b'{"kind":"case","file":"case.rs","name":"later","status":"passed"}\n'
                    expected = b'"name":"early"'
                    unexpected = b'"name":"later"'
                else:
                    first = (
                        b'<testsuites><testsuite name="suite">'
                        + b" " * 300
                        + b'<testcase classname="suite" name="early"><failure/></testcase>'
                    )
                    second = b'<testcase classname="suite" name="later"/>'
                    expected, unexpected = b'name="early"', b'name="later"'
                producer = (
                    "import sys,time; out=open(sys.argv[1],'wb',buffering=0); "
                    "out.write(bytes.fromhex(sys.argv[2])); time.sleep(0.6); "
                    "out.write(bytes.fromhex(sys.argv[3])); time.sleep(1)"
                )
                driver = """import subprocess, sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
import bounded_report
original = Path.replace
spawn = subprocess.Popen
child = None
calls = 0
def start(*args, **kwargs):
    global child
    child = spawn(*args, **kwargs)
    return child
subprocess.Popen = start
def replace(path, target):
    global calls
    calls += 1
    if calls == 2:
        child.terminate()
        child.wait(timeout=5)
        raise SystemExit(77)
    return original(path, target)
Path.replace = replace
bounded_report.capture(sys.argv[2], Path(sys.argv[3]), 256, sys.argv[4:])
"""
                result = subprocess.run(
                    [
                        sys.executable,
                        "-c",
                        driver,
                        str(script.parent),
                        mode,
                        str(output),
                        sys.executable,
                        "-c",
                        producer,
                        str(output) + ".fifo",
                        first.hex(),
                        second.hex(),
                    ],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    timeout=10,
                    check=False,
                )
                self.assertEqual(result.returncode, 77)
                evidence = output.read_bytes()
                self.assertIn(expected, evidence)
                self.assertNotIn(unexpected, evidence)
                if mode == "fifo-jsonl":
                    self.assertTrue(json.loads(evidence.splitlines()[-1])["incomplete"])
                else:
                    self.assertIn(b'incomplete="true"', evidence)

    def test_partial_evidence_is_visible_before_producer_exits(self) -> None:
        script = Path(__file__).with_name("bounded_report.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for mode in ("stdout", "fifo", "fifo-jsonl", "fifo-xml"):
                output = root / mode
                if mode == "fifo-jsonl":
                    initial = (
                        b'{"kind":"start"}\n{"kind":"case","file":"case.rs","name":"early",'
                        b'"status":"failed"}\n{"kind":"end","incomplete":false}\n'
                    )
                    final = b""
                    expected = b'"name":"early"'
                elif mode == "fifo-xml":
                    initial = (
                        b'<testsuites><testsuite name="suite">'
                        + b" " * 300
                        + b'<testcase classname="suite" name="early"><failure/></testcase>'
                    )
                    final = b"</testsuite></testsuites>"
                    expected = b'name="early"'
                else:
                    initial, final, expected = b"early evidence\n", b"finished\n", b"early evidence"
                child = (
                    "import sys; from pathlib import Path; "
                    "out = sys.stdout.buffer if sys.argv[1] == '-' "
                    "else open(sys.argv[1], 'wb', buffering=0); "
                    "out.write(bytes.fromhex(sys.argv[2])); out.flush(); "
                    "sys.stdin.buffer.read(1); out.write(bytes.fromhex(sys.argv[3])); out.flush()"
                )
                destination = "-" if mode == "stdout" else str(output) + ".fifo"
                process = subprocess.Popen(
                    [
                        sys.executable,
                        str(script),
                        mode,
                        str(output),
                        "256",
                        "--",
                        sys.executable,
                        "-c",
                        child,
                        destination,
                        initial.hex(),
                        final.hex(),
                    ],
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                )
                try:
                    deadline = time.monotonic() + 5
                    while time.monotonic() < deadline:
                        if output.exists() and expected in output.read_bytes():
                            break
                        self.assertIsNone(process.poll(), f"{mode} producer exited early")
                        time.sleep(0.01)
                    else:
                        self.fail(f"{mode} evidence not persisted before producer exit")
                    self.assertIsNone(process.poll())
                    if mode == "fifo-jsonl":
                        self.assertTrue(
                            json.loads(output.read_text().splitlines()[-1])["incomplete"]
                        )
                    if mode == "fifo-xml":
                        self.assertIn(b'incomplete="true"', output.read_bytes())
                finally:
                    process.communicate(b"x", timeout=5)

    def test_windows_capture_fails_before_launching_producer(self) -> None:
        import bounded_report

        output = Path("unused")
        with (
            patch.object(os, "name", "nt"),
            patch.object(subprocess, "Popen") as spawn,
        ):
            with self.assertRaisesRegex(OSError, "unsupported on Windows"):
                bounded_report.capture("stdout", output, 256, ["producer"])
            spawn.assert_not_called()

    def test_xml_overflow_retains_late_failure_and_matching_pass(self) -> None:
        script = Path(__file__).with_name("bounded_report.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            first, second = root / "first.xml", root / "second.xml"
            for index, (output, failed) in enumerate(((first, True), (second, False))):
                cases = "".join(f'<testcase classname="suite" name="case{i}"/>' for i in range(30))
                state = "<failure/>" if failed else ""
                report = (
                    '<testsuites><testsuite name="suite">'
                    + cases
                    + f'<testcase classname="suite" name="late">{state}</testcase>'
                    + "</testsuite></testsuites>"
                )
                code = (
                    "from pathlib import Path; import sys; "
                    "Path(sys.argv[1]).write_text(sys.argv[2])"
                )
                env = dict(os.environ)
                if index:
                    env["EJM_HARNESS_JUNIT_REFERENCE"] = str(first)
                else:
                    env.pop("EJM_HARNESS_JUNIT_REFERENCE", None)
                result = subprocess.run(
                    [
                        sys.executable,
                        str(script),
                        "fifo-xml",
                        str(output),
                        "400",
                        "--",
                        sys.executable,
                        "-c",
                        code,
                        str(output) + ".fifo",
                        report,
                    ],
                    env=env,
                    capture_output=True,
                    timeout=10,
                    check=False,
                )
                self.assertEqual(result.returncode, 1)
                self.assertLessEqual(output.stat().st_size, 400)
            inventory, summary = root / "inventory.json", root / "summary.json"
            inventory.write_text("{}")
            result = subprocess.run(
                [
                    sys.executable,
                    "-I",
                    str(script.with_name("registry_rust_compare.py")),
                    str(inventory),
                    str(first),
                    str(second),
                    str(summary),
                ],
                capture_output=True,
                timeout=10,
                check=False,
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
                        [
                            sys.executable,
                            str(script),
                            mode,
                            str(output),
                            "32",
                            "--",
                            sys.executable,
                            "-c",
                            producer,
                            destination,
                            str(size),
                        ],
                        capture_output=True,
                        timeout=10,
                        check=False,
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
                    "import os,sys; os.unlink(sys.argv[1]); open(sys.argv[1], 'wb').write(b'x')"
                    if replacement
                    else "pass"
                )
                result = subprocess.run(
                    [
                        sys.executable,
                        str(script),
                        "fifo",
                        str(output),
                        "32",
                        "--",
                        sys.executable,
                        "-c",
                        code,
                        fifo,
                    ],
                    capture_output=True,
                    timeout=10,
                    check=False,
                )
                self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
