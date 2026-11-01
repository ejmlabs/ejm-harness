# CI deploys

Read this in a repository with a `.github/workflows/deploy.yml` caller: before you merge a change
that ships, while you follow a deploy, and when one fails.

This file is synced from `ejmlabs/.github` at the commit this repository's callers pin, and the
Repository standards check holds the copy to that commit. So it describes the deploy this
repository runs now. The repository's own docs add only its site facts: the Worker names,
hostnames, config path, the checks to run before a merge and what to look at live. They link
here for the rest instead of restating it.

## What a push to main runs

`deploy.yml` calls the shared `deploy-site.yml`, pinned to the same `ejmlabs/.github` commit as
`pr.yml` and `release.yml`. Never edit it by hand: the standards check requires the exact
canonical text except for the pin. Each push to main runs three jobs in order.

1. `build` checks out that main commit, and ejm-ui at the revision in `.github/ui-source.json`
   when the site uses it. With `contents: read` and no secrets, it runs the repository's
   executable `.github/site-build` if there is one, else the site's commands in `SITES` in
   `scripts/deploy.py` of `ejmlabs/.github`. It uploads the Wrangler config and the build output
   as one artifact.
2. `staging` deploys that artifact with `wrangler deploy --env staging`.
3. `production` runs once staging succeeds and deploys the same files with the top-level config.

Two human decisions ship a change. The merge (`gate:approve` and the required checks) sends it
to staging. A maintainer releases it to production: the `production` environment requires a
maintainer as a reviewer, so each production job waits for review until one approves it under
Review deployments. Release the newest run. A production job deploys only while its commit is
still main's head, so an older run that is approved late fails instead of rolling production
back. Approving is a human action: an agent approves a deployment only when the user explicitly
asks, and only after the check in [Production ahead of main](#production-ahead-of-main). An
agent never changes an environment to skip a review.

Each job queues per repository in push order. A deploy that has started is never cancelled, so
Wrangler never stops midway. A newer main commit replaces an older one that is still waiting, so
the latest merge is the one that deploys.

## What a deploy refuses

A deploy job trusts nothing in the artifact. It checks out the reviewed commit itself and stops
before Wrangler runs when:

- the `staging` or `production` environment is missing, or its deployment branch policy is
  anything other than exactly one branch, `main`;
- `production` has no required reviewer, or the job's commit is no longer main's head;
- the artifact's Wrangler config differs from the reviewed commit by a single byte (for ejm-ide,
  the whole `distribution` directory, which holds its Worker). Change the config in source, never
  in a build step;
- the config names another Cloudflare account or declares a Wrangler `build` command, its
  `assets.directory` is outside the site's declared output, or its `main` is not committed source;
- `env.staging` lacks its own Worker name and routes. Without them, `--env staging` would deploy
  to the production custom domain.

Wrangler runs from the lockfile in `deploy/` of `ejmlabs/.github` at the pin, never from the
repository's own `wrangler` dependency. Read the version from the banner in the deploy log. It
gets the `CLOUDFLARE_API_TOKEN` secret of the job's environment and no GitHub token.

## Before the merge

A merge to main deploys, so finish the checks first. Run the repository's checks from a clean
build (see [build-and-deploy.md](build-and-deploy.md)), read the list of files that will be
published, and run a Wrangler dry run for each environment the change touches. Run it from the
directory that holds the config, with the Wrangler the repository pins; a repository without a
pin uses a current Wrangler 4 and says which version ran:

```sh
npx --no-install wrangler deploy --config wrangler.jsonc --env "" --dry-run
npx --no-install wrangler deploy --config wrangler.jsonc --env staging --dry-run
```

A dry run checks configuration and packaging. It proves nothing about permission or live
behavior.

## After the merge

Never run `wrangler deploy` from a workstation while the caller exists. Follow the run:

```sh
gh run list --workflow deploy.yml --branch main --limit 3
gh run watch RUN_ID --exit-status
```

When the staging job finishes, check staging live the way [verify.md](verify.md) and the
repository's docs say, then production once its job finishes. Record the merge SHA, the ejm-ui
pin when the site has one, the run URL, and the `Current Version ID` each deploy job prints.

## Production ahead of main

A deploy by hand from a branch leaves production serving a build that main does not have, and the
next production job from main rolls it back. Before you approve a production job, confirm which
main build is live:

