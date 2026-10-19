#!/usr/bin/env python3
"""Trusted, commit-bound admission and approval for ejmlabs repositories."""
import json
import os
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path

REPOS = {"ejmlabs/" + name for name in (".github", "ejmlabs", "ejm-cli", "ejm-harness", "ejm-ide")}
CONTEXTS = ("Human readiness", "Maintainer approval", "Quality", "Repository standards", "AI review")
COMMAND = re.compile(r"^/(ready|approve|revoke) ([0-9a-f]{40})$")


def command(text):
    match = COMMAND.fullmatch(text.strip())
    if not match:
        raise ValueError("Use /ready, /approve, or /revoke followed by the full 40-character head SHA")
    return match.groups()


def human(user, permission):
    return user.get("type") == "User" and permission in {"admin", "maintain"}


def current_pr(pr, sha):
    return pr["state"] == "open" and not pr.get("draft") and pr["base"]["ref"] == "main" and pr["head"]["sha"] == sha


class GitHub:
    def __init__(self):
        self.repo = os.environ["GITHUB_REPOSITORY"]
        if self.repo not in REPOS:
            raise ValueError("Repository has not adopted the ejmlabs policy")
        self.token = os.environ["GH_TOKEN"]
        self.root = "https://api.github.com/repos/" + self.repo

    def request(self, path, method="GET", data=None):
        url = path if path.startswith("https://api.github.com/") else self.root + path
        body = None if data is None else json.dumps(data).encode()
        req = urllib.request.Request(url, data=body, method=method, headers={
            "Authorization": "Bearer " + self.token, "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28", "Content-Type": "application/json",
        })
        with urllib.request.urlopen(req, timeout=30) as response:
            return json.loads(response.read() or "null")

    def pages(self, path, limit=10):
        result = []
        for page in range(1, limit + 1):
            batch = self.request(path + ("&" if "?" in path else "?") + f"per_page=100&page={page}")
            result.extend(batch)
            if len(batch) < 100:
                return result
        raise ValueError("Metadata limit exceeded; maintainer triage is required")

    def permission(self, login):
        if not re.fullmatch(r"[A-Za-z0-9-]+", login):
            return "none"
        role = self.request(f"/collaborators/{login}/permission")
        return role.get("role_name", role["permission"])

    def statuses(self, sha):
        result = {}
        for status in self.pages(f"/commits/{sha}/statuses"):
            result.setdefault(status["context"], status)
        return result

    def status(self, sha, context, state, description):
        self.request(f"/statuses/{sha}", "POST", {
            "context": context, "state": state, "description": description[:140],
            "target_url": f"https://github.com/{self.repo}/actions/runs/{os.environ['GITHUB_RUN_ID']}",
        })


def outputs(**items):
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as out:
        for key, value in items.items():
            out.write(f"{key}={value}\n")


def first_attempt():
    if os.environ.get("GITHUB_RUN_ATTEMPT", "1") != "1":
        raise ValueError("Workflow reruns cannot replay admission; use a fresh human command after revocation")


def live_decision(api, number, sha, comment_id, kind):
    comment = api.request(f"/issues/comments/{int(comment_id)}")
    if comment["issue_url"] != f"https://api.github.com/repos/{api.repo}/issues/{int(number)}":
        raise ValueError("Human decision belongs to another PR")
    if command(comment["body"]) != (kind, sha) or not human(comment["user"], api.permission(comment["user"]["login"])):
        raise ValueError("Human decision is no longer a current human maintainer command")
    if comment["created_at"] != comment["updated_at"]:
        raise ValueError("Human decision was edited; use a fresh comment")
    return comment


def live_ready(api, number, sha, comment_id):
    first_attempt()
    pr = api.request(f"/pulls/{number}")
    if not current_pr(pr, sha):
        raise ValueError("PR head changed, PR is a draft, or PR no longer targets main")
    live_decision(api, number, sha, comment_id, "ready")
    status = api.statuses(sha).get("Human readiness", {})
    marker = f"[ready:{comment_id}]"
    if status.get("state") != "success" or marker not in status.get("description", ""):
        raise ValueError("Readiness was revoked or superseded")
    return pr


def dismiss_bot_approvals(api, number, sha, reason):
    for review in api.pages(f"/pulls/{number}/reviews"):
        if review["user"]["login"] == "github-actions[bot]" and review["state"] == "APPROVED" and review["commit_id"] == sha:
            path = f"/pulls/{number}/reviews/{review['id']}"
            try:
                api.request(path + "/dismissals", "PUT", {"message": reason})
            except urllib.error.HTTPError as error:
                if error.code not in (403, 422) or api.request(path)["state"] == "APPROVED":
                    raise


def invalidate(api, number, sha, reason):
    statuses = api.statuses(sha)
    ready = statuses.get("Human readiness", {})
    for context in ("Human readiness", "Maintainer approval"):
        api.status(sha, context, "failure", reason)
    dismiss_bot_approvals(api, number, sha, reason)
    run = re.search(r"\[run:(\d+)\]", ready.get("description", ""))
    if run and run[1] != os.environ["GITHUB_RUN_ID"]:
        try:
            api.request(f"/actions/runs/{run[1]}/cancel", "POST")
        except urllib.error.HTTPError as error:
            if error.code not in (409, 422):
                raise


