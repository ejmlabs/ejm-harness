#!/usr/bin/env python3
"""Dependency-free documentation, workflow pin, and release version checks."""
import json
import re
import sys
import tomllib
from pathlib import Path
from urllib.parse import unquote

HEADINGS = ("Overview", "Status", "Requirements", "Quick start", "Usage", "Development", "Documentation", "Contributing", "Security", "Support", "License")
DOCS = ("README.md", "CONTRIBUTING.md", "SECURITY.md", "SUPPORT.md", "CODE_OF_CONDUCT.md", "RELEASING.md", "THIRD_PARTY_NOTICES.md", "CHANGELOG.md", "LICENSE")
COMMUNITY_GUIDES = {"ejm-cli": "docs/contributing.md", "ejm-harness": "docs/contributing.md", "ejm-ide": "docs/development.md", "ejmlabs": "site/README.md", ".github": "docs/operations.md"}
VERSIONS = {
    "ejm-cli": (("Cargo.toml", "workspace.package.version"),),
    "ejm-harness": (("Cargo.toml", "workspace.package.version"), ("sdk/typescript/package.json", "version"), ("sdk/python/pyproject.toml", "project.version"), ("registry/Cargo.toml", "package.version")),
    "ejm-ide": (("package.json", "version"), ("package-lock.json", "version"), ("src-tauri/Cargo.toml", "package.version"), ("src-tauri/tauri.conf.json", "version")),
    "ejmlabs": (("site/package.json", "version"),),
    ".github": (),
}
SEMVER = re.compile(r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$")
MIT_TERMS = '''Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:
The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.
THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.'''


def semver(value):
    match = SEMVER.fullmatch(value)
    if not match:
        return False
    return not match[4] or all(not item.isdigit() or item == "0" or not item.startswith("0") for item in match[4].split("."))


def read(root, path):
    file = root / path
    if file.is_symlink() or not file.resolve().is_relative_to(root.resolve()) or not file.is_file():
        raise ValueError(f"Missing or unsafe file: {path}")
    if file.stat().st_size > 8_000_000:
        raise ValueError(f"File exceeds validation limit: {path}")
    return file.read_text(encoding="utf-8")


def versions(root, profile):
    if profile not in VERSIONS:
        raise ValueError("Unknown repository profile")
    canonical = read(root, "VERSION").strip()
    if not semver(canonical):
        raise ValueError("VERSION must be valid SemVer without a v prefix")
    licenses(root, profile)
    for path, key in VERSIONS[profile]:
        text = read(root, path)
        value = json.loads(text) if path.endswith(".json") else tomllib.loads(text)
        for part in key.split("."):
            value = value[part]
        if value != canonical:
            raise ValueError(f"{path}: {key} differs from VERSION")
    if profile == "ejm-ide":
        lock = json.loads(read(root, "package-lock.json"))
        if lock["packages"][""]["version"] != canonical:
            raise ValueError("package-lock.json root package differs from VERSION")
    if profile == "ejm-harness":
        for path in ("sdk/typescript/LICENSE", "sdk/python/LICENSE"):
            if read(root, path) != read(root, "LICENSE"):
                raise ValueError("SDK license text differs from root: " + path)
        lock = json.loads(read(root, "package-lock.json"))
        if lock["packages"]["sdk/typescript"]["version"] != canonical:
            raise ValueError("SDK npm lock version differs from VERSION")
        python_lock = tomllib.loads(read(root, "sdk/python/uv.lock"))
        if not any(p["name"] == "ejm-harness" and p["version"] == canonical for p in python_lock["package"]):
            raise ValueError("SDK Python lock version differs from VERSION")
    lockfile = "src-tauri/Cargo.lock" if profile == "ejm-ide" else "Cargo.lock"
    if profile in ("ejm-cli", "ejm-harness", "ejm-ide"):
        lock = tomllib.loads(read(root, lockfile))
        owned = [p for p in lock["package"] if p["name"] == profile or (profile == "ejm-harness" and p["name"].startswith("ejm-harness"))]
        if not owned or any(p["version"] != canonical for p in owned):
            raise ValueError("Owned Cargo lock package versions differ from VERSION")
    notes = read(root, "CHANGELOG.md")
    if not re.search(r"^## \[" + re.escape(canonical) + r"\] - \d{4}-\d{2}-\d{2}$", notes, re.M):
        raise ValueError("CHANGELOG.md needs a dated entry for VERSION")
    return canonical


def licenses(root, profile):
    if profile == "ejmlabs":
        if json.loads(read(root, "site/package.json")).get("license") != "SEE LICENSE IN ../LICENSE":
            raise ValueError("Studio metadata must preserve its specific rights notice")
        return
    text = read(root, "LICENSE")
    if not text.startswith("MIT License") or not re.search(r"Copyright.*\d{4}.*\S", text) or " ".join(MIT_TERMS.split()) not in " ".join(text.split()):
        raise ValueError("Complete MIT license terms and copyright notice are required")
    manifests = []
    if profile in ("ejm-cli", "ejm-harness"):
        cargo = tomllib.loads(read(root, "Cargo.toml"))
        if cargo["workspace"]["package"].get("license") != "MIT":
            raise ValueError("Workspace license must remain MIT")
        members = cargo["workspace"]["members"]
        if len(members) > 50:
            raise ValueError("Too many owned manifests")
        for member in members:
            package = tomllib.loads(read(root, member + "/Cargo.toml"))["package"]
            if package.get("license") not in ("MIT", {"workspace": True}):
                raise ValueError("Owned crate license differs from MIT: " + member)
    if profile == "ejm-harness":
        manifests = [("sdk/typescript/package.json", "license"), ("sdk/python/pyproject.toml", "project.license"), ("registry/Cargo.toml", "package.license")]
    elif profile == "ejm-ide":
        manifests = [("package.json", "license"), ("src-tauri/Cargo.toml", "package.license")]
    for path, key in manifests:
        value = json.loads(read(root, path)) if path.endswith(".json") else tomllib.loads(read(root, path))
        for part in key.split("."):
            value = value[part]
        if value != "MIT":
            raise ValueError("Owned manifest license differs from MIT: " + path)


def consumer_workflows(revision):
    """Return the only accepted consumer workflows for one reviewed revision."""
    if not isinstance(revision, str) or not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise ValueError("Standards revision must be a full lowercase commit SHA")
    return {name: text.replace("STANDARDS_REVISION", revision) for name, text in {
        "pr.yml": """name: Human-admitted PR checks
on:
  issue_comment:
    types: [created, edited, deleted]
permissions: {}
jobs:
  governance:
    if: ${{ github.event.issue.pull_request }}
    permissions:
      contents: read
      pull-requests: write
      statuses: write
      actions: write
    uses: ejmlabs/.github/.github/workflows/governance.yml@STANDARDS_REVISION
    secrets:
      OPENAI_API_KEY: ${{ secrets.OPENAI_API_KEY }}
      EJM_HARNESS_TOKEN: ${{ secrets.EJM_HARNESS_TOKEN }}
""",
        "release.yml": """name: Draft source release
on:
  workflow_dispatch:
    inputs:
      commit:
        description: Full protected main commit SHA
        required: true
        type: string
      version:
        description: SemVer version matching VERSION
        required: true
        type: string
permissions: {}
jobs:
  release:
    permissions:
      contents: write
      pull-requests: read
      statuses: read
    uses: ejmlabs/.github/.github/workflows/draft-release.yml@STANDARDS_REVISION
    with:
      commit: ${{ inputs.commit }}
      version: ${{ inputs.version }}
      standards-ref: "STANDARDS_REVISION"
""",
    }.items()}


def _reusable_workflow(text, name):
    """Accept only the trigger scaffold and YAML subset emitted by our generator."""
    header = '# Generated by scripts/build_workflows.py. Edit scripts and regenerate.\n'
    if name == "governance.yml":
        header += '''"name": "Human-admitted PR checks"
"on":
  "workflow_call":
    "secrets":
      "OPENAI_API_KEY":
        "required": false
      "EJM_HARNESS_TOKEN":
        "required": false
"permissions": {}
'''
        roots = ['"jobs":']
    else:
        header += '''"name": "Checked draft source release"
"on":
  "workflow_call":
    "inputs":
      "commit":
        "required": true
        "type": "string"
      "version":
        "required": true
        "type": "string"
      "standards-ref":
        "required": true
        "type": "string"
"permissions": {}
'''
        roots = ['"concurrency":', '"jobs":']
    if not text.startswith(header):
        raise ValueError("Reusable workflow must retain its workflow_call scaffold: " + name)
    top = [line for line in text.splitlines() if line and not line[0].isspace()]
    expected = [line for line in header.splitlines() if line and not line[0].isspace()] + roots
    if top != expected:
        raise ValueError("Unexpected or duplicate reusable workflow key: " + name)

    # This is deliberately not a general YAML parser. The generator quotes keys
    # and scalar values as JSON and emits block strings for embedded scripts.
    # Reject other YAML syntax instead of guessing its meaning. Block contents
    # are data here; fake uses/on text inside a script is not workflow structure.
    block = None
    for line in text.splitlines():
        if not line.strip():
            continue
        indent = len(line) - len(line.lstrip(" "))
        if block is not None and indent > block:
            continue
        block = None
        stripped = line.lstrip(" ")
        if stripped.startswith("#") or stripped == "-":
            continue
        if stripped.startswith("- "):
            key, value = None, stripped[2:]
        else:
            field = re.fullmatch(r'("(?:[^"\\]|\\.)*"):(?: (.*))?', stripped)
            if not field:
                raise ValueError("Reusable workflow must use canonical generated YAML: " + name)
            key, value = json.loads(field[1]), field[2]
            if json.dumps(key) != field[1]:
                raise ValueError("Reusable workflow has a noncanonical key: " + name)
        if value is None:
            continue
        if value == "|":
            block = indent
            continue
        scalar = json.loads(value)
        if isinstance(scalar, (dict, list)) and scalar:
            raise ValueError("Reusable workflow has an unexpected flow mapping or list: " + name)
        if key == "uses" and (not isinstance(scalar, str) or not re.fullmatch(r"[^@\s]+@[0-9a-f]{40}", scalar)):
            raise ValueError("Reusable workflow action must use an immutable SHA: " + name)


def workflows(root, profile):
    directory = root / ".github/workflows"
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError("Missing or unsafe workflows directory")
    allowed = {"pr.yml", "release.yml"}
    if profile == ".github":
        allowed.update(("governance.yml", "draft-release.yml"))
    if any(file.name not in allowed for file in directory.iterdir()):
        raise ValueError("Unexpected workflow; use only the canonical consumers and shared reusables")
    pr = read(root, ".github/workflows/pr.yml")
    # Extract the candidate pin only. Exact document comparison below is the
    # validation, so comments, quoted keys, duplicate triggers and extra jobs fail.
    pin = re.search(r"^    uses: ejmlabs/\.github/\.github/workflows/governance\.yml@([0-9a-f]{40})$", pr, re.M)
    if not pin:
        raise ValueError("PR workflow must pin governance.yml to a full commit SHA")
    for name, expected in consumer_workflows(pin[1]).items():
        if read(root, ".github/workflows/" + name) != expected:
            raise ValueError("Workflow differs from the canonical consumer: " + name)
    if profile == ".github":
        for name in ("governance.yml", "draft-release.yml"):
            if (directory / name).exists() or (directory / name).is_symlink():
                _reusable_workflow(read(root, ".github/workflows/" + name), name)


def uniform_guidance(root, profile, templates):
    for name, template in templates.items():
        expected = template.replace("{{repository}}", "ejmlabs/" + profile).replace("{{development_guide}}", COMMUNITY_GUIDES[profile])
        if read(root, name) != expected:
            raise ValueError("Community guidance differs from the pinned standard: " + name)


def standards(root, profile, templates=None):
    version = versions(root, profile)
    if templates is None:
        directory = Path(__file__).resolve().parents[1] / "templates/community"
        templates = {file.name: file.read_text() for file in directory.glob("*.md")}
    if not templates:
        raise ValueError("Pinned community templates are required")
    uniform_guidance(root, profile, templates)
    for path in DOCS:
        text = read(root, path)
        if not text.strip() or "\u2014" in text:
            raise ValueError(f"Empty document or em dash: {path}")
        if not path.endswith(".md"):
            continue
        for target in re.findall(r"\[[^\]]*\]\(([^)]+)\)", text):
            target = target.strip("<>").split("#", 1)[0]
            if not target or re.match(r"[a-zA-Z][a-zA-Z0-9+.-]*:", target):
                continue
            resolved = (root / Path(path).parent / unquote(target)).resolve()
            if not resolved.is_relative_to(root.resolve()) or not resolved.exists():
                raise ValueError(f"Broken or outside-repository link in {path}: {target}")
    if profile != "ejmlabs" and "MIT License" not in read(root, "LICENSE"):
        raise ValueError("Product and standards source must retain the MIT license")
    readme = read(root, "README.md")
    for heading in HEADINGS:
        if not re.search(r"^## " + re.escape(heading) + r"$", readme, re.M):
            raise ValueError(f"README.md lacks heading: {heading}")
    for path in (".github/CODEOWNERS", ".github/dependabot.yml", ".github/pull_request_template.md", ".github/ISSUE_TEMPLATE/config.yml"):
        read(root, path)
    workflows(root, profile)
    if profile in ("ejm-cli", "ejm-ide"):
        if not re.fullmatch(r"[0-9a-f]{40}\n?", read(root, ".github/harness-revision")):
            raise ValueError("harness revision must be a full immutable commit")
    print(f"Repository standards and version {version} passed for {profile}")


if __name__ == "__main__":
    try:
        standards(Path(sys.argv[1]), sys.argv[2])
    except (ValueError, KeyError, OSError, tomllib.TOMLDecodeError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
