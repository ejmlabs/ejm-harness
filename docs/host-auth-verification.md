# Host authentication publication verification

Snapshot: **October 19, 2026**. This record separates the current
`publish-host-auth-apis` preparation from prior verification. It does not
announce a package release, native platform certification, hosted CI success
or a deployed docs page.

## Current protected-parent preparation

The current candidate is based on protected standards merge
`ee1d2e3df5d77027be226d20830f6c5ea84d0d1d`. Its tree matches reviewed PR #1
head `6da46394f29b3d0d1f068175b1715e8c6b048d23`. Reapplying only the original
source changes from `b97d5838a68a56495ba17f413a566b7122aabb24` to
`943aa862209101624421ca82745cc68642169647`, produced reviewed source tree
`737cbfec9696eee983681c728ecab8c3bb57e87d` before this publication-reference
update. The parent adds hook-fixture timing and diagnostics without changing
the API implementation or dependency locks.

The final-parent `./scripts/check` exited **0** on macOS with Rust **1.99.1**,
Node **26.12.0** and Python SDK **3.14.2**, with at most two Cargo jobs. It passed
198 Rust tests, 15 TypeScript SDK tests, 4 policy checks, 11 Python language-host
fixtures and 16 Python SDK tests, plus all required lint, build, documentation,
dependency and generated-protocol checks. Three Rust fixtures remain ignored.
The current 87 tracked Rust workspace inputs have canonical JSON content-receipt
SHA-256 `5c3a37097ec868392e0b776ad607fe4a97375dcf49c18c8541f071627127554f`.
Only the parent's hook-test source differs from the original input receipt.

Fresh locked consumer checks against that source also exited **0**:
`cargo check --locked --tests` for the integrated CLI source tree
`03596d6a2f2b51c7b0830d2e8545c5ec13cbd213` and
`cargo check --locked --tests --manifest-path src-tauri/Cargo.toml` for IDE
`ad97a401ac956ab7dac167e08d9459b9317309c6`. Both used their scoped resolved
locks in isolated archives. Their active checkouts were unchanged.

The local gate log is `ejm-harness-final-parent-full-gate-20261019.log`,
SHA-256 `9c04fb4c9092e72b07557cd105533f8f6c98b3148a8d7391aa4c78e42d7fa250`.
The locked consumer logs are
`ejm-harness-final-parent-ejm-cli-consumer-20261019.log` and
`ejm-harness-final-parent-ejm-ide-consumer-20261019.log`.
These results do not prove live key-service behavior, native cross-platform
acceptance, host runtime acceptance or hosted CI. The final reviewed publication
commit and fresh hosted checks remain separate. Prior results below apply to
`943aa862209101624421ca82745cc68642169647`, based on
`b97d5838a68a56495ba17f413a566b7122aabb24`.

## Prior local harness gate

The parent-owned `./scripts/check` run exited **0** on macOS **26.5.2 arm64**,
Rust **1.99.1**, Node **26.12.0** and Python **3.14**. Cargo jobs were capped at
two. The run used unoptimized test/development builds with debug information
disabled. No real provider sign-in or paid inference ran.

| Check                         | Recorded result                                                                                                                                                                                  |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Rust tests                    | 198 passed, 3 ignored                                                                                                                                                                            |
| TypeScript SDK tests          | 15 passed                                                                                                                                                                                        |
| Python policy checks          | 4 passed                                                                                                                                                                                         |
| Python language-host fixtures | 11 passed                                                                                                                                                                                        |
| Python SDK tests              | 16 passed                                                                                                                                                                                        |
| Other required gate stages    | Policy, rustfmt, strict Clippy, workspace/all-target builds, rustdoc, Machete, Deny, generated protocol freshness, ESLint, Prettier, TypeScript, Ruff, mypy and Python source/wheel build passed |

The Rust run includes the nested-parent save-and-reopen regression for the
credential directory adapter, using injected key storage. Running that portable
fixture on macOS does not execute the Windows ACL or handle path. The three
ignored fixtures remain separate from the standard gate; the two explicit
vault profile runs below are recorded separately.

The local gate log is `ejm-harness-publication-corrected-env-20261019.log`,
SHA-256 `83e7863378f83ff72fb6973d179f581a77a8c137269d2f7d0830b6b01690f72c`.
It is a local preparation artifact, not a hosted check or published attachment.

### Earlier failure and diagnostic repeats

The first full gate failed the existing
`signout_committed_before_cancellation_remains_observable` fixture at its
one-second pipe-EOF deadline. The focused official-client suite then passed all
five tests. The existing cancellation fixture passed **100 consecutive repeats
in 21.96 seconds**, and the subsequent full gate passed.

The failure's cause was not reproduced. No auth code or timeout was changed to
obtain the later pass. The successful repeats do not establish that the earlier
failure is fixed. Local diagnostic logs are
`ejm-harness-claude-focused-20261019.log` and
`ejm-harness-claude-race-reproduction-20261019.log`; retain this incident in the
review record if another host or CI reproduces it.

### Toolchain warning

