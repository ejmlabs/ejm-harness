"""Exercise registry Rust comparison on partial nextest evidence."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


class RegistryRustCompareTests(unittest.TestCase):
    def test_oversized_summary_retains_comparable_failures(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            listing = root / "inventory.json"
            listing.write_text("{}")
            first, second = root / "first.xml", root / "second.xml"
            names = [f"{index:05}" + "x" * 900 for index in range(2500)]
            for path, state in ((first, "<failure/>"), (second, "")):
                path.write_text(
                    '<testsuites><testsuite name="suite">'
                    + "".join(
                        f'<testcase classname="suite" name="{name}">{state}</testcase>'
                        for name in names
                    )
                    + "</testsuite></testsuites>"
                )
            summary = root / "summary.json"
            result = subprocess.run(
                [
                    sys.executable,
                    "-I",
                    str(Path(__file__).with_name("registry_rust_compare.py")),
                    str(listing),
                    str(first),
                    str(second),
                    str(summary),
                ],
                capture_output=True,
                check=False,
            )
            data = json.loads(summary.read_text())
            self.assertEqual(result.returncode, 1)
            self.assertLessEqual(summary.stat().st_size, 4 * 1024 * 1024)
            self.assertTrue(data["incomplete"] and data["summary_truncated"])
            self.assertTrue(data["mixed"] and data["first_failed"])

    def test_truncated_multibyte_suffix_keeps_completed_case(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            listing = root / "inventory.json"
            listing.write_text("{}")
            first, second = root / "first.xml", root / "second.xml"
            first.write_bytes(
                b'<testsuites><testsuite name="suite">'
                b'<testcase classname="suite" name="flip"><failure/></testcase>'
                b'<testcase classname="suite" name="\xc3'
            )
            second.write_text(
                '<testsuites><testsuite name="suite">'
                '<testcase classname="suite" name="flip"/>'
                "</testsuite></testsuites>"
            )
            summary = root / "summary.json"
            result = subprocess.run(
                [
                    sys.executable,
                    "-I",
                    str(Path(__file__).with_name("registry_rust_compare.py")),
                    str(listing),
                    str(first),
                    str(second),
                    str(summary),
                ],
                capture_output=True,
                check=False,
            )
            data = json.loads(summary.read_text())
            self.assertEqual(result.returncode, 1)
            self.assertTrue(data["incomplete"])
            self.assertEqual(data["mixed"], [["suite", "flip"]])

    def test_truncated_report_keeps_completed_comparable_case(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            listing = root / "inventory.json"
            listing.write_text("{}")
            first, second = root / "first.xml", root / "second.xml"
            first.write_text(
                '<testsuites><testsuite name="suite">'
                '<testcase classname="suite" name="flip"><failure/></testcase>'
                '<testcase classname="suite" name="truncated"'
            )
            second.write_text(
                '<testsuites><testsuite name="suite">'
                '<testcase classname="suite" name="flip"/>'
                "</testsuite></testsuites>"
            )
            summary = root / "summary.json"
            result = subprocess.run(
                [
                    sys.executable,
                    "-I",
                    str(Path(__file__).with_name("registry_rust_compare.py")),
                    str(listing),
                    str(first),
                    str(second),
                    str(summary),
                ],
                capture_output=True,
                check=False,
            )
            data = json.loads(summary.read_text())
            self.assertEqual(result.returncode, 1)
            self.assertTrue(data["incomplete"])
            self.assertEqual(data["mixed"], [["suite", "flip"]])

    def test_duplicate_failure_does_not_hide_an_unambiguous_flake(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            listing = root / "inventory.json"
            listing.write_text("{}")
            first, second = root / "first.xml", root / "second.xml"
            first.write_text(
                '<testsuites><testsuite name="suite">'
                '<testcase classname="suite" name="duplicate"><failure/></testcase>'
                '<testcase classname="suite" name="duplicate"/>'
                '<testcase classname="suite" name="flip"><failure/></testcase>'
                "</testsuite></testsuites>"
            )
            second.write_text(
                '<testsuites><testsuite name="suite">'
                '<testcase classname="suite" name="duplicate"/>'
                '<testcase classname="suite" name="flip"/>'
                "</testsuite></testsuites>"
            )
            summary = root / "summary.json"
            script = Path(__file__).with_name("registry_rust_compare.py")
            result = subprocess.run(
                [
                    sys.executable,
                    "-I",
                    str(script),
                    str(listing),
                    str(first),
                    str(second),
                    str(summary),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 1)
            data = json.loads(summary.read_text())
            self.assertTrue(data["incomplete"])
            self.assertEqual(data["mixed"], [["suite", "flip"]])
            self.assertEqual(data["first_failed"], [["suite", "duplicate"], ["suite", "flip"]])

    def test_case_limit_preserves_already_parsed_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            listing = root / "inventory.json"
            listing.write_text("{}")
            first, second = root / "first.xml", root / "second.xml"
            cases = "".join(f'<testcase classname="suite" name="case{i}"/>' for i in range(20_000))
            first.write_text(
                '<testsuites><testsuite name="suite">'
                '<testcase classname="suite" name="flip"><failure/></testcase>'
                + cases
                + "</testsuite></testsuites>"
            )
            second.write_text(
                '<testsuites><testsuite name="suite">'
                '<testcase classname="suite" name="flip"/>'
                "</testsuite></testsuites>"
            )
            summary = root / "summary.json"
            result = subprocess.run(
                [
                    sys.executable,
                    "-I",
                    str(Path(__file__).with_name("registry_rust_compare.py")),
                    str(listing),
                    str(first),
                    str(second),
                    str(summary),
                ],
                capture_output=True,
                check=False,
            )
            data = json.loads(summary.read_text())
            self.assertEqual(result.returncode, 1)
            self.assertTrue(data["incomplete"])
            self.assertEqual(data["mixed"], [["suite", "flip"]])
            self.assertEqual(data["first_failed"], [["suite", "flip"]])

    def test_incomplete_inventory_keeps_a_comparable_flake(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            listing = root / "inventory.json"
            listing.write_text("{}")
            first, second = root / "first.xml", root / "second.xml"
            for path, failure in ((first, True), (second, False)):
                count = int(failure)
                attributes = f'tests="1" failures="{count}" errors="0" skipped="0"'
                path.write_text(
                    f'<testsuites {attributes}><testsuite name="suite" {attributes}>'
                    f'<testcase classname="suite" name="case">'
                    f"{'<failure/>' if failure else ''}</testcase>"
                    "</testsuite></testsuites>"
                )
            summary = root / "summary.json"
            script = Path(__file__).with_name("registry_rust_compare.py")
            result = subprocess.run(
                [
                    sys.executable,
                    "-I",
                    str(script),
                    str(listing),
                    str(first),
                    str(second),
                    str(summary),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 1)
            data = json.loads(summary.read_text())
            self.assertTrue(data["incomplete"])
            self.assertEqual(data["mixed"], [["suite", "case"]])
            self.assertEqual(data["first_failed"], [["suite", "case"]])


if __name__ == "__main__":
    unittest.main()
