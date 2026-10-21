# Native preview provenance

These additive passive previews retain a reviewed local Harbor workflow from
2026-10-19. They do not replace the existing interactive simulations or
caller-owned recording APIs. Browser playback runs no command, worker or model
and sends no project data.

## CLI

The scene is authored against `fixtures/harbor/`. The retained
`fixtures/record-cli.mjs` copies that sample to a new temporary Git repository,
applies `fixtures/retry.fixed.rs`, and records `cargo test --quiet`,
`git diff --check` and `git diff -- src/retry.rs`. All three recorded exit codes
are zero; Cargo reports three passing tests. The raw receipt retains stdout,
stderr and elapsed duration. Those durations describe one local run, not a
benchmark. Narration, timestamps, session label and grant indicators are authored.
Read/write/shell grants model launch permissions and do not change in the scene.
The script only changes its own temporary workspace and receipt.

`recordings/cli.json` retains compiled transcript updates. Static/completed
frames select the prompt, patch acknowledgement, test result and final reply.
The full recording retains read/search output and the full diff. Regeneration
of the raw receipt requires Rust and Git; a changed receipt requires deliberate
review and an updated timeline and excerpt. No automatic capture runs in a build.

## IDE

The native adapter follows the product's editor, project tree, chat and inline
automatic review presentation. Its tools retain the public `workbench_` names
from `ejm-ide/docs/agent-tools.md`, including read, search, open and write tools.
The workflow and two reviewed writes are authored. Five boundary tests are
added; they were not run, and the final reply asks the developer to run Cargo.
The model label is a sample. Source/diff text uses the bundled mono font; native
UI type follows the desktop app. Lucide icon paths retain their original license.
No Tauri or Monaco runtime is imported.

`recordings/ide.json` stores compact authored updates. The adapter combines
pairs of original 20ms typing chunks to stay within the existing 256-beat bound,
retaining every character and each stream's end time. It updates at most 25
times per second. The static/completed excerpt selects the final edit and reply.
There is no claim of captured desktop execution or passing IDE tests.

## Harness

`fixtures/harness-workflow.mjs` uses the public TypeScript SDK and a local worker
with its offline replay provider. The retained receipt includes a real file
read and shell events, two workspaces sharing one heavy slot, running/queued
snapshots, completion, a restarted worker, a reopened session and an event page.
The shared source guide's default echo/replay behavior remains valid for the
public runtime examined for this release. This historical receipt establishes
no claim about an unpublished development branch.

Governor numbers are admission reservations, not measured CPU use or enforced
memory isolation. Known complete JSON snapshots are formatted as labelled counts
and reservation rows without changing their values. Unknown fields and malformed
lines remain raw. No dashboard or extra application chrome is invented. The
static/completed excerpt keeps running/queued work, a tool result, both completed
runs and the reopened journal; full output remains in the recording and receipt.

To recapture intentionally, build the public SDK and worker, set
`EJM_HARNESS_DIR` to that repository and run
`EJM_RECORD_SHOWCASE=1 node packages/demos/fixtures/harness-workflow.mjs`.
The script uses and removes its own temporary workspaces and state. The retained
raw receipt has no immutable worker revision, so it does not prove exact source
reproducibility, live provider authentication or current deployed behavior.
Timing is scripted. No fresh worker run is claimed by this presentation change.

## Retained integrity records

The IDs in the adapters are SHA256 digests of their raw timeline inputs. They
catch stale data/adapter pairs; they do not authenticate a server or prove the
truth of authored narration. Tests also compare recorded CLI/Harness stdout with
the raw receipts and check finite playback and immutable final excerpts.

| File, relative to this package | SHA256 |
| --- | --- |
| `src/previews/recordings/cli.json` | `35364d0b1b9dc10bed75d3e1925ef2a8a9cfebbde1d5f4924e893fe12bd3e0d8` |
| `src/previews/recordings/ide.json` | `093840636c87fa0d98a1580ab55a87812de222d7f95eaa84ac3b7cbbc030917c` |
| `src/previews/recordings/harness.json` | `9985083493447566a434b1f40ed22271889c06bf48e202c646d68fc813679433` |
| `fixtures/cli-output.json` | `392fe4f19350cef498b5b372e961528c5f339414a3e9db1b5b5defaf0dbbe3b4` |
| `fixtures/harness-output.json` | `e9c7ca66ce68936a8ff9c9235e552408a6d36abd095aee8436d5e68e6a6324e6` |
| `src/previews/frames/cli.json` | `ee1ab8977ffa6e4a60c4c3195097baa691a7d60853e46ea30f9a4f2c45dda219` |
| `src/previews/frames/ide.json` | `5809b237d2d8b790ccc727fb588d6f27e576dbacb26840ad2a1d3d1a1176a46a` |
| `src/previews/frames/harness.json` | `2e8cdffcce9e013188af8cfba795da212e44418c260486d499f82a75b1e065bc` |