The successful gate still emitted nonfatal `rust-objcopy` warnings about a
missing `libLLVM.dylib` in the installed toolchain. A child that clears inherited
library-search settings can still expose this warning. No toolchain repair or
successful release stripping was verified. A passing development build does not
prove that release binaries were stripped.

## Synthetic vault measurements

The explicit `store::vault::tests::vault_save_reopen_profile` fixture ran twice
against this extraction with **33 samples per workload**. The canonical
[measurement table and reproduction method](credential-vault.md#measured-publication-extraction)
record payload sizes, save/reopen latency and native process maximum RSS.
It uses a synthetic injected key, local temporary files and fixed nonsecret
records. Real key-service calls and unlock prompts are excluded. No application
idle baseline or native Windows/Linux performance was measured.

The two local profiler logs are
`ejm-harness-vault-profile-small-measured-20261019.log` and
`ejm-harness-vault-profile-full-measured-20261019.log`. Earlier isolated cipher
prototype numbers in the vault guide remain a separate experiment and do not
measure this extraction's complete persistence path.

## Prior consuming-host checks

| Host source                                                           | Check                                                               | Scope and status                                                                                                                             |
| --------------------------------------------------------------------- | ------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| ejm-ide `ad97a401ac956ab7dac167e08d9459b9317309c6`                    | `cargo check --locked --tests --manifest-path src-tauri/Cargo.toml` | Passed in 9.64 seconds in an isolated tracked-source archive after resolving 17 required dependency packages into that temporary copy's lock |
| ejm-cli native source tree `04f45d11b8befbbe2cd27483644a9f1728eccfb8` | `cargo check --locked --tests`                                      | Passed in 0.54 seconds in an isolated tracked-source archive using its resolved temporary lock                                               |

The IDE archive reused installed Node dependencies only after confirming its
`package-lock.json` matched the baseline. Its first `--locked` attempt correctly
rejected the stale lock. A later resolved build first lacked an esbuild resource;
reusing the matching dependency tree allowed the final locked check to pass.
Neither failure was an API compile failure. The final local log is
`ejm-harness-ide-consumer-final-locked-20261019.log`.

The CLI archive used the isolated harness candidate's full working source in a
sibling layout, including the parent-owned changes used by the passing harness
gate. The 87 tracked Rust workspace input files, including manifests and the
lock, have content-receipt SHA-256
`17a07e8b1c079a6d62fc9fb61be5d0549dee6a0a939ba711bdbcd4dc25173c48`.
This receipt covers the full working source rather than its earlier partial
index. Its initial
`cargo check --offline --tests` resolved the terminal, native remote and harness
dependency additions and compiled in **25.47 seconds**. A fresh
`cargo check --locked --tests` exited **0** in **0.54 seconds**. The resolved
CLI lock SHA-256 is
`5143432fc0201bedeef680d18ae1c4681e2215fddd6e2fb45cd4cbf0149f7d1b`;
the local log is `ejm-harness-cli-consumer-final-locked-20261019.log`.

Read-only comparison found 91 package records added, none removed and no
existing version, source or checksum replacement. All 462 registry checksums
matched the local cached index and package archives. The graph includes both
`webpki-roots` 0.26.11 and 1.0.9 through the compatibility wrapper; that graph fact
is not a policy failure.

The parent's separate `cargo deny --locked --all-features check` then exited
**0** on this resolved CLI lock: advisories, bans, licenses and sources passed.
No exception was added. An earlier sandbox attempt could not lock the read-only
advisory cache and stopped before scanning; the retry used the normal refreshed
cache. The successful log is `ejm-cli-native-dependency-policy-20261019.log`.
This dependency check does not complete the CLI's full host gate.

The active CLI and IDE checkouts and locks were not changed by these checks.
Their owners must adopt the matching locks with the reviewed harness revision.
These checks prove local API compilation; they are not host runtime, UI,
packaging, native terminal or PTY acceptance. The full consuming-host gates remain
separate. Hosted checks must use the final reviewed immutable harness source and
a suitably scoped private checkout credential; this record does not claim that
credential is configured.

## Remaining native and release evidence

- Real macOS Keychain, Windows Credential Manager and Linux Secret Service setup,
  locked/unavailable/lost-key recovery and persistent restart acceptance.
- Native Windows ACL, reparse, nested-parent creation, interrupted-write,
  write-through and process Job behavior, including assignment failure and
  retained ownership until descendants are gone. Power-loss durability remains
  unverified.
- Native Linux procfs/process cleanup and user-session Secret Service behavior.
- Complete persistence latency with the real key service, an application idle
  baseline, and native process spawn/cleanup resource measurements.
- Host lock adoption, full consuming-host gates, hosted checks for the reviewed
  source and public docs publication.

Native execution is not a filesystem or network sandbox. Unix process groups
and sampling remain monitored controls. Hosts retain their runtime and admission
leases until cleanup completes and own terminal restoration around interactive
sign-in. See [native child ownership](native-process-support.md),
[credential recovery](credential-vault.md#key-service-and-recovery) and the
[official-client contract](claude-code.md). No offline fixture proves a live
subscription, another operating system or an available service.
