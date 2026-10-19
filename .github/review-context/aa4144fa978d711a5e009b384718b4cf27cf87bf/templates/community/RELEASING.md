# Releases

Each repository has an independent [Semantic Version](https://semver.org/) in `VERSION`. Update it with matching package metadata and a dated section in `CHANGELOG.md` in a reviewed PR. Use MAJOR for breaking changes, MINOR for compatible features, and PATCH for compatible fixes. Before 1.0, a MINOR release may break the API; a PATCH may not. Use `-alpha.1`, `-beta.1`, or `-rc.1` for prereleases. SDKs in ejm-harness share its version. A dependency version is pinned separately from the consumer's version.

## Prepare a release

1. Complete the required checks and human approval on the release PR. Include upgrade notes for breaking changes and supported platform evidence.
2. Merge it through protected `main`. Copy the full resulting main commit SHA.
3. Run **Draft release** from `main`, supplying that commit and its `VERSION` without a `v` prefix. Only a current human maintainer may run it.
4. The shared pipeline validates versions, dated notes, main ancestry, the merged PR and its required checks. It refuses an existing tag or release. It creates a new immutable `vVERSION` tag and a **draft** GitHub release with source archives, SHA-256 checksums, and a provenance manifest.
5. Review the draft, source contents, checksums, notes, and any required security disclosure before explicit publication.

Setup creates no product tag or release. Source archives are not signed installers or a package registry publication. CLI/IDE archives need the exact harness revision recorded in `.github/harness-revision`; fetch that dependency separately. Native binaries need platform builds, code signing/notarization where relevant, and smoke tests. npm, PyPI, crates.io, and deployment publishing require separate scoped credentials and explicit release gates. Do not claim these artifacts exist until they have been built and verified.

Never move a released tag or replace its assets. Fix a bad release with a new version. For a withdrawn release, explain why in the release notes and issue a replacement. Shared workflow updates land through reviewed PRs that change the full commit pins in the consumers.
