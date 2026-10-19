#!/usr/bin/env python3
"""Package admitted sources as data without running any project code."""
import hashlib
import json
import os
import re
import subprocess
import sys
import urllib.request
from pathlib import Path

MAX_SOURCE_FILES = 10_000
MAX_SOURCE_BYTES = 128_000_000
MAX_TREE_BYTES = 8_000_000
MAX_DIFF_FILES = 200
MAX_DIFF_BYTES = 1_000_000
HARNESS_REPOSITORY = "ejmlabs/ejm-harness"
HARNESS_PIN = ".github/harness-revision"
MAX_HARNESS_METADATA_BYTES = 8_000_000
MAX_HARNESS_SELECTION_BYTES = 4_096


def run(*args):
    return subprocess.run(args, check=True, capture_output=True).stdout


def full_sha(value):
    if not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{40}", value):
        raise ValueError("Source and diff revisions must be full lowercase commit SHAs")
    return value


def ui_revision(root):
    """Read the optional owner source pin from protected main, never PR source."""
    root = Path(root).resolve()
    path = root / ".github/ui-source.json"
    if (root / ".github").is_symlink() or path.is_symlink():
        raise ValueError("Unsafe UI source pin")
    if not path.exists():
        return ""
    if not path.is_file() or path.stat().st_size > 1024:
        raise ValueError("Missing or oversized UI source pin")
    value = json.loads(path.read_text(encoding="utf-8"))
    if (not isinstance(value, dict) or set(value) != {"repository", "revision"}
            or value["repository"] != "ejmlabs/ejm-ui"):
        raise ValueError("UI source must be the fixed ejmlabs/ejm-ui owner")
    revision = value["revision"]
    if not isinstance(revision, str):
        raise ValueError("UI source revision must be a commit SHA")
    return full_sha(revision)


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


def harness_pin(root, revision):
    """Read a small ordinary pin blob, never a source-controlled filesystem path."""
    full_sha(revision)
    entry = run("git", "-C", str(root), "ls-tree", "-l", "-z", revision, "--", HARNESS_PIN)
    if not entry.endswith(b"\0") or entry.count(b"\0") != 1:
        raise ValueError("Missing or unsafe harness pin blob")
    metadata, path = entry[:-1].split(b"\t", 1)
    mode, kind, blob, size = metadata.split()
    if path != HARNESS_PIN.encode() or mode not in (b"100644", b"100755") or kind != b"blob" or size not in (b"40", b"41"):
        raise ValueError("Harness pin must be a small ordinary Git blob")
    value = run("git", "-C", str(root), "cat-file", "blob", full_sha(blob.decode()))
    if not re.fullmatch(br"[0-9a-f]{40}\n?", value):
        raise ValueError("Unpinned harness dependency")
    return value.decode().rstrip("\n")


class NoHarnessRedirects(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, response, code, message, headers, new_url):
        raise ValueError("Harness source authority must not redirect")


def harness_json(path, token):
    """Read bounded metadata only from the fixed harness API authority."""
    if not re.fullmatch(r"branches/main|compare/[0-9a-f]{40}\.\.\.[0-9a-f]{40}\?per_page=1", path):
        raise ValueError("Invalid harness metadata path")
    url = "https://api.github.com/repos/" + HARNESS_REPOSITORY + "/" + path
    request = urllib.request.Request(url, headers={
        "Authorization": "Bearer " + token, "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
    })
    try:
        with urllib.request.build_opener(NoHarnessRedirects()).open(request, timeout=30) as response:
            if response.status != 200 or response.geturl() != url:
                raise ValueError("Unexpected harness metadata response")
            data = response.read(MAX_HARNESS_METADATA_BYTES + 1)
        if not data or len(data) > MAX_HARNESS_METADATA_BYTES:
            raise ValueError("Harness metadata exceeds its byte limit")
        value = json.loads(data)
        if not isinstance(value, dict):
            raise ValueError("Harness metadata must be an object")
        return value
    except (OSError, ValueError):
        raise ValueError("Could not verify protected harness source metadata") from None


def select_harness(profile, head, base, token):
    if profile == "ejm-cli":
        revision = harness_pin("product", full_sha(head))
        try:
            branch = harness_json("branches/main", token)
            if branch.get("name") != "main" or branch.get("protected") is not True:
                raise ValueError("Harness main must be protected")
            main = full_sha(branch["commit"]["sha"])
            comparison = harness_json("compare/" + revision + "..." + main + "?per_page=1", token)
            ahead = comparison.get("ahead_by")
            if (comparison.get("status") != ("identical" if revision == main else "ahead")
                    or type(comparison.get("behind_by")) is not int or comparison["behind_by"] != 0
                    or type(ahead) is not int or (ahead != 0 if revision == main else ahead <= 0)
                    or comparison["base_commit"]["sha"] != revision
                    or comparison["merge_base_commit"]["sha"] != revision):
                raise ValueError("Harness pin is not an ancestor of protected main")
            return {"repository": HARNESS_REPOSITORY, "revision": revision,
                    "tree": full_sha(comparison["base_commit"]["commit"]["tree"]["sha"]),
                    "protected_main": main, "pin_source": "head", "pin_revision": head}
        except (KeyError, TypeError):
            raise ValueError("Incomplete protected harness source metadata") from None
    if profile == "ejm-ide":
        # IDE keeps its existing trusted-base selection until its own adoption.
        pin = Path("base", HARNESS_PIN)
        if pin.is_symlink():
            raise ValueError("Unsafe harness pin")
        revision = full_sha(pin.read_text().strip())
        return {"repository": HARNESS_REPOSITORY, "revision": revision,
                "pin_source": "base", "pin_revision": full_sha(base)}
    return None


