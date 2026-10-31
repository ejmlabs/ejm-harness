# Parallel work

## Fanning out

When several agents share one task:

1. Write the conventions first: file naming, module layout, patterns, the helpers to reuse. Give
   every agent the same written sheet. Divergent shapes (`useThing.ts` next to `use-thing.ts`,
   `mod.rs` next to `foo.rs` plus `foo/`, a second copy of a helper) are a defect.
2. Give each agent a disjoint set of files and one written brief: goal, files it owns, what is
   already ruled out, what done looks like.
3. Agents edit only their own files. They report a needed cross-file change instead of making it,
   and never change git state (no commit, checkout, stash, reset or push).
4. One coordinator merges the results, brings any divergence back to the conventions, runs the
   repository's gate, checks the result the way a user would, and commits each slice on its own.
5. Record any new or chosen convention in the repository's docs or skill, not only in a scratch
   file.

## Sibling repositories

Repositories can depend on each other by path and by pinned revision. The repository's AGENTS.md
or a repository skill names who owns each shared piece; this skill holds only the rules.

- Never edit a sibling repository from another repository's task without its owner's agreement.
  When a sibling breaks your build, tell its session and wait, or work against its pinned
  revision.
- A change to a public item (crate, export, path, wire type, token) is coordinated: check every
  consumer still builds, and tell the owning sessions about removals, renames or type changes.
- Build and deploy against the pinned revision of a dependency, never against a sibling checkout
  that may hold uncommitted work.

## Machine limits

- Check free disk and memory before large builds. On a constrained shared machine, cap build
  parallelism; CI and dedicated machines use every core.
- When a disk fills, clean only this repository's build output. Never delete another project's
  files.
- Bound concurrent jobs and child agents. Keep the machine responsive for the person using it.
