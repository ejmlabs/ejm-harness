# Release metadata and downloads

Read this before you change a download page, a release list, a download Worker, or anything
that makes a file appear on a site. The owning product repository's release skill decides what
is released; this file covers how a Cloudflare site offers it.

## Pick the smallest form

House default: a static release list. Use a download Worker with private R2 only when Cloudflare
must serve the files themselves.

| Form                                                                                     | Used by                                                   | Runtime                            |
| ---------------------------------------------------------------------------------------- | --------------------------------------------------------- | ---------------------------------- |
| Static list: a reviewed JSON file built into the site, pointing at GitHub release assets | ejmlabs.com `/download/`, `/install.sh`, `/releases.json` | none; the site stores no installer |
| Download Worker: release metadata and installers served from a private R2 bucket         | ide.ejmlabs.com `/api/releases`, `/downloads/*`           | Worker plus R2                     |

## Static release list

- One file lists every released file: product, version, target, a versioned HTTPS release-asset
  URL on the repository that owns the product, size and SHA-256.
- The build validates it and stops on a bad list: HTTPS only, a digest on every entry, no
  duplicate target, one version per product, and the asset URL on the owning repository.
- Everything that reads the list (the download page, the install script, project pages) reads it
  at build time from that one file. Never keep a second list.
- The list is empty until a release is published. An empty list offers no file.
- Change it only in a reviewed change deployed from main. Never replace a listed file; publish a
  new version instead.
- The install script and the checksum it checks share an origin, so a deploy is the control:
  deploy only from main at a named revision after a fresh security pass. Owner sign-in stays
  behind MFA, with no long-lived deploy credential in CI.
- After the deploy, check that `/install.sh` and `/releases.json` match the reviewed build, that
  each URL is a versioned release asset of its repository, and that the page offers only files
  that exist.

## Download Worker on private R2

- The bucket stays private: no `r2.dev` access and no public custom domain. The Worker reaches it
  through a binding. Inspect exposure and CORS settings, never object contents, to check this.
- A private manifest (for example `releases/latest.json`) is the only pointer to the current
  release. Its empty value (`version: null`, no artifacts) means no release.
- The download route reads the current manifest and requires the exact digest, so cached
  metadata can never select bytes from another release.

| Contract              | Required behavior                                                                                                                                                                                                        |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Routes                | Metadata at `/api/releases`; installers at `/downloads/{target}?sha256={sha256}`                                                                                                                                         |
| Methods               | `GET` and `HEAD`                                                                                                                                                                                                         |
| Digest                | The URL carries the exact artifact digest, which is part of the cache key. Missing, stale or mismatched: non-cacheable `409`. Unreleased target: `404`. Listed but missing or wrong-size object: `503` with a retry hint |
| Cache                 | Metadata 30 seconds, digest-bound downloads 60 seconds, errors `no-store`                                                                                                                                                |
| Ranges and validators | R2 handles them; keep `206`, `304`, `412` and `416`. Ignored ranges and `If-Range` fall back to a full `200`                                                                                                             |
| Streaming             | The Worker streams from R2 and never buffers an installer                                                                                                                                                                |

- Route only those paths to the Worker (`run_worker_first`); static assets serve the rest.
- Browser challenges never cover these routes. Check them with `curl -sSI` after a deploy.

### Publishing

Publish with the repository's publisher script, which dry-runs by default. It must, in order:

1. Snapshot every local installer and verify each snapshot's size and SHA-256 before any upload.
2. Upload each snapshot under a content-addressed key, for example
   `releases/{version}/{sha256}/{filename}`.
3. Download each uploaded object and verify its digest and size.
4. Upload a manifest archive under its own digest.
5. Upload the current manifest last.

A failure before the last step leaves the previous release live. Never overwrite a
content-addressed object with different bytes. Never edit the current manifest by hand.

### Fixtures and empty releases

- Use local R2 (`wrangler dev --local`) and disposable fixtures for tests. Never upload a fixture
  or a fake release to the remote bucket.
- An empty manifest leaves real digest, range and conditional acceptance unproven. Say so in the
  report. Do not fill the gap with a test upload.

## Rollback

- Static list: revert the list change on main through the normal PR gate and deploy it, or roll
  the Worker back to the last verified version (see [build-and-deploy.md](build-and-deploy.md)).
  A released file is never replaced; a withdrawn release says why in its notes and gets a new
  version.
- Download Worker: Worker rollback does not touch R2. Retrieve a verified archived manifest,
  validate it with the repository's parser, confirm every installer it names still exists, and
  upload that exact JSON as the current manifest with wrangler. The site picks it up within the
  metadata cache lifetime. Old digest-bound links may return `409`; a page refresh gets current
  links.
- Keep earlier installers and manifest archives until the retention policy allows removal.
