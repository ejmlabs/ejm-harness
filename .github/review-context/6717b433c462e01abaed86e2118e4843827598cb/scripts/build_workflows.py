#!/usr/bin/env python3
"""Embed trusted scripts in reusable workflows so private callers need no read token."""
import json
import base64
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CHECKOUT = "actions/checkout@11d5960a326750d5838078e36cf38b85af677262"
NODE = "actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020"
UV = "astral-sh/setup-uv@d0cc045d04ccac9d8b7881df0226f9e82c39688e"
UPLOAD = "actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02"
DOWNLOAD = "actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093"
CODEX = "openai/codex-action@bdf19a4a223ec2549a3e2274a0cf61556bc07675"


def yaml(value, indent=0):
    """Emit the limited maps, lists and strings used here, without dependencies."""
    pad = " " * indent
    if isinstance(value, dict):
        lines = []
        for key, item in value.items():
            name = json.dumps(key)
            if isinstance(item, (dict, list)) and item:
                lines.append(pad + name + ":\n" + yaml(item, indent + 2))
            elif isinstance(item, str) and "\n" in item:
                lines.append(pad + name + ": |\n" + "\n".join(" " * (indent + 2) + line if line else "" for line in item.rstrip().splitlines()))
            else:
                lines.append(pad + name + ": " + json.dumps(item))
        return "\n".join(lines)
    if isinstance(value, list):
        return "\n".join(pad + "-\n" + yaml(item, indent + 2) if isinstance(item, dict) else pad + "- " + json.dumps(item) for item in value)
    raise TypeError(type(value))


def source(name, library=False):
    text = (ROOT / "scripts" / name).read_text()
    return text.split('\nif __name__ == "__main__":')[0] if library else text


def python(text, args=""):
    # GitHub evaluates expressions even inside quoted script strings. Encoding the
    # trusted library preserves literal consumer expressions without evaluating them.
    encoded = base64.b64encode(text.encode()).decode()
    return "python3 - " + args + " <<'TRUSTED_PYTHON'\nimport base64\nexec(compile(base64.b64decode('" + encoded + "'), 'trusted-workflow.py', 'exec'))\nTRUSTED_PYTHON\n"


def run(name, script, **extra):
    return {"name": name, "shell": "bash", "run": script, **extra}


def checkout(path, ref, **extra):
    return {"uses": CHECKOUT, "with": {"path": path, "ref": ref, "persist-credentials": False, "fetch-depth": 1, "submodules": False, **extra}}


ENV = {"PR_NUMBER": "${{ needs.admission.outputs.pr }}", "PR_HEAD": "${{ needs.admission.outputs.head }}", "PR_BASE": "${{ needs.admission.outputs.base }}", "ADMISSION_ID": "${{ needs.admission.outputs.admission }}", "GH_TOKEN": "${{ github.token }}"}
VERIFY = run("Recheck current human admission", python(source("governance.py"), "verify"))
ARTIFACT = {"uses": DOWNLOAD, "with": {"artifact-ids": "${{ needs.prepare.outputs.artifact }}", "merge-multiple": True, "path": "prepared"}}
EXTRACT = run("Extract source data safely", """python3 - <<'PY'
import tarfile
from pathlib import Path
for file in Path('prepared').glob('*.tar'):
    with tarfile.open(file) as archive:
        archive.extractall('.', filter='data')
PY
""")


