# Host authentication publication verification

Snapshot: **October 19, 2026**. This record separates the current
`publish-host-auth-apis` preparation from prior verification. It does not
announce a package release, native platform certification, hosted CI success
or a deployed docs page.

## Current adopted-caller preparation

Protected caller PR #13 merged as
`99ba4575ad9070a7a0f3296a35e746858af904b0`, with reviewed tree
`c05980717cd691025eebe6f9514b607d40ca01ae`. Its retained shared caller
`c4f2873ef0eef5bc3c7d75cf71fa3489a911d134` includes Linux D-Bus build
setup and strict complete-review enforcement. The caller change completed its
required hosted checks and separate exact-head approval before protected merge.
That acceptance covers the caller change, not this API source.

The ancestry-preserving API merge and resumed-clock fixture correction passed
the normal local hook and became commit
`90aa7350e30eaad908d14e8b3260610d5f15f2fe`, tree
`a2a65b0fd54cd666d25c863c184d2f69a1f6af0b`. Its parents are API head
`4aeff321c3ab1148bfc92776471552308d3e62da` and the adopted caller above.
It includes the ten-path vault correction ending at frozen tree
`4b84fa8790ba8b53c61d0e6270a182b687501bc0`. Of that correction's 87 runtime
inputs, 86 remained identical; only the existing deadline fixture changed.
Both workflow files, all 33 canonical review-context files, lint exclusions
and checked standards task history stayed unchanged.

