# Harness engineering decisions

This review applies the eight references below to ejm-harness's current
workload: hosted models, a small coding tool registry, durable sessions, and
many agents sharing bounded local execution. Published vendor results guide
hypotheses. They do not establish ejm-harness's task quality or speed.

## Applied changes

| Source                                                                                                                           | Decision                                                                                                                                                                                            | Local evidence                                                                                                                                |
| -------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| [Manus context engineering](https://manus.im/blog/Context-Engineering-for-AI-Agents-Lessons-from-Building-Manus)                 | Keep system and tool prefixes stable. Preserve failure status and artifact references through journal messages and provider conversion.                                                             | Message compatibility tests and local HTTP provider fixtures.                                                                                 |
| [The Complexity Trap](https://github.com/JetBrains-Research/the-complexity-trap)                                                 | Under context pressure, mask older successful tool observations before discarding actions. Retain the latest exchange in full and preserve error text while masking. Add no summarizer call.        | `policy.rs` tests preserve IDs, arguments, encrypted continuation, recent outputs, errors, UTF-8, and deterministic byte bounds.              |
| [12 Factor Agents](https://github.com/humanlayer/12-factor-agents)                                                               | Retain the explicit driver, effect journal, injected ports, and host-owned decisions. Recover uncertain effects without replay.                                                                     | `tests/recovery.rs` reopens twice, preserves confirmed results, and supplies uncertainty to the next explicitly started run.                  |
| [LangChain harness engineering](https://www.langchain.com/blog/improving-deep-agents-with-harness-engineering)                   | Strengthen the default verification and handoff guidance. Keep current hard time, step, token, and repetition bounds. Remove the redundant failed-call signature map without changing repeat stops. | Run fixtures cover cancellation and repetition. Default completion still needs no extra provider call.                                        |
| [Writing tools for agents](https://www.anthropic.com/engineering/writing-tools-for-agents)                                       | Make incomplete retrieval results visible in model-facing text, with a reason and a concrete way to narrow scope. Keep the text within the engine's output bound.                                   | Coding tool fixtures cover hit, byte, entry, skipped-file, and directory limits. A public Harness fixture inspects the next provider request. |
| [Code execution with MCP](https://www.anthropic.com/engineering/code-execution-with-mcp)                                         | Keep the small direct tool registry. Introduce discovery or a maintained MCP adapter when a concrete connector workload warrants it.                                                                | The current registry has three read tools and optional writes/shell. Its bounded worker callbacks already share admission.                    |
| [Effective harnesses for long-running agents](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents) | Ask for verification against the request and a useful handoff. Leave initializer scripts, acceptance checks, commits, and browser workflows to the host.                                            | Default prompt plus explicit recovery contract. `Completed` reports a clean run ending; it does not certify task acceptance.                  |
| [Demystifying evals](https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents)                                     | Grade observable results and inspect model-visible context. Use clean local fixtures for engine regressions and separate model trials for coding quality.                                           | The long-turn fixture exceeds the old history ceiling, completes through the public API, and checks the journal was not rewritten.            |

## Context and recovery contract

Native file reads, listings, and search reject `.git` and `.ejm-harness` path
components case-insensitively. They also reject symlink aliases into those paths
after resolution through the workspace capability. Ordinary workspace aliases
remain readable. Edits retain their stricter rule rejecting symbolic links.
These path controls do not sandbox shell commands or trusted extension code.

The journal retains full bounded messages. Context preparation changes only the
request copy. Loading history has separate byte and row limits. A page that ends
inside an exchange drops that partial older exchange, fetches the original user
anchor, and adds an explicit omission note to the request copy. It never sends
orphan results because a user anchor fell outside the page.

`context_max_bytes` bounds serialized `Message` history. System text, tool
definitions, and provider framing have separate limits and add to the request.
This byte budget is not a token count.

`RecentContext` first prefers complete recent user turns. Inside a long current
turn, it masks older successful tool observations only when needed. Calls,
arguments, result identities, error text, and artifact references remain during
masking. If that still cannot fit, complete older exchanges may be omitted,
including their error text. The latest intent and latest exchange must fit or
the run fails clearly. Artifact references identify outputs; they confer no
read permission and do not bypass the host's tool grants.

Pending-effect recovery reads the latest assistant exchange independently of
context trimming. Recovery therefore does not depend on an old turn fitting a
model request. A confirmed result stays confirmed. An unconfirmed effect gets
an error result stating that its outcome is unknown. Reopening again adds no
second interruption event or result.

`Message.is_error` and `Message.artifact` default to false and absent when an
older journal lacks them. Old messages do not gain metadata retroactively.
Use `Message::tool(call_id, &output)` when constructing tool results. Built-in
providers expose this metadata to the model; Anthropic also receives its native
`tool_result.is_error` field.

## Deferred choices

Do not copy a paper's observation window or summarizer threshold as a default.
The [Complexity Trap paper](https://arxiv.org/html/2508.21433) found that useful
window settings varied between agent implementations. This implementation uses
the existing byte budget and protects the latest exchange.

Changing budget reminders must not invalidate the stable system prefix. A tail
reminder needs a host contract, correct tool-result ordering, and context
accounting before adoption. A generic completion verifier can create unwanted
effects or extra model calls. Use host-defined acceptance criteria when that
enforcement is needed.

A persistent code runtime per agent would add host cost. Revisit it only with a
workload that demonstrates large tool catalogs or intermediate-result overhead.
Any future composed call must retain shared admission, cancellation, effect
journaling, and explicit uncertain outcomes. Native command supervision remains
distinct from a security sandbox.

## Evaluation rules

Run local replay and HTTP fixtures to check engine contracts. Assert final
workspace or journal state, provider-visible evidence, bounds, and cleanup.
Inspect failure traces. Avoid graders that require one arbitrary tool sequence.

Before claiming better coding outcomes, run repeated trials on fixed tasks,
models, settings, budgets, and clean environments with independent acceptance
checks. Record successes, regressions, tokens, calls, elapsed time, and relevant
host usage. No paid model trial was part of this review. The capacity benchmark
in [benchmarks.md](benchmarks.md) measures local engine behavior and terminal
startup, and cannot establish model quality or competitor superiority.

## Local validation on 2026-10-18

Both required project gates passed after these changes. ejm-harness passed
106 Rust tests, with one intentional subprocess fixture ignored, plus 13
TypeScript and 13 Python SDK tests. ejm-cli passed its 17 Rust tests, release
build, and macOS PTY interaction and restoration checks. Strict linters,
dependency policy, generated-contract freshness, and package builds passed.
A separate Python 3.11.14 run passed all 13 SDK tests against the release worker.

Repeated SDK full-journal failures exposed the worker runtime waiting for
Tokio's uncancellable stdin read while the host waited for worker exit. The
worker now awaits server and harness cleanup, then shuts down its runtime
without waiting for idle stdin. The existing two-second run-wait and one-second
process-exit limits were retained. The fixture reproduced the old binary's
failure before the fix.

These checks use local fixtures. Paid inference, live sign-in, target-laptop
capacity, and other operating systems remain outside this evidence.