def governance():
    templates = {path.name: path.read_text() for path in sorted((ROOT / "templates/community").glob("*.md"))}
    standards_check = source("standards.py", True) + "\nstandards(Path(sys.argv[1]), sys.argv[2], " + repr(templates) + ")\n"
    admission = {
        "if": "${{ github.run_attempt == 1 && github.event_name == 'issue_comment' && github.event.issue.pull_request && contains(fromJSON('[\"OWNER\",\"MEMBER\",\"COLLABORATOR\"]'), github.event.comment.author_association) && ((github.event.action == 'created' && github.event.sender.type == 'User' && (startsWith(github.event.comment.body, '/ready ') || startsWith(github.event.comment.body, '/approve ') || startsWith(github.event.comment.body, '/revoke '))) || (github.event.action != 'created' && (startsWith(github.event.comment.body, '/ready ') || startsWith(github.event.comment.body, '/approve ') || startsWith(github.event.changes.body.from, '/ready ') || startsWith(github.event.changes.body.from, '/approve ')))) }}",
        "runs-on": "ubuntu-24.04", "timeout-minutes": 3,
        "permissions": {"contents": "read", "pull-requests": "write", "statuses": "write", "actions": "write"},
        "concurrency": {"group": "human-command-${{ github.repository }}-${{ github.event.issue.number }}", "cancel-in-progress": False},
        "outputs": {key: "${{ steps.admit.outputs." + key + " }}" for key in ("run", "pr", "head", "base", "admission", "profile")},
        "steps": [run("Validate live maintainer and commit; consume once", python(source("governance.py"), "admit"), id="admit", env={"GH_TOKEN": "${{ github.token }}"})],
    }
    prepare = {
        "needs": "admission", "if": "${{ github.run_attempt == 1 && needs.admission.outputs.run == 'true' }}",
        "runs-on": "ubuntu-24.04", "timeout-minutes": 5, "permissions": {"contents": "read", "pull-requests": "read", "statuses": "read"}, "env": ENV,
        "outputs": {"artifact": "${{ steps.sources.outputs.artifact-id }}"},
        "steps": [VERIFY, checkout("product", "${{ needs.admission.outputs.head }}"), checkout("base", "${{ needs.admission.outputs.base }}"),
                  run("Select the admitted harness dependency against trusted source authority", python(source("prepare.py"), "select-harness"),
                      id="harness", env={"PROFILE": "${{ needs.admission.outputs.profile }}", "HARNESS_READ_TOKEN": "${{ secrets.EJM_HARNESS_TOKEN || github.token }}"}),
                  {"uses": CHECKOUT, "if": "${{ steps.harness.outputs.revision != '' }}", "with": {"repository": "ejmlabs/ejm-harness", "ref": "${{ steps.harness.outputs.revision }}", "path": "harness", "persist-credentials": False, "token": "${{ secrets.EJM_HARNESS_TOKEN || github.token }}"}},
                  run("Read optional public UI dependency from trusted main", python(source("prepare.py"), "ui-pin"), id="ui"),
                  {"uses": CHECKOUT, "if": "${{ steps.ui.outputs.revision != '' }}", "with": {"repository": "ejmlabs/ejm-ui", "ref": "${{ steps.ui.outputs.revision }}", "path": "ejm-ui", "persist-credentials": False, "fetch-depth": 1, "submodules": False}},
                  run("Archive source and bounded diff", python(source("prepare.py"))), VERIFY,
                  {"uses": UPLOAD, "id": "sources", "with": {"name": "admitted-sources", "path": "prepared/", "retention-days": 3, "if-no-files-found": "error"}}],
    }
    quality = {
        "needs": ["admission", "prepare"], "if": "${{ github.run_attempt == 1 }}", "runs-on": "${{ matrix.os }}", "timeout-minutes": 45, "permissions": {},
        "strategy": {"fail-fast": False, "max-parallel": 2, "matrix": {"os": "${{ fromJSON(needs.admission.outputs.profile == 'ejm-harness' && '[\"ubuntu-24.04\",\"macos-15\",\"windows-2025\"]' || needs.admission.outputs.profile == 'ejm-cli' && '[\"ubuntu-24.04\",\"macos-15\"]' || needs.admission.outputs.profile == 'ejm-ide' && '[\"macos-15\"]' || '[\"ubuntu-24.04\"]') }}"}},
        "env": {"CARGO_BUILD_JOBS": "2", "CARGO_INCREMENTAL": "0", "PROFILE": "${{ needs.admission.outputs.profile }}", "CI": "true"},
        "steps": [ARTIFACT, EXTRACT, {"uses": NODE, "with": {"node-version": "24"}}, {"uses": UV, "with": {"version": "0.9.26"}},
                  run("Install Linux harness keyring build prerequisites", """sudo apt-get update
sudo apt-get install --yes --no-install-recommends pkg-config libdbus-1-dev
""", **{"if": "${{ needs.admission.outputs.profile == 'ejm-harness' && runner.os == 'Linux' }}"}),
                  run("Install pinned Rust tools", """if [[ "$PROFILE" == ejm-* ]]; then
  rustup toolchain install 1.99.1 --profile minimal --component clippy,rustfmt
  rustup default 1.99.1
  if [[ "$PROFILE" != ejm-ui ]]; then
    cargo install cargo-deny --version 0.20.2 --locked
    cargo install cargo-machete --version 0.9.2 --locked
  fi
fi
"""),
                  {"name": "Prepare ejm-cli remote test prerequisites", "if": "${{ needs.admission.outputs.profile == 'ejm-cli' }}", "uses": "./product/.github/actions/setup-ci"},
                  run("Run the project's required gate without secrets or write tokens", """cd product
if [[ -d ../ejm-ui ]]; then
  npm --prefix ../ejm-ui ci --ignore-scripts
fi
case "$PROFILE" in
  ejm-cli) ./scripts/check ;;
  ejm-harness) npm ci --ignore-scripts; npm --prefix registry ci --ignore-scripts; ./scripts/check ;;
  ejm-ide) npm ci --ignore-scripts; npm run verify; npm run site:check; npm run site:build ;;
  ejm-ui)
    export npm_config_ignore_scripts=true
    npm ci --ignore-scripts
    npm rebuild node-pty --ignore-scripts=false
    npm run check
    npm run build
    node scripts/size.mjs
    npm run size -w @ejm/demos
    npm run check:rust
    node scripts/pack-proof.mjs
    ;;
  ejmlabs) node scripts/check.mjs; npm --prefix site test; npm --prefix site run build ;;
  .github) python3 -m unittest discover -s tests -v; python3 scripts/build_workflows.py --check ;;
  *) exit 1 ;;
esac
""")],
    }
    registry = {
        "needs": ["admission", "prepare"], "if": "${{ github.run_attempt == 1 }}", "runs-on": "ubuntu-24.04", "timeout-minutes": 25, "permissions": {},
        "env": {"CARGO_BUILD_JOBS": "2", "CARGO_INCREMENTAL": "0", "PROFILE": "${{ needs.admission.outputs.profile }}"},
        "steps": [ARTIFACT, EXTRACT, {"uses": NODE, "if": "${{ needs.admission.outputs.profile == 'ejm-harness' }}", "with": {"node-version": "24"}},
                  run("Run separate registry gate when applicable", """if [[ "$PROFILE" == ejm-harness ]]; then
  rustup toolchain install 1.99.1 --profile minimal --component clippy,rustfmt --target wasm32-unknown-unknown
  rustup default 1.99.1
  cargo install cargo-deny --version 0.20.2 --locked
  cargo install cargo-machete --version 0.9.2 --locked
  cargo install worker-build --version 0.8.7 --locked --jobs 2
  cd product
  if [[ -d ../ejm-ui ]]; then
    npm --prefix ../ejm-ui ci --ignore-scripts
  fi
  npm ci --ignore-scripts
  npm --prefix registry ci --ignore-scripts
  ./scripts/check-registry
fi
""")],
    }
    standards = {
        "needs": ["admission", "prepare"], "if": "${{ github.run_attempt == 1 }}", "runs-on": "ubuntu-24.04", "timeout-minutes": 3, "permissions": {},
        "env": {"PROFILE": "${{ needs.admission.outputs.profile }}"},
        "steps": [ARTIFACT, EXTRACT, run("Validate shared public standards and version metadata", python(standards_check, 'product "$PROFILE"'))],
    }
    prompt = (ROOT / "review/prompt.md").read_text()
    schema = (ROOT / "review/schema.json").read_text()
    ai = {
        "needs": ["admission", "prepare"], "if": "${{ github.run_attempt == 1 }}", "runs-on": "ubuntu-24.04", "timeout-minutes": 12,
        "permissions": {"contents": "read", "pull-requests": "read", "statuses": "read"}, "env": ENV,
        "strategy": {"fail-fast": False, "max-parallel": 1, "matrix": {"pass": [1, 2]}},
        "steps": [VERIFY, ARTIFACT,
                  run("Prepare trusted reviewer directory and isolated configuration", """python3 - <<'PY'
import json, os, tarfile
from pathlib import Path
root = Path('reviewer/review-data')
root.mkdir(parents=True)
for file in sorted(Path('prepared').glob('*.tar')):
    with tarfile.open(file) as archive: archive.extractall(root, filter='data')
for name in ('context.json', 'change.patch'):
    (root / name).write_bytes((Path('prepared') / name).read_bytes())
context = json.loads((root / 'context.json').read_text())
context['pass'] = int(os.environ['REVIEW_PASS'])
(root / 'context.json').write_text(json.dumps(context))
Path(os.environ['RUNNER_TEMP'], 'review-home').mkdir()
if os.environ['HAS_MODEL_KEY'] != 'true': raise ValueError('OPENAI_API_KEY is required; review fails closed')
PY
""", env={"REVIEW_PASS": "${{ matrix.pass }}", "HAS_MODEL_KEY": "${{ secrets.OPENAI_API_KEY != '' }}"}),
                  {"name": "Independent Codex review", "uses": CODEX, "timeout-minutes": 9, "env": {"GH_TOKEN": "", "GITHUB_TOKEN": ""}, "with": {
                      "openai-api-key": "${{ secrets.OPENAI_API_KEY }}", "model": "gpt-6.1-sol", "effort": "medium", "codex-version": "0.163.0",
                      "working-directory": "reviewer", "codex-home": "${{ runner.temp }}/review-home", "permission-profile": ":read-only", "safety-strategy": "drop-sudo",
                      "output-file": "${{ github.workspace }}/review-result.json", "output-schema": schema, "prompt": prompt}},
                  run("Validate the exact head, pass, and bounded review output", python(source("review_result.py"), 'review-result.json "$PR_HEAD" "$REVIEW_PASS"'), env={"PR_HEAD": "${{ needs.admission.outputs.head }}", "REVIEW_PASS": "${{ matrix.pass }}"}),
                  {"uses": UPLOAD, "if": "${{ always() }}", "with": {"name": "review-${{ matrix.pass }}", "path": "review-result.json", "if-no-files-found": "ignore", "retention-days": 7}}, VERIFY],
    }
    finish = {"needs": ["admission", "prepare", "quality", "registry", "standards", "ai"],
              "if": "${{ github.run_attempt == 1 && always() && needs.admission.outputs.run == 'true' }}", "runs-on": "ubuntu-24.04", "timeout-minutes": 3,
              "permissions": {"contents": "read", "pull-requests": "write", "statuses": "write"},
              "env": {**ENV, "JOB_RESULTS": "${{ toJSON(needs) }}"},
              "steps": [run("Publish required results only after live admission validation", python(source("governance.py"), "finish"))]}
    return {"name": "Human-admitted PR checks", "on": {"workflow_call": {"secrets": {"OPENAI_API_KEY": {"required": False}, "EJM_HARNESS_TOKEN": {"required": False}}}}, "permissions": {}, "jobs": {"admission": admission, "prepare": prepare, "quality": quality, "registry": registry, "standards": standards, "ai": ai, "finish": finish}}


