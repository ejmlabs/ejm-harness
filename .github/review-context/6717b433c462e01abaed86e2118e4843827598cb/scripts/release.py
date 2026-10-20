#!/usr/bin/env python3
"""Validate a protected main commit and produce draft source-release assets."""
import hashlib
import io
import json
import os
import re
import subprocess
import sys
import tarfile
import urllib.error
import zipfile
from pathlib import Path

# The workflow generator embeds governance.py, standards.py, and prepare.py first.


def validate_archive(data, format_, prefix, required):
    if format_ == "zip":
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            for path, content in required.items():
                if archive.read(prefix + "/" + path) != content:
                    raise ValueError("Source archive license/document differs: " + path)
    else:
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
            for path, content in required.items():
                member = archive.getmember(prefix + "/" + path)
                if not member.isfile() or archive.extractfile(member).read() != content:
                    raise ValueError("Source archive license/document differs: " + path)


def archive_requirements(root, sha, profile):
    names = subprocess.run(["git", "-C", str(root), "ls-tree", "-r", "--name-only", sha], check=True, capture_output=True).stdout.decode().splitlines()
    if len(names) > 100_000:
        raise ValueError("Source tree exceeds release inspection limit")
    required = set(DOCS) | {"VERSION"}
    required.update(path for path in names if path.endswith(("-OFL.txt", "/OFL.txt", "/OFL.md")))
    if profile == "ejm-harness":
        required.update(("sdk/python/LICENSE", "sdk/typescript/LICENSE", "sdk/python/pyproject.toml", "sdk/typescript/package.json"))
    if profile == "ejm-ui":
        required.update(("package.json", "package-lock.json", "crates/ejm-tui/Cargo.toml", "crates/ejm-tui/Cargo.lock", *UI_FONT_NOTICES))
        required.update(path + "/" + name for path in UI_PACKAGES for name in ("package.json", "LICENSE", "THIRD_PARTY_NOTICES.md"))
    if profile in ("ejm-cli", "ejm-ide"):
        required.add(".github/harness-revision")
    return {path: read(root, path, preserve_newlines=True).encode("utf-8") for path in required}


def release(api, root, sha, requested):
    if os.environ.get("GITHUB_RUN_ATTEMPT", "1") != "1":
        raise ValueError("Release reruns are not allowed; inspect the existing tag and draft")
    if not re.fullmatch(r"[0-9a-f]{40}", sha) or not semver(requested):
        raise ValueError("Supply a full commit SHA and a SemVer version")
    actor = api.request("https://api.github.com/users/" + os.environ["GITHUB_ACTOR"])
    if not human(actor, api.permission(actor["login"])):
        raise ValueError("Only a current human maintainer can prepare a release")
    main = api.request("/branches/main")["commit"]["sha"]
    if sha != main:
        raise ValueError("Release commit must be the current protected main head")
    version = versions(root, api.repo.split("/")[1])
    if version != requested:
        raise ValueError("Requested version differs from committed VERSION")
    tag = "v" + version
    for path in ("/git/ref/tags/" + tag, "/releases/tags/" + tag):
        try:
            api.request(path)
        except urllib.error.HTTPError as error:
            if error.code == 404:
                continue
            raise
        raise ValueError("Version tag or release already exists; never overwrite it")
    pulls = api.pages(f"/commits/{sha}/pulls")
    eligible = [p for p in pulls if p.get("merged_at") and p["base"]["ref"] == "main" and p["merge_commit_sha"] == sha]
    if len(eligible) != 1:
        raise ValueError("Release commit must come from exactly one checked merged PR")
    pr = eligible[0]
    head = pr["head"]["sha"]
    statuses = api.statuses(head)
    if any(statuses.get(name, {}).get("state") != "success" for name in CONTEXTS):
        raise ValueError("Merged PR lacks a successful required check")
    for kind, context in (("ready", "Human readiness"), ("approve", "Maintainer approval")):
        match = re.search(r"\[" + kind + r":(\d+)\]", statuses[context]["description"])
        if not match:
            raise ValueError("Missing human decision evidence")
        comment = api.request(f"/issues/comments/{match[1]}")
        if (comment["issue_url"] != f"https://api.github.com/repos/{api.repo}/issues/{pr['number']}"
                or command(comment["body"]) != (kind, head)
                or comment["created_at"] != comment["updated_at"]):
            raise ValueError("Human decision was edited or belongs to another PR")
        if not human(comment["user"], api.permission(comment["user"]["login"])):
            raise ValueError("Approver is no longer a human maintainer")
    target_tree = api.request(f"/git/commits/{sha}")["tree"]["sha"]
    head_tree = api.request(f"/git/commits/{head}")["tree"]["sha"]
    if target_tree != head_tree:
        raise ValueError("Main source differs from the checked PR source; revalidate through a PR")
    # Source inspection only. No hooks, package lifecycle, or project scripts run here.
    output = Path("release-assets")
    output.mkdir()
    prefix = api.repo.split("/")[1].lstrip(".") + "-" + version
    required = archive_requirements(root, sha, api.repo.split("/")[1])
    for format_, suffix in (("tar.gz", "tar.gz"), ("zip", "zip")):
        archive = source_archive(root, sha, prefix, format_)
        # export-ignore/export-subst must never silently remove or alter notices.
        validate_archive(archive, format_, prefix, required)
        (output / (prefix + "." + suffix)).write_bytes(archive)
    manifest = {"repository": api.repo, "version": version, "commit": sha, "checked_pr": pr["number"], "checked_head": head,
                "workflow_run": f"https://github.com/{api.repo}/actions/runs/{os.environ['GITHUB_RUN_ID']}",
                "artifact_kind": "source-only", "signed": False, "standards_revision": os.environ["STANDARDS_REF"]}
    (output / "provenance.json").write_text(json.dumps(manifest, indent=2) + "\n")
    files = sorted(output.iterdir())
    (output / "SHA256SUMS").write_text("".join(hashlib.sha256(file.read_bytes()).hexdigest() + "  " + file.name + "\n" for file in files))
    notes = read(root, "CHANGELOG.md")
    match = re.search(r"^## \[" + re.escape(version) + r"\] - .+\n([\s\S]*?)(?=^## |\Z)", notes, re.M)
    if not match or not match[1].strip():
        raise ValueError("Release needs nonempty dated notes")
    Path("release-notes.md").write_text(match[1].strip() + "\n\nSource archives only. See RELEASING.md for dependency pins and binary publication gates.\n")
    outputs(version=version, tag=tag, commit=sha)


if __name__ == "__main__":
    release(GitHub(), Path("product"), os.environ["RELEASE_COMMIT"], os.environ["RELEASE_VERSION"])
