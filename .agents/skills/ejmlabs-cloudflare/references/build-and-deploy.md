# Build and deploy

Read this before a deploy, a dry run, or a rollback. The repository's own deploy skill names
its site directory, check script, Worker and approval rule.

## Why a clean build

Sites on ejm-ui link `@ejm/*` packages with `file:` paths to a sibling ejm-ui checkout. That
checkout is a working copy: it is often on a feature branch with uncommitted work, so a build
from it ships code that is not on ejm-ui main. Typical symptoms are typecheck errors in files
your change did not touch, or a bundle that differs from CI. The revision the repository pins
(`.github/ui-source.json` for ejmlabs sites) is the one that was reviewed, so build against that.

The same goes for the site repository: a local checkout may hold another agent's work. Build
the merge commit, not your working tree.

## Before you start

1. The PR merged through the gate (see `ejmlabs-workflow`). Read the merge commit:
   `gh pr view N --json mergeCommit --jq .mergeCommit.oid`.
2. The repository's approval rule allows this deploy. If production deploys are marked
   approval-gated, the user asked for this one.
3. You know the Worker name and config path from the repository's deploy skill or README.

## Sequence

Clone both repositories side by side in a fresh scratch directory, so `../../ejm-ui` from the
site directory resolves to the clean clone. `SITE` is the repository name, `MERGE_SHA` the full
merge commit, and `site` the directory that holds `wrangler.jsonc` (use the repository's own).
A site that does not use ejm-ui skips the ejm-ui clone.

```sh
work=$(mktemp -d)   # or the session scratch directory
git clone https://github.com/ejmlabs/SITE.git "$work/SITE"
git -C "$work/SITE" checkout --detach MERGE_SHA
ui_rev=$(node -p "require('$work/SITE/.github/ui-source.json').revision")
git clone https://github.com/ejmlabs/ejm-ui.git "$work/ejm-ui"
git -C "$work/ejm-ui" checkout --detach "$ui_rev"
(cd "$work/ejm-ui" && npm ci --ignore-scripts)
cd "$work/SITE/site"
npm ci --ignore-scripts
```

Then run the repository's checks in its order and stop at the first failure. For ejmlabs.com:

```sh
npm run typecheck
npm test            # builds dist, then reads every built route
npm run perf        # load-time baseline; needs Chrome
```

A site with a single check script runs that instead (for example `npm run check`). A site that
needs a lifecycle script drops `--ignore-scripts` only for a reason recorded in its docs.

List what will be published and read it:

```sh
find dist -type f | sort
```

Dry run, then deploy, from the site directory:

```sh
export WRANGLER_SEND_METRICS=false
npx wrangler deployments list --config wrangler.jsonc   # note the live version id
npx wrangler deploy --config wrangler.jsonc --dry-run
npx wrangler deploy --config wrangler.jsonc
```

When the config defines environments, name production explicitly with `--env ""` and run a
separate dry run for each environment you touch. A repository that has not pinned wrangler yet
uses a current wrangler 4; say which version ran in the report and propose the pin.

A dry run checks configuration and packaging. It proves nothing about permission or live
behavior. Remove the scratch directory when the live check is done.

## Wrangler prompts and auth

- Agent shells have no TTY. If wrangler stops on a prompt (most often an offer to replace a DNS
  record for the custom domain), stop. Do not script an answer. A DNS prompt means the domain
  mapping differs from the config; report it and give the user the command to run themselves.
- If wrangler is not logged in or the login lacks access, ask the user to run `wrangler login`.
  Never create, paste or store an API token to get past it.

## Report

Record, for each deploy:

| Field | Example form |
| --- | --- |
| Site commit | full merge SHA |
| ejm-ui revision | full SHA from the pin, when the site uses ejm-ui |
| Wrangler version | from `npx wrangler --version` |
| Previous version id | from `wrangler deployments list` before the deploy |
| New version id | the `Current Version ID` line wrangler prints |
| Checks | each command that ran, and its result |
| Live check | what you looked at, and how (see [verify.md](verify.md)) |

## Rollback

A Worker version holds its code, static assets, bindings and compatibility settings. It does
not hold R2, D1 or KV data.

1. Pick the target: the previous version id from the last report, or the last version you saw
   working live. Confirm it with `npx wrangler versions list --config wrangler.jsonc`.
2. Get the same authorization a deploy needs.
3. Roll back, with a message that says why:

   ```sh
   npx wrangler rollback VERSION_ID --config wrangler.jsonc --message "Restore VERSION_ID: REASON"
   ```

4. Repeat the live check on the affected routes and headers.
5. Land a revert of the bad change on main through the normal PR gate, then deploy it. Until
   that deploy, main and live differ; say so in the report.

Rollback fails if a binding the old version used was deleted. For bad data in R2, D1 or KV,
restore through that service's owning procedure (for release manifests, see
[release-metadata.md](release-metadata.md)). For a bad setting (WAF, Access, routes), restore
only that setting from its reviewed before snapshot. Never loosen WAF, Access or storage
exposure to recover a service.