def release_workflow():
    inputs = {"commit": {"required": True, "type": "string"}, "version": {"required": True, "type": "string"}, "standards-ref": {"required": True, "type": "string"}}
    # Embed libraries, not their command entry points, before the release entry point.
    code = source("governance.py", True) + "\n" + source("standards.py", True) + "\n" + source("prepare.py", True) + "\n" + source("release.py")
    return {"name": "Checked draft source release", "on": {"workflow_call": {"inputs": inputs}}, "permissions": {},
            "concurrency": {"group": "draft-release-${{ github.repository }}", "cancel-in-progress": False},
            "jobs": {"draft": {"if": "${{ github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main' }}", "runs-on": "ubuntu-24.04", "timeout-minutes": 8,
                               "permissions": {"contents": "write", "pull-requests": "read", "statuses": "read"},
                               "env": {"GH_TOKEN": "${{ github.token }}", "RELEASE_COMMIT": "${{ inputs.commit }}", "RELEASE_VERSION": "${{ inputs.version }}", "STANDARDS_REF": "${{ inputs.standards-ref }}"},
                               "steps": [checkout("product", "${{ inputs.commit }}"), run("Validate protected source and prepare assets without executing project code", python(code)),
                                         run("Create a new tag and draft release without replacing anything", """python3 - <<'PY'
import json, os, subprocess
from pathlib import Path
manifest = json.loads(Path('release-assets/provenance.json').read_text())
tag = 'v' + manifest['version']
repo = os.environ['GITHUB_REPOSITORY']
# VERSION is already validated; hyphens in build metadata do not mark prereleases.
prerelease_flags = ['--prerelease', '--latest=false'] if '-' in manifest['version'].split('+', 1)[0] else []
# No --force, --clobber, or implicit existing-tag acceptance.
subprocess.run(['gh', 'api', '--method', 'POST', 'repos/' + repo + '/git/refs', '-f', 'ref=refs/tags/' + tag, '-f', 'sha=' + manifest['commit']], check=True)
subprocess.run(['gh', 'release', 'create', tag, '--repo', repo, '--verify-tag', '--draft', '--title', tag, '--notes-file', 'release-notes.md', *prerelease_flags, *[str(p) for p in sorted(Path('release-assets').iterdir())]], check=True)
PY
""")]}}}


def main():
    for name, data in (("governance.yml", governance()), ("draft-release.yml", release_workflow())):
        path = ROOT / ".github/workflows" / name
        expected = "# Generated by scripts/build_workflows.py. Edit scripts and regenerate.\n" + yaml(data) + "\n"
        if "--check" in sys.argv:
            if not path.exists() or path.read_text() != expected:
                raise SystemExit(f"Stale generated workflow: {path}")
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(expected)


if __name__ == "__main__":
    main()
