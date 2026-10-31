# The PR gate

The gate decides which pull requests spend CI minutes. Opening a PR is the request. A draft
spends nothing.

## Admission

| Who                                                | How checks start                                         |
| -------------------------------------------------- | -------------------------------------------------------- |
| Maintainer author, same-repo, non-draft            | Admitted on open, push and Ready                         |
| Bot or non-maintainer author, same-repo, non-draft | A maintainer adds `gate:approve`: it admits and approves |
| Fork                                               | Never admitted; see below                                |
| Draft                                              | Never admitted                                           |

- Open the PR non-draft once local checks pass. For a maintainer's same-repo PR that is the
  admission, and the user's request covers it. Draft is for unfinished work or when the user asks.
- Marking a user's draft Ready is a human action. Ask.
- A fork's token is read-only, so the gate cannot admit it. A maintainer who has reviewed outside
  work pushes it to a branch in the repository, and the gate runs there.
- Comments never admit or approve. Only the label and Ready events count.

## Labels

Exact label name equality, added by a current human maintainer. Agents add these only when the
user explicitly asks.

| Label          | Effect                                                                          |
| -------------- | ------------------------------------------------------------------------------- |
| `gate:approve` | Records a bot APPROVE for the current head, admits it if needed, removes label  |
| `gate:revoke`  | Stops the head: cancels this gate's runs, dismisses bot approvals, fails checks |

```sh
gh pr edit 41 --repo ejmlabs/REPO --add-label gate:approve
gh pr edit 41 --repo ejmlabs/REPO --add-label gate:revoke
```

Converting a PR to draft stops it the same way as `gate:revoke`.

## Rules

- The approval binds to one head commit. Push the final commit first; a new commit needs a new
  approval. A maintainer's PR is admitted again by the push itself.
- No size cap. Keep PRs small because small PRs review and revert well.
- Rerun a failed job once for an infrastructure failure. A rerun keeps the admission and
  re-checks the live head. For a code failure, fix it in a new commit. Never rerun to hide a
  flaky test.
- A notice such as "head moved" or "already admitted" is not a failure. A newer event handles
  the new head, and a head that already passed is not run twice.
- When `main` moves and the PR is behind, update the branch (`gh pr update-branch N`). That
  makes a new head commit.
- With strict up-to-date checks, land one PR per repository at a time. Every merge puts the
  others behind.
- A consumer's pin move, caller form change and community file resync are one PR. The new pin's
  standards check that PR.

## Required statuses

**Quality** and **Repository standards**, plus one approving GitHub review (`enforce_admins`).
Read the PR's checks for the current set.

- Repository standards checks community files and versions against the pinned shared standard.
  Never edit synced files by hand; resync them from `ejmlabs/.github`.

## Merge

```sh
gh pr checks N --repo ejmlabs/REPO
gh pr merge N --repo ejmlabs/REPO --squash --match-head-commit FULL_SHA
```

Merge only when every check is green. Never merge as an administrator around a failing or
missing check. When the user asks to ship before the approval exists, arm auto-merge instead:
`gh pr merge N --auto --squash --match-head-commit FULL_SHA`. It lands the PR once the human
approval is recorded and the required checks are green.
