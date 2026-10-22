# Provenance

The scenario files below were copied byte for byte from the old kit (`design-kit`, repo `ejmlabs`) on 2026-10-18. The sha256 is of the file contents, which are the same in both places. `scenarios.test.ts` fails if a copied file stops matching its hash, so a change to scenario data is a deliberate edit of this file too.

| File | Source path | sha256 |
| --- | --- | --- |
| cli.json | `design-kit/web/demos/cli.json` | `a88ae53eb2f782f56dcb71b5f43c5a0c6341d5d74472cadcf752fbc67ab2a9ea` |
| ide.json | `design-kit/web/demos/ide.json` | `c71fe90e2c989f0d295a88e2f28c4a7036811a76f34bcc937a00a31dceb0fff0` |
| harness.json | `design-kit/web/demos/harness.json` | `22e3a36eac3d82bd4dfe7fa2047562452370a324ea178c677574d324d3e262eb` |
| harbor.json | `design-kit/web/demos/harbor.json` | `7bd65edf20de46a1e659c1db2f96e351f9fc7bedcc6a308e137db62cc24ba4b8` |

The behavior was ported from `design-kit/web/demo.js` (sha256 `8d20f00a6d9a74886e30c1bf6c524302cfc930c66cde3a4d90fd58ed10b3add9`) and the rules from `design-kit/web/DEMOS.md` (sha256 `3345b9eec932dc6259beb4d2c8348dea8c63cf38628c65979ad7b7cd45f935a5`).

## Written here, not copied

`labels.json` holds strings that `demo.js` built in code and that name a command, flag, key or tool (for example the blocked-write line, `/resume` answering "not part of this demo", the palette command names). They are data so the honesty check can read them. The wording is the old wording.

## Edited after the copy

`ide.json` was edited on 2026-10-18 to use the tool names ejm-ide documents in `docs/agent-tools.md` (`search_code` replaces `search`, and `list_files` is gone because ejm-ide has no such tool; the explain scenario searches for `pub fn` instead) and to drop the `Ctrl+K` and `Cmd+K` shortcut, which no ejm-ide document describes. On 2026-10-20, its read, search and write names were aligned with the published `workbench_read_file`, `workbench_search_code` and `workbench_write_file` contracts. The static frame and generated write label use the same names. The hash above is the edited file.


## Product-owned recordings

The added `LazyCliShowcase` and `LazyHarnessShowcase` entries contain playback
machinery, not captured workflows. Their consumers supply the compact frames, a lazy timeline and the
SHA-256 ID of the capture input manifest. The timeline must carry that same ID.
Matching IDs catch stale pairings; they do not prove authenticity or execution.

Keep the real product source, binary digest, command/replay inputs, observed
output/status and generated-frame receipt with the product-owned recording.
State which narration and timing are authored. A browser replay never executes
the shown commands or calls a model. The package's tiny counter/frame test data
is labelled test-only and claims no native output. No older local recording was
imported into this change, and no new capture or publication is claimed here.
