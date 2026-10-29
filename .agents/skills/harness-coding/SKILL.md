---
name: harness-coding
description: Change the native coding tools in ejm-harness-coding, including file reads, search, writes, patches, the optional shell tool, path safety, and visible output limits.
---

# ejm-harness coding tools

Read `crates/ejm-harness-coding/src/`: `lib.rs` registers the tools, `path.rs` owns path safety,
and each tool has its own module. Then read [the executor contract](../../../docs/executor.md)
and [harness engineering decisions](../../../docs/harness-engineering.md) for retrieval limits.
The tool set is small on purpose. Prefer tightening an existing tool over adding one.

- Registering a tool is the grant. `tools(CodingConfig)` always offers read, list, and
  search. Write and patch need `allow_write`. The shell tool needs `allow_shell`. No
  repository file, model output, or session option can add a tool or an effect.
- Reach files through the session directory capability (`cap-std`). Reject `..`, absolute
  and prefixed paths, symlinks that leave the workspace, and the protected names `.git` and
  `.ejm-harness` (matched case-insensitively). Check the resolved parent, not only the
  requested string.
- Edits carry a SHA-256 precondition from a prior read. A stale or missing precondition
  fails the edit. Never overwrite on a guess.
- Keep every limit visible to the model: per file size, total search bytes, files visited,
  and retrieval output size. A truncated result says it was truncated. Raise a limit only
  with a bound that still holds across a hundred sessions.
- Mutating tools serialize by canonical session directory. Read tools may run concurrently.
- The shell tool runs through the shared `Executor`, reserves resources first, and writes
  large output to the artifact directory instead of memory. Keep normal Bash semantics when
  it is granted, and do not run a shell for work a native tool can do.
- Rust hosts can use `tools_with_shell_process_tree_cancellation` to require enforced
  process-tree cancellation; unsupported or monitored cleanup fails before launch.
  Defaults remain unchanged.
- Do not hand-roll traversal, diffing, or encoding. Use the maintained libraries already
  in the crate and keep custom code on grants, bounds, and preconditions.

Cover a changed behavior with a test beside the code, as the existing tests do: parent
and symlink escapes, protected metadata, limits at the exact boundary, stale preconditions,
and cancellation. Run `cargo test -p ejm-harness-coding` and strict Clippy, then
`./scripts/check`. Use [harness-execution](../harness-execution/SKILL.md) when a change
touches admission or process ownership, and
[harness-maintenance](../harness-maintenance/SKILL.md) when a grant or limit contract changes.
