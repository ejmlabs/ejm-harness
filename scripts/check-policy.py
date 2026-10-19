#!/usr/bin/env python3
"""Check crate boundaries, lint inheritance, native exceptions, and project skills."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import tempfile
import tomllib
import unittest
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
Object = dict[str, Any]
OPENSPEC_SKILLS = frozenset(
    f"openspec-{action}"
    for action in (
        "apply-change",
        "archive-change",
        "explore",
        "propose",
        "sync-specs",
        "update-change",
    )
)


def dependency_errors(package: Object, policies: Object) -> list[str]:
    """Check actual Cargo dependencies, including renamed dependencies."""
    name: str = package["name"]
    if name not in policies:
        return [f"{name}: add an explicit crate boundary and rationale to the policy"]
    policy = policies[name]
    errors = []
    for dependency in package["dependencies"]:
        target = dependency["name"]
        if target in policy["forbidden"] or (
            target.startswith("ejm-harness") and target not in policy["internal"]
        ):
            errors.append(f"{name}: dependency on {target} violates its boundary")
    if not policy.get("reason", "").strip():
        errors.append(f"{name}: the crate boundary needs a rationale")
    return errors


def source_errors(root: Path, unsafe_files: set[str]) -> list[str]:
    """Keep exceptions local and native unsafe access in reviewed modules."""
    errors = []
    broad = re.compile(r"(?:allow|expect)\s*\([^)]*clippy::(?:all|pedantic)\b", re.DOTALL)
    native = re.compile(r"#\s*!?\s*\[\s*(?:allow|expect)\s*\(\s*unsafe_code\b")
    for path in (root / "crates").rglob("*.rs"):
        source = path.read_text()
        relative = path.relative_to(root).as_posix()
        if broad.search(source):
            errors.append(f"{relative}: broad Clippy allowances are prohibited")
        if native.search(source) and relative not in unsafe_files:
            errors.append(f"{relative}: native unsafe exception is outside the reviewed modules")
        if "\u2014" in source:
            errors.append(f"{relative}: use plain punctuation instead of an em dash")
    return errors


def skill_errors(root: Path) -> list[str]:
    """Validate discoverable skill metadata and local reference paths."""
    root = root.resolve()
    errors = []
    routing = (root / "AGENTS.md").read_text()
    skills = sorted((root / ".agents/skills").glob("*/SKILL.md"))
    if not skills:
        return ["project skills are missing"]
    for path in skills:
        relative = path.relative_to(root).as_posix()
        text = path.read_text()
        header, separator, body = text.removeprefix("---\n").partition("\n---\n")
        if not text.startswith("---\n") or not separator:
            errors.append(f"{relative}: YAML frontmatter is missing")
            continue
        metadata = dict(line.split(": ", 1) for line in header.splitlines() if ": " in line)
        if metadata.get("name") != path.parent.name:
            errors.append(f"{relative}: skill name must match its directory")
        if not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", metadata.get("name", "")):
            errors.append(f"{relative}: skill name must use lowercase words with hyphens")
        generated = path.parent.name in OPENSPEC_SKILLS
        description = metadata.get("description", "")
        if not description or (
            not generated and (len(description) > 300 or "<" in description or ">" in description)
        ):
            errors.append(f"{relative}: use a concise single-line discovery description")
        if relative not in routing:
            errors.append(f"{relative}: add the skill to AGENTS.md routing")
        if not generated and len(body.splitlines()) > 100:
            errors.append(f"{relative}: move conditional detail into linked references")
        if "[TODO:" in body or (not generated and "\u2014" in text):
            errors.append(f"{relative}: remove unfinished instructions or em dashes")
        for target in re.findall(r"\[[^\]]*\]\(([^)]+)\)", body):
            if "://" in target or target.startswith("#"):
                continue
            linked = (path.parent / target.split("#", 1)[0]).resolve()
            if not linked.is_relative_to(root) or not linked.exists():
                errors.append(
                    f"{relative}: local reference is missing or outside the repo: {target}"
                )
    return errors


def check(root: Path) -> list[str]:
    """Read Cargo's resolved graph without compiling source."""
    policy = tomllib.loads((root / "scripts/architecture-policy.toml").read_text())
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    )
    metadata = json.loads(result.stdout)
    errors = []
    for package in metadata["packages"]:
        errors.extend(dependency_errors(package, policy["crates"]))
        manifest = tomllib.loads(Path(package["manifest_path"]).read_text())
        if manifest.get("lints", {}).get("workspace") is not True:
            errors.append(f"{package['name']}: inherit workspace lints")
    errors.extend(source_errors(root, set(policy["unsafe"]["files"])))
    errors.extend(skill_errors(root))
    return errors


