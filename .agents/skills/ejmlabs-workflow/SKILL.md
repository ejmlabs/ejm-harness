---
name: ejmlabs-workflow
description: Use at the start of any change in an ejmlabs repository and before you commit, push, open or merge a PR, deploy, release, or hand off. Covers starting safely beside other agents, OpenSpec, branches, the human PR gate, shipping, and reporting.
---

# The ejmlabs workflow

Finish what you start. A requested change is done when it is merged, deployed where the
repository deploys, checked live, and reported honestly.

## Start

- Run `git status`, `git log -5` and `git worktree list` first. Other agents and sessions may be
  editing this repository at the same time.
- Never edit a checkout that holds someone else's uncommitted work or feature branch. When the
  checkout is shared with other work, use a new branch in its own worktree or clean clone cut from
  a fresh `origin/main` (the repository's own worktree command, when it has one). Skip this when
  the session gave you an isolated clone.
- Change only the paths your task needs. Never revert, reformat, or commit changes you did not
  make. Never use bare `git stash` or `git stash pop`; the stash is shared across worktrees.
- Read the repository's AGENTS.md, then each skill whose trigger matches the work.
- In a repository with `openspec/`, use OpenSpec only when a change adds or alters a contract
  other repositories or users rely on (gate behavior, public API, wire or storage format, security
  boundary, new capability) or spans several PRs. Bug fixes with a regression test, dependency
  bumps, docs, copy, refactors and config go straight to a PR. Archive in the PR that finishes
  the change.

## Build

- Follow an existing pattern, or define one and record it before you use it (see
  `ejmlabs-engineering`).
- Run focused checks while editing and the repository's single gate before you hand off.
- When stuck or an error repeats, change the approach. Ask a stronger reviewer if one is
  available, or report the blocker with what you tried and the exact error.

## Commit and PR

- Use the branch the harness assigned; otherwise `<kind>/<slug>` (`feat`, `fix`, `docs`, `deps`,
  `ci`). A repository rule wins.
- Stage files by name. Never `git add -A` or `git add .`. Never commit secrets, local logs,
  build output, or another agent's unfinished work.
- Commit and PR titles are short imperative sentences in sentence case, with no type prefix:
  "Add a retry limit to the sync client". The body says why, and what was verified.
- Never bypass hooks with `--no-verify`. Never push to `main`, force-push a shared branch, or
  rewrite published history.
- Keep PRs small and single-purpose. Fill in the PR template: summary, verification (what ran,
  where, and what did not), and security or compatibility effects.

## The human gate

- Open the PR non-draft once local checks pass. For a maintainer's same-repo PR that is the
  admission, and the user's request covers it. Draft only for unfinished work or when asked.
- `gate:approve`, `gate:revoke`, marking a user's draft Ready, production approvals and releases
  are human actions. Ask; never do them yourself.
- Your stop point is: PR open, checks green, waiting for the human approval. Only when the user
  asks to ship, arm auto-merge: `gh pr merge N --auto --squash --match-head-commit FULL_SHA`.
- A repository AGENTS.md that describes a different gate wins. Details and recovery:
  [references/pr-gate.md](references/pr-gate.md).

## Ship

- When the work is built and checked, carry it through without asking at each step: commit, push,
  PR, then after the human approval merge, deploy, live check. Stop only for a human action, a
  failing check, or an ambiguous product decision.
- Merge only when every check is green, including ones GitHub does not require. Never merge on a
  pending or failed check. Use `gh pr merge N --squash --match-head-commit FULL_SHA`.
- Rerun a failed job once for an infrastructure failure. For a code failure, fix it in a new
  commit. Never rerun to hide a flaky test.
- Deploy only from merged `main`, the way the repository's deploy skill says. Releases (tags,
  package publishing, signed installers) and approval-gated production deploys need the user's
  explicit request. See [references/releases.md](references/releases.md).
- After a merge, delete your branch and any worktree you made.

## Report

- Say what changed, with links to the PR, commit, deploy or release.
- Say what you verified, with which command and on which platform, and what you did not.
- When the user corrects you or sets a standard that will apply again, propose the edit to the
  repository's AGENTS.md or the owning skill in the same task.

## Read next

- [references/pr-gate.md](references/pr-gate.md): admission, labels, statuses, reruns.
- [references/parallel-work.md](references/parallel-work.md): fanning work out to several agents
  and coordinating with sibling repositories.
- [references/releases.md](references/releases.md): versions, changelogs, draft releases, tags.