1. Find the last production job on main that succeeded
   (`gh api repos/ejmlabs/REPO/deployments?environment=production`, then each deployment's
   statuses), and read its commit and the `Current Version ID` in its log.
2. Compare that id with the Worker's active version: `npx wrangler deployments list --config
   wrangler.jsonc` with the owner's login, or the Worker's Deployments tab. The same id means
   production is that commit's build. A different id means a deploy ran outside CI: stop and
   report it.
3. Approve only when that commit is an ancestor of the job's commit
   (`git merge-base --is-ancestor`). If production is not a main build, the branch behind it
   lands through the gate first. Never deploy by hand to close the gap.

Matching files is supporting evidence, not proof. Download the build's `site` artifact
(`gh run download RUN_ID -n site`, unpacked as data) and request its hashed assets from
production with curl; they are not challenged. A mismatch proves production differs. A match
cannot rule out a branch that changed the Worker or added files, so when you cannot read the
active version, say it is unconfirmed and leave the approval to the user.

## When a run fails

Read the failing step's log before you act. Rerun a failed job once only for a runner or network
failure.

| Where it stops                                                     | Cause                                                            | What to do                                              |
| ------------------------------------------------------------------ | ---------------------------------------------------------------- | ------------------------------------------------------- |
| `Build the site without credentials`                               | The site does not build at that commit                           | Fix it on main through the gate                         |
| `Require the ... environment's protections`                        | The environment is missing or its branch policy is not `main`    | Report it for the owner setup below; change nothing     |
| `Deploy stopped: ValueError: The production environment must ...`  | Production has no required reviewer                              | Report it for the owner setup below; change nothing     |
| `Deploy stopped: ValueError: Main has moved past this commit ...`  | An older run was approved after a newer commit reached main      | Nothing to fix; release the newest production run       |
| `Deploy stopped: ValueError: CLOUDFLARE_API_TOKEN is not set`      | The environment has no token                                     | Report it for the owner setup below                     |
| `Deploy stopped: ValueError:` with any other message               | A refusal listed above                                           | Fix the config or build in source through the gate      |
| A Wrangler authentication or permission error                      | The token is wrong, expired or lacks a right                     | Report it to the owner; never create or paste a token   |
| Wrangler waits on a prompt, most often about a DNS record          | A DNS record for the hostname exists outside Wrangler            | Stop, and give the owner the hostname and the record    |

Never weaken WAF, bot or Access rules, and never loosen an environment, to get a deploy through.

## Owner setup

The owner does this once per repository, and again to rotate the token. An agent never creates
or changes environments, secrets or tokens.

- `python3 scripts/set_deploy_token.py` in `ejmlabs/.github` asks once for a Cloudflare API
  token and checks it with Cloudflare. It then creates or repairs `staging` and `production` with
  the main-only branch policy in every site repository (or one, with `--repository NAME`) and
  stores the token as `CLOUDFLARE_API_TOKEN` in each. It keeps any reviewer the owner set.
- In Settings, Environments, `production`, the owner adds at least one maintainer under Required
  reviewers and leaves "Prevent self-review" off, so a maintainer can release their own merge.
  Production deploys fail until a reviewer is set. `set_deploy_token.py` and
  `configure_repositories.py` keep the reviewers a human set and warn when there are none.
- The token needs Workers scripts, routes and custom domains on `ejmlabs.com`, plus R2 read for a
  site that binds a bucket.

## Staging names

Staging serves `staging.<production host>`, for example `staging.ejmlabs.com` and
`staging.ui.ejmlabs.com`. Its Worker is by convention the production Worker's name plus
`-staging`. Cloudflare issues the custom domain's certificate, deeper subdomains included. Pick a
hostname that has no DNS record yet: an existing record makes Wrangler prompt, and CI cannot
answer. Staging is public and serves the latest main.

## Roll back

Follow the rollback in [build-and-deploy.md](build-and-deploy.md): redeploy the last verified
Worker version under the same authorization as a deploy, with `--env staging` for the staging
Worker, then land a revert on main. The revert deploys through this same pipeline. Until it does,
main and live differ; say so in the report.

## When the pipeline changes

The pipeline lives in `ejmlabs/.github`. A change there reaches a repository only when a PR moves
its pins. That PR also runs `python3 scripts/sync_agents.py` from the new commit, so this file and
the house rules arrive with the behavior they describe. The Repository standards check fails any
PR whose shared skills or house rules differ from the pinned commit. Never edit a synced copy;
change the template in `ejmlabs/.github`.