class PolicyTests(unittest.TestCase):
    """Prove the gate rejects boundary violations rather than matching wording."""

    def test_forbidden_and_unknown_dependencies_fail(self) -> None:
        policies = {"core": {"internal": [], "forbidden": ["ratatui"], "reason": "No UI"}}
        self.assertTrue(dependency_errors({"name": "unknown", "dependencies": []}, policies))
        self.assertTrue(
            dependency_errors({"name": "core", "dependencies": [{"name": "ratatui"}]}, policies)
        )
        self.assertTrue(
            dependency_errors(
                {"name": "core", "dependencies": [{"name": "ejm-harness-ui"}]}, policies
            )
        )
        self.assertFalse(
            dependency_errors({"name": "core", "dependencies": [{"name": "serde"}]}, policies)
        )

    def test_native_and_broad_lint_exceptions_fail(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            source = root / "crates/core/src/lib.rs"
            source.parent.mkdir(parents=True)
            source.write_text('#[allow(unsafe_code, reason = "test")]\nfn native() {}\n')
            self.assertTrue(source_errors(root, set()))
            self.assertFalse(source_errors(root, {"crates/core/src/lib.rs"}))
            source.write_text('#![allow(clippy::pedantic, reason = "test")]\n')
            self.assertTrue(source_errors(root, set()))

    def test_missing_skill_references_fail(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            relative = ".agents/skills/harness-test/SKILL.md"
            source = root / relative
            source.parent.mkdir(parents=True)
            (root / "AGENTS.md").write_text(relative)
            source.write_text(
                "---\nname: harness-test\ndescription: Test project skills.\n---\n"
                "Read [the contract](missing.md).\n"
            )
            self.assertTrue(skill_errors(root))
            (source.parent / "missing.md").write_text("A maintained reference.\n")
            self.assertFalse(skill_errors(root))

    def test_only_known_openspec_skills_use_generated_style(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            source = root / ".agents/skills/openspec-explore/SKILL.md"
            source.parent.mkdir(parents=True)
            routing = root / "AGENTS.md"
            routing.write_text(source.relative_to(root).as_posix())
            text = (
                "---\nname: openspec-explore\ndescription: "
                + "Generated workflow. " * 20
                + "\n---\n"
                + "Generated guidance.\n" * 101
            )
            source.write_text(text)
            self.assertFalse(skill_errors(root))
            source.write_text(text + "Read [the contract](missing.md).\n")
            self.assertTrue(skill_errors(root))
            source.write_text(text)
            routing.write_text("")
            self.assertTrue(skill_errors(root))
            routing.write_text(source.relative_to(root).as_posix())
            source.write_text(text.replace("name: openspec-explore", "name: wrong"))
            self.assertTrue(skill_errors(root))
            target = source.parent.with_name("openspec-custom")
            source.parent.rename(target)
            source = target / "SKILL.md"
            source.write_text(text.replace("openspec-explore", "openspec-custom"))
            routing.write_text(source.relative_to(root).as_posix())
            self.assertTrue(skill_errors(root))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        unittest.main(argv=["check-policy"], exit=True)
    errors = check(ROOT)
    if errors:
        raise SystemExit("\n".join(errors))
    print("Crate boundaries, lint inheritance, native exceptions, and skills pass")


if __name__ == "__main__":
    main()