def admit(api, event):
    outputs(run="false")
    first_attempt()
    issue = event.get("issue", {})
    comment = event.get("comment", {})
    if not issue.get("pull_request"):
        return
    number, comment_id = int(issue["number"]), int(comment["id"])
    pr = api.request(f"/pulls/{number}")
    sha = pr["head"]["sha"]
    if event["action"] in {"edited", "deleted"}:
        statuses = api.statuses(sha)
        if any(f"[{kind}:{comment_id}]" in statuses.get(context, {}).get("description", "")
               for kind, context in (("ready", "Human readiness"), ("approve", "Maintainer approval"))):
            invalidate(api, number, sha, "Human decision edited or deleted; fresh commands required")
        return
    if event["action"] != "created":
        raise ValueError("Unsupported human decision event")
    user = comment["user"]
    sender = event["sender"]
    if not human(sender, api.permission(sender["login"])):
        raise ValueError("Only a current human maintainer may admit or approve a PR")
    if sender["login"] != user["login"] or not human(user, api.permission(user["login"])):
        raise ValueError("A command must be posted by the human maintainer who authorizes it")
    action, requested = command(comment["body"])
    if not current_pr(pr, requested):
        raise ValueError("Command must name the current non-draft PR head targeting main")
    if action == "revoke":
        invalidate(api, number, sha, "Revoked by a current human maintainer")
        return
    if action == "approve":
        ready = api.statuses(sha).get("Human readiness", {})
        ready_id = re.search(r"\[ready:(\d+)\]", ready.get("description", ""))
        if not ready_id:
            raise ValueError("The current commit must be admitted before approval")
        live_ready(api, number, sha, ready_id[1])
        approval = live_decision(api, number, sha, comment_id, "approve")
        if approval["body"] != comment["body"] or any(approval["user"][key] != user[key] for key in ("id", "login", "type")):
            raise ValueError("Approval no longer matches the human event author and body")
        api.status(sha, "Maintainer approval", "pending", f"[approve:{comment_id}] Recording current human approval")
        try:
            api.request(f"/pulls/{number}/reviews", "POST", {
                "commit_id": sha, "event": "APPROVE",
                "body": f"Explicit human maintainer approval by @{user['login']} in comment {comment['html_url']} for `{sha}`. Required checks still apply.",
            })
            live_ready(api, number, sha, ready_id[1])
            if live_decision(api, number, sha, comment_id, "approve")["body"] != comment["body"]:
                raise ValueError("Approval changed while its native review was recorded")
            api.status(sha, "Maintainer approval", "success", f"[approve:{comment_id}] Human maintainer approved this commit")
        except (ValueError, KeyError, urllib.error.URLError):
            api.status(sha, "Maintainer approval", "failure", "Human approval could not be recorded; fresh command required")
            dismiss_bot_approvals(api, number, sha, "Human approval could not be recorded")
            raise
        return
    history = api.pages(f"/commits/{sha}/statuses")
    if any(f"[ready:{comment_id}]" in status.get("description", "") for status in history):
        raise ValueError("This admission has already been consumed")
    if api.statuses(sha).get("Human readiness", {}).get("state") == "success":
        raise ValueError("This commit is already admitted; revoke before an explicit fresh run")
    if pr["changed_files"] > 200 or pr["additions"] + pr["deletions"] > 12000:
        raise ValueError("PR exceeds review limits; split it before admission")
    for context in CONTEXTS[1:]:
        api.status(sha, context, "pending", "Waiting for current human approval" if context == "Maintainer approval" else "Admitted checks queued")
    api.status(sha, "Human readiness", "success", f"[ready:{comment_id}] [run:{os.environ['GITHUB_RUN_ID']}] Human admitted this commit")
    outputs(run="true", pr=number, head=sha, base=pr["base"]["sha"], admission=comment_id,
            profile=api.repo.split("/")[1])


def finish(api):
    first_attempt()
    number, sha, admission = os.environ["PR_NUMBER"], os.environ["PR_HEAD"], os.environ["ADMISSION_ID"]
    try:
        live_ready(api, number, sha, admission)
    except (ValueError, KeyError, urllib.error.URLError):
        ready = api.statuses(sha).get("Human readiness", {})
        if f"[ready:{admission}]" in ready.get("description", ""):
            for context in ("Human readiness", "Maintainer approval"):
                api.status(sha, context, "failure", "Human readiness no longer valid; fresh commands required")
            dismiss_bot_approvals(api, number, sha, "Human readiness no longer valid")
        raise
    approval = api.statuses(sha).get("Maintainer approval", {})
    if approval.get("state") == "success":
        try:
            marker = re.search(r"\[approve:(\d+)\]", approval.get("description", ""))
            if not marker:
                raise ValueError("Successful approval is missing its human decision")
            live_decision(api, number, sha, marker[1], "approve")
        except (ValueError, KeyError, urllib.error.URLError):
            api.status(sha, "Maintainer approval", "failure", "Human approval no longer valid; fresh command required")
            dismiss_bot_approvals(api, number, sha, "Human approval no longer valid")
            raise
    results = json.loads(os.environ["JOB_RESULTS"])
    quality = all(results.get(job, {}).get("result") == "success" for job in ("quality", "registry"))
    standards = results.get("standards", {}).get("result") == "success"
    reviews = results.get("ai", {}).get("result") == "success"
    for context, ok in (("Quality", quality), ("Repository standards", standards), ("AI review", reviews)):
        api.status(sha, context, "success" if ok else "failure", "Passed for admitted commit" if ok else "Failed, cancelled, missing, or incomplete")


def main():
    api = GitHub()
    mode = sys.argv[1]
    if mode == "admit":
        admit(api, json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text()))
    elif mode == "verify":
        live_ready(api, os.environ["PR_NUMBER"], os.environ["PR_HEAD"], os.environ["ADMISSION_ID"])
    elif mode == "finish":
        finish(api)
    else:
        raise ValueError("Unknown operation")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, urllib.error.URLError) as error:
        # Do not print untrusted response bodies, comment text, or credentials.
        print(f"Governance stopped: {type(error).__name__}: {error}", file=sys.stderr)
        sys.exit(1)
