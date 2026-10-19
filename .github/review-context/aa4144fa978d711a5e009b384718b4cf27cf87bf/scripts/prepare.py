#!/usr/bin/env python3
"""Package admitted sources as data without running any project code."""
import json
import os
import re
import subprocess
import urllib.request
from pathlib import Path

MAX_SOURCE_FILES = 10_000
MAX_SOURCE_BYTES = 128_000_000
MAX_TREE_BYTES = 8_000_000
MAX_DIFF_FILES = 200
MAX_DIFF_BYTES = 1_000_000


def run(*args):
    return subprocess.run(args, check=True, capture_output=True).stdout


def full_sha(value):
    if not re.fullmatch(r"[0-9a-f]{40}", value):
        raise ValueError("Source and diff revisions must be full lowercase commit SHAs")
    return value


def source_archive(root, sha, prefix, format_):
    """Export committed bytes, without allowing source-controlled archive attributes."""
    full_sha(sha)
    prefix = prefix.rstrip("/")
    if format_ not in ("tar", "tar.gz", "zip") or not re.fullmatch(r"[A-Za-z0-9._+-]{1,200}", prefix) or prefix in (".", ".."):
        raise ValueError("Invalid source archive format or prefix")
    root = Path(root).resolve()
    tree = run("git", "-C", str(root), "ls-tree", "-r", "-t", "-l", "-z", sha)
    entries = tree.rstrip(b"\0").split(b"\0") if tree else []
    if len(tree) > MAX_TREE_BYTES or len(entries) > MAX_SOURCE_FILES:
        raise ValueError("Source tree exceeds the file or path limit")
    total = 0
    for entry in entries:
        metadata, path = entry.split(b"\t", 1)
        mode, kind, _, size = metadata.split()
        if (kind, mode) != (b"tree", b"040000") and (kind != b"blob" or mode not in (b"100644", b"100755", b"120000")):
            raise ValueError("Source exports require ordinary blobs, not submodules")
        if path.startswith(b"/") or b".." in path.split(b"/"):
            raise ValueError("Unsafe source archive path")
        if kind == b"blob":
            total += int(size)
    if total > MAX_SOURCE_BYTES:
        raise ValueError("Source tree exceeds the byte limit")
    # info/attributes has priority over every committed .gitattributes file.
    # Clear these attributes for directories too, so archive never skips a subtree.
    attributes = Path(run("git", "-C", str(root), "rev-parse", "--path-format=absolute", "--git-path", "info/attributes").decode().strip())
    if attributes.is_symlink():
        raise ValueError("Unsafe Git metadata attributes file")
    original = attributes.read_bytes() if attributes.exists() else None
    attributes.parent.mkdir(parents=True, exist_ok=True)
    try:
        attributes.write_bytes((original or b"") + b"\n* -export-ignore -export-subst\n")
        return run("git", "-C", str(root), "archive", "--format=" + format_, "--prefix=" + prefix + "/", sha)
    finally:
        if original is None:
            attributes.unlink(missing_ok=True)
        else:
            attributes.write_bytes(original)


def compare_url(repository, base, head):
    full_sha(base)
    full_sha(head)
    if not re.fullmatch(r"[A-Za-z0-9-]+/[A-Za-z0-9_.-]+", repository):
        raise ValueError("Invalid GitHub repository")
    return "https://api.github.com/repos/" + repository + "/compare/" + base + "..." + head


def review_diff(repository, base, head, token):
    request = urllib.request.Request(compare_url(repository, base, head), headers={
        "Authorization": "Bearer " + token, "Accept": "application/vnd.github.v3.diff",
    })
    with urllib.request.urlopen(request, timeout=30) as response:
        patch = response.read(MAX_DIFF_BYTES + 1)
    files = len(re.findall(br"^diff --git ", patch, re.M))
    if not patch or len(patch) > MAX_DIFF_BYTES or not files or files > MAX_DIFF_FILES:
        raise ValueError("Empty or oversized review diff; split the PR")
    return patch


def main():
    head, base = full_sha(os.environ["PR_HEAD"]), full_sha(os.environ["PR_BASE"])
    repository = os.environ["GITHUB_REPOSITORY"]
    out = Path("prepared")
    out.mkdir()
    (out / "product.tar").write_bytes(source_archive("product", head, "product", "tar"))
    if Path("harness/.git").exists():
        harness = run("git", "-C", "harness", "rev-parse", "HEAD").decode().strip()
        (out / "harness.tar").write_bytes(source_archive("harness", harness, "ejm-harness", "tar"))
    (out / "change.patch").write_bytes(review_diff(repository, base, head, os.environ["GH_TOKEN"]))
    (out / "context.json").write_text(json.dumps({"repository": repository, "pr": int(os.environ["PR_NUMBER"]), "head": head, "base": base}))


if __name__ == "__main__":
    main()
