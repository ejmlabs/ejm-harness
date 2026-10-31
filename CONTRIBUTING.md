# Contributing

Start with the [README](README.md) and [development guide](docs/contributing.md). Use small PRs, clear titles, and tests for changed behavior. Follow the existing formatting, architecture, and accessibility rules. If `openspec/` exists, use OpenSpec for a change to a contract other repositories or users rely on, or one that spans several PRs, and archive it in the PR that finishes it. Bug fixes with a regression test, dependency bumps, docs and refactors go straight to a PR.

## Human admission and review

A draft PR runs no checks. Mark the PR Ready for review when the work is ready for them.

A same-repository PR whose author is a current human maintainer, with GitHub **maintain** or **admin** permission, is admitted when it is opened, reopened, pushed or marked Ready. Any other PR in the repository, such as a Dependabot PR, is admitted when a current human maintainer marks it Ready or adds the `gate:approve` label. A fork PR is never admitted, because a fork's token cannot write the required statuses. A maintainer who has reviewed an outside change pushes it to a branch in this repository, and the checks run there. There is no limit on PR size, and each job has a time limit.

Admission, approval and revocation apply to the head commit the event saw. A new commit needs a new admission: the push for a maintainer's own PR, a new Ready or `gate:approve` for any other.

After reviewing that commit, a maintainer leaves an approving GitHub review, or adds `gate:approve` for self-approve under administrator-enforced required reviews. Trusted automation records the label as a bot approval for the current head, admits the head if no run holds it, and removes the label so it can be re-applied after a new push. The `gate:revoke` label, or converting the PR to draft, stops the head's runs and dismisses bot approvals.

The required statuses are **Quality** and **Repository standards**, plus one approving GitHub review. Re-run failed jobs only for an infrastructure failure. Fix review comments and failing checks in a new commit. Do not bypass the requirements as an administrator.

## Changes and reporting

Keep documentation and `CHANGELOG.md` current. Update all release metadata together when changing `VERSION`; see [releases](RELEASING.md). Never add keys, tokens, personal data, local logs, or private customer files. Use fake fixtures. Report vulnerabilities through [Security](SECURITY.md), and follow the [Code of Conduct](CODE_OF_CONDUCT.md).