def load_harness_selection(profile, head, base):
    file = Path("harness-selection.json")
    if file.is_symlink():
        raise ValueError("Unsafe harness source selection")
    with file.open("rb") as source:
        data = source.read(MAX_HARNESS_SELECTION_BYTES + 1)
    if len(data) > MAX_HARNESS_SELECTION_BYTES:
        raise ValueError("Harness source selection exceeds its byte limit")
    selection = json.loads(data)
    expected = {"repository", "revision", "pin_source", "pin_revision"}
    if profile == "ejm-cli":
        expected.update(("tree", "protected_main"))
    if not isinstance(selection, dict) or set(selection) != expected or selection["repository"] != HARNESS_REPOSITORY:
        raise ValueError("Invalid harness source selection")
    revision = full_sha(selection["revision"])
    if profile == "ejm-cli":
        full_sha(selection["tree"])
        full_sha(selection["protected_main"])
        if selection["pin_source"] != "head" or selection["pin_revision"] != head or harness_pin("product", head) != revision:
            raise ValueError("Harness selection does not match admitted CLI source")
    elif selection != select_harness(profile, head, base, ""):
        raise ValueError("Harness selection does not match trusted IDE base")
    return selection


def archive_harness(selection):
    revision = full_sha(selection["revision"])
    if run("git", "-C", "harness", "rev-parse", "HEAD").decode().strip() != revision:
        raise ValueError("Harness checkout does not match selected source")
    tree = full_sha(run("git", "-C", "harness", "rev-parse", "HEAD^{tree}").decode().strip())
    if selection.get("tree", tree) != tree:
        raise ValueError("Harness checkout tree does not match source authority")
    data = source_archive("harness", revision, "ejm-harness", "tar")
    return data, {**selection, "tree": tree, "archive_sha256": hashlib.sha256(data).hexdigest()}


def main():
    if sys.argv[1:] == ["ui-pin"]:
        with open(os.environ["GITHUB_OUTPUT"], "a") as output:
            output.write("revision=" + ui_revision("base") + "\n")
        return
    head, base = full_sha(os.environ["PR_HEAD"]), full_sha(os.environ["PR_BASE"])
    # Validate proposed metadata without allowing it to select privileged input.
    ui_revision("product")
    repository = os.environ["GITHUB_REPOSITORY"]
    profile = repository.split("/")[-1]
    if len(sys.argv) == 2 and sys.argv[1] == "select-harness":
        if os.environ["PROFILE"] != profile:
            raise ValueError("Harness profile does not match admitted repository")
        selection = select_harness(profile, head, base, os.environ.get("HARNESS_READ_TOKEN", ""))
        if selection is not None:
            with Path("harness-selection.json").open("x") as output:
                output.write(json.dumps(selection))
        with open(os.environ["GITHUB_OUTPUT"], "a") as output:
            output.write("revision=" + (selection["revision"] if selection else "") + "\n")
        return
    if len(sys.argv) != 1:
        raise ValueError("Unknown source preparation operation")
    selection = load_harness_selection(profile, head, base) if profile in ("ejm-cli", "ejm-ide") else None
    out = Path("prepared")
    out.mkdir()
    (out / "product.tar").write_bytes(source_archive("product", head, "product", "tar"))
    context = {"repository": repository, "pr": int(os.environ["PR_NUMBER"]), "head": head, "base": base}
    if selection is not None:
        data, context["harness"] = archive_harness(selection)
        (out / "harness.tar").write_bytes(data)
    ui = ui_revision("base")
    if ui:
        if run("git", "-C", "ejm-ui", "cat-file", "-t", ui).strip() != b"commit":
            raise ValueError("UI source pin must name a commit, not a tree or tag")
        (out / "ui.tar").write_bytes(source_archive("ejm-ui", ui, "ejm-ui", "tar"))
        context["ui_source"] = {"repository": "ejmlabs/ejm-ui", "revision": ui}
    (out / "change.patch").write_bytes(review_diff(repository, base, head, os.environ["GH_TOKEN"]))
    (out / "context.json").write_text(json.dumps(context))


if __name__ == "__main__":
    main()