Hosted run [39299507686](https://github.com/ejmlabs/ejm-harness/actions/runs/39299507686)
then exposed the two registry issues below. The current correction preserves
that committed source and caller while addressing those findings and the later
auth cleanup finding below. This successor has not run its local gate or fresh
hosted review. OpenSpec task 3.5 stays open.

## Hosted auth process-group correction

Hosted run [39302646399](https://github.com/ejmlabs/ejm-harness/actions/runs/39302646399)
reviewed registry-corrected head
`86677bbf89f34275614963eb26b91e05fcb7c80b` against protected base
`99ba4575ad9070a7a0f3296a35e746858af904b0`. The admitted source context and
both raw reports name that head. Both reviewers marked their static review
complete and ran no project code or tests. Pass 1 reported one medium defect;
pass 2 reported no findings. Their raw report SHA-256 values are
`6f377df659b36de26aa9b7b01988adee43664b738851ab244816ad42b7021e52`
and `000acabd4762928a1d0759be2a0e76ab58879af9b7d5885c4dc412665f39379d`.

Successful Unix auth cleanup killed its process group and reaped the child but
kept the numeric group identity. A later owner drop, after output-reader waits,
could signal an unrelated group if the OS reused that identifier. The successor
retires the identity synchronously after all termination and reaping checks
succeed. Cleanup errors and pending awaits retain ownership; Windows Job
behavior is unchanged. A bounded real-child regression checks retirement
without forcing identifier reuse. No dependency, workflow, vault deadline or
public API changes are part of this correction.

At the retained exact-head snapshot, Linux, macOS, registry and standards jobs
completed successfully; Windows was still running. AI pass 1 failed its
required validation because of the finding. These historical results do not
accept the successor. Its regression, full local gate and fresh exact-head
hosted checks remain pending under task 3.5. Real native key-service acceptance,
package release and docs publication remain separate and unproved here.

## Paused-clock deadline fixture follow-up

The normal pre-commit gate for staged tree
`b15bcb59da50138126be64b2f2c8eeb4fc4c5936` failed before commit at
`native_key_read_obeys_the_original_lock_deadline`: the assertion
`native call must retain ownership` failed. That auth test binary reported
33 passed, 1 failed and 2 ignored in 6.00 seconds. The retained log is
`normal-commit-pinned.log`, SHA-256
`4588283e3bf3bca22b8b41b33aee3eab458e08d4e74fb52112aeaee8c7a0b202`.
This is a failed gate, not acceptance of that preparation.

The fixture advanced a paused Tokio clock, then awaited a one-second Tokio
completion guard while its blocking key stand-in used a five-second wall-clock
watchdog. Released Tokio 1.53.3 rounds timer deadlines up to a millisecond and
inhibits paused-clock auto-advance during a blocking task. A jump can stop short
of the rounded timer tick, leaving the completion guard stationary until the
real watchdog returns and the worker releases its lock. This is a reachable
fixture failure path consistent with the observed duration; the exact failing
interleaving was not instrumented or reproduced.

The isolated follow-up resumes Tokio immediately after the deadline jump,
before awaiting the completion guard. It retains the watchdog, the original
production deadline and every lock-ownership, ciphertext and recovery assertion.
Only `crates/ejm-harness-auth/src/store/tests.rs` changes among the 87 runtime
inputs; the other 86, including all production source, manifests and the lock,
remain byte-for-byte and mode-for-mode identical. The new ordered input receipt
is `6ead7566688e18e4bbfc5806148372951024a79795765e3876eff5aea0670a45`.

The root-owned focused deadline test then passed: 1 passed, none failed or
ignored, in 0.11 seconds. The normal commit's required quality hook also
reported **Passed**, without a bypass, and produced `90aa7350e30eaad908d14e8b3260610d5f15f2fe`.
Its successful hook output suppresses individual suite counts; the earlier
202-test result below must not be attributed to this run. The normal log is
`normal-commit-clean-metadata.log`, SHA-256
`36d3bb3e44d1eb3a4a9f0989c3d5eda59632934b47e61ac7598d6d5e20d72266`.
The matching receipt is `normal-commit-clean-metadata-receipt.json`, SHA-256
`742d166872881e9cd1c4477418a72dc413d419a539be0cfb1ea7cf9113c01397`.

## Hosted registry correction

Both raw AI reports for exact head `90aa7350e30eaad908d14e8b3260610d5f15f2fe`
marked their review complete. Pass 1 reported no findings. Pass 2 reported two
medium defects, which remain required corrections despite the clean first pass:

- The ordinary `registry_tokens_are_origin_bound_and_redacted` integration test
  calls the native OS key service. The completed Ubuntu job failed this test
  when no usable Secret Service was available: 8 integration tests passed and
  1 failed. The existing injected-key test
  `encrypted_registry_sessions_are_origin_bound_and_deletion_is_permanent`
  passed in the same job and already protects round-trip, origin separation,
  redaction, encryption, deletion and stale-import behavior. Remove only the
  redundant native-store-dependent case; preserve explicitly opt-in native
  key-service acceptance.
- A legacy registry FIFO with no writer can block the read-only open before
  descriptor validation, while holding the vault transaction. Open the Unix
  source without blocking before checking that it is a private regular file.
  A bounded Linux regression must reject the FIFO, join its watchdog and preserve
  both source and existing encrypted destination.

These changes use the existing injected key boundary and maintained Rustix
adapter. They change no public API, dependency, workflow, vault deadline or
native key-service requirement. At the retained Linux analysis snapshot, the
macOS, Windows and registry jobs were still running; their acceptance is not
established here. The sanitized Linux log is
`linux-job-118220920291-sanitized.log`, SHA-256
`c1080495a78af781c63c623128b5f32f162e9d17fb020562d986121a0ef6e9e8`.
Task 3.5 remains open for the corrected source's focused and full local checks
and fresh exact-head hosted acceptance.

## Prior native-key wait correction

Hosted run [39245467732](https://github.com/ejmlabs/ejm-harness/actions/runs/39245467732)
reviewed exact API head `4aeff321c3ab1148bfc92776471552308d3e62da`.
AI pass 1 reported that cancellation and the original 30-second deadline were
not observed while synchronous native key access blocked. The independent
second pass completed with no findings. The first finding remains a required
correction; a clean second pass does not dismiss it.

The isolated follow-up keeps the blocking worker's stable lock through the OS
call, bounds the async caller and checks cancellation and the deadline before
later key or durable changes and transaction delivery. An already-started key
write can finish after cancellation, leaving its bootstrap marker for recovery
with the same key. Synchronous OS calls and runtime shutdown remain subject to
native completion. Key initialization remains serialized. No dependency or
storage namespace changes are part of this correction.

New injected-key fixtures cover a blocked read cancelled by its caller, a read
that reaches the original deadline, retained file-lock ownership and committed
state, and recovery after cancelling or abandoning an already-started first key
save. All four passed in the root-owned required `./scripts/check` run on runtime
source tree `de80bf8d9a67d54041cb7d167ece5faf55c42ecc`. That gate exited **0**
on macOS with Rust **1.99.1**, Node **26.12.0** and Python SDK **3.14.2**, with
at most two Cargo jobs. It passed **202 Rust tests** with **3 ignored**, **15
TypeScript SDK tests**, **4 policy tests**, **11 Python language-host tests**
and **16 Python SDK tests**, plus the required lint, build, Rustdoc, dependency,
formatting and generated-protocol checks and Python source/wheel builds.

The same ordered 87 Rust workspace inputs have canonical JSON content-receipt
SHA-256 `752e0b103d746df4e527892062a7728d5113fe6d2f43471bc519034cdfcd0a97`.
Only `store/vault.rs` and `store/tests.rs` differ from the prior 87-input receipt
below. This receipt identifies runtime inputs independently of later
documentation updates. The local log is
`ejm-harness-vault-cancel-full-gate-20261019.log`, SHA-256
`ec8daa2e0b5e38edfdfdd4962cc9fedf26ce06a9821b7a2a5fe3101a64280e0e`.

Prior gate results below belong to their named earlier source snapshots.
The Linux D-Bus setup and trusted caller prerequisite are now adopted as
recorded above. The later `90aa7350e30eaad908d14e8b3260610d5f15f2fe` normal hook
passed, but its hosted findings require the new correction above. Real native
key-service acceptance remains unproved. Task 3.5 stays open for the corrected
source's local gate and fresh exact-head hosted acceptance; package release and
docs publication remain separate.

## Prior protected-parent preparation

The prior final-parent candidate is based on protected standards merge
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
That candidate's 87 tracked Rust workspace inputs have canonical JSON content-receipt
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
separate. A later CLI dependency transition also needs reviewed CI dependency
selection so each harness revision uses its matching lock; that consumer change
is outside this harness correction. Hosted checks must use the final reviewed
immutable harness source and any checkout credential required by the repository's
actual access policy. The owner authorized public source rollout separately;
this API correction does not change repository visibility.

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
