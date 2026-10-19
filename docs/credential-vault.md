# Shared credential vault

## Package decision and prototype

The harness owns one encrypted format, transaction implementation, and scoped
secret API for the CLI and IDE. Native keyring adapters hold only its 32-byte
random key. No background service or helper process belongs to the vault.

The implementation uses the existing `aws-lc-rs` 1.18.2 AEAD and random-key APIs.
`RandomizedNonceKey` generates each AES-256-GCM nonce inside the maintained
library. The application never supplies a nonce sequence or implements a cipher.
The small envelope is necessary because no evaluated complete format fits the
production requirement:

| Candidate                              | Fit and decision                                                                                                                                                                                                                                      |
| -------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `age` 0.12.1, MIT/Apache-2.0           | A standard authenticated file format with bounded streaming and native key management. Its crate documentation explicitly restricts all pre-1.0 releases to testing purposes. Do not adopt it for production while that restriction applies.          |
| `aws-lc-rs` 1.18.2, ISC/Apache-2.0/BSD | Already used for JWT/TLS. Its maintained randomized-nonce AEAD supplies encryption, authentication, and randomness. Selected for the bounded application envelope.                                                                                    |
| Whole state in native keyring entries  | Native payload limits differ, and separate entries cannot atomically commit provider recovery state with host secrets. Use native entries only for the key.                                                                                           |
| `keyring` 4.2.0 umbrella               | Its documentation recommends `keyring-core` plus explicit native stores for applications that choose their backends. Use the CLI's existing pinned native packages, with no global default-store mutation.                                            |
| `tempfile` 3.27.0                      | Existing Unix replacement and private-file primitive. Keep directory synchronization and stable locks. Its Windows rename alone does not prove durability.                                                                                            |
| `atomic-write-file` 0.3.1              | Its generic Windows backend uses ordinary rename and has no directory commit. Insufficient by itself for durable vault updates.                                                                                                                       |
| `atomicwrites` 0.4.4                   | Windows uses write-through replacement, but this adds old platform dependency versions and does not supply protected handle-relative state access.                                                                                                    |
| `fs-mistrust` 0.16.0                   | Established Unix checking library. Its README states that it accepts all Windows owners and permissions, so it cannot supply this Windows boundary.                                                                                                   |
| `rightkit-fs` 0.4.4, MIT/Apache-2.0    | Protected Windows DACL creation, pinned handle-relative state access, reparse rejection, and write-through publication; macOS extended allow-ACL refusal. Windows acceptance remains pending; source inspection alone does not prove native behavior. |

Prototype environment: Apple Silicon, macOS 26.5.2 (25F84), Rust 1.99.1,
release profile with thin LTO, one codegen unit and stripped binaries, two build
jobs. The harness source baseline is `9f7cadf9485873ece2130c0a698261c8dade8bf1`
with preexisting local changes. Separate temporary binaries used the exact
package versions above. Each size used 33 encrypt/decrypt repetitions, including
buffer allocation, with a fixed nonsecret input. Results exclude filesystem and
OS key service latency and are not native Windows/Linux measurements.

| Input | Existing AEAD median / p95 | age median / p95   |
| ----- | -------------------------- | ------------------ |
| 1 KiB | 0.002 / 0.010 ms           | 0.085 / 0.128 ms   |
| 1 MiB | 0.298 / 0.318 ms           | 3.697 / 4.165 ms   |
| 4 MiB | 1.281 / 1.443 ms           | 14.870 / 16.294 ms |

Standalone stripped prototype sizes were 1,088,432 bytes (AEAD) and 833,040
bytes (age). These isolated sizes do not measure incremental harness size: the
harness already links AWS-LC. No bundle-size improvement is claimed. Peak RSS
was not available from sandboxed `time -l`; it reported a denied `sysctl` after
the age workload, so that wrapper's exit status was not a successful RSS check.

## Measured publication extraction

On October 19, 2026, the isolated publication candidate ran
`store::vault::tests::vault_save_reopen_profile` on macOS 26.5.2 arm64,
Rust 1.99.1, with two Cargo jobs and unoptimized test/development builds with
debug information disabled. These are persistence measurements for this
extraction, separate from the earlier cipher prototype above.

Each workload used 33 samples and an injected synthetic key. One timed sample
acquires the vault lock, commits existing state, reopens the encrypted vault,
acquires another lock and checks the retained secret and migration marker.
The initial fixture setup is outside the timed samples. Filesystem and format
work is included; real OS key-service access and unlock prompts are excluded.
The native process profiler measures the whole test process, including setup.

| Workload                                                                                          | Plaintext bytes | Encrypted bytes | Median / p95         | Process maximum RSS |
| ------------------------------------------------------------------------------------------------- | --------------: | --------------: | -------------------- | ------------------: |
| One 1 KiB secret and one migration marker                                                         |           1,228 |           1,264 | 10.130 / 30.627 ms   |    11,026,432 bytes |
| 128 secrets of 16 KiB, 128 markers, 64 registrations and provider state filled to its 1 MiB limit |       3,162,984 |       3,163,020 | 273.892 / 299.450 ms |    38,879,232 bytes |

The large workload fills the listed record/provider limits, not the vault's
4 MiB total-plaintext ceiling. Its process maximum RSS is **26.6 MiB** above
the separate small-fixture process. That difference is not an application's
increase above idle and does not establish the 32 MiB above-idle target.
These local injected-key timings do not establish complete native key-service
latency, release-build cost or performance on Windows, Linux or other hardware.
No before/after speed or footprint improvement is claimed.

To repeat the measurement, build the auth library test executable with the
same profile and run that executable alone under the native profiler. Do not
profile a Cargo wrapper as if its RSS were the test process's RSS:

```sh
CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
  cargo test --locked -p ejm-harness-auth --lib --no-run
# Set AUTH_TEST_BINARY to the library test executable Cargo reports.
/usr/bin/time -l "$AUTH_TEST_BINARY" --exact \
  store::vault::tests::vault_save_reopen_profile --ignored --nocapture
EJM_HARNESS_VAULT_PROFILE_FULL=1 /usr/bin/time -l "$AUTH_TEST_BINARY" --exact \
  store::vault::tests::vault_save_reopen_profile --ignored --nocapture
```

The [publication verification record](host-auth-verification.md) names the local
logs and separates the passing harness gate from native key-service, consuming
host and release acceptance.

## Format and bounds

The format starts with `EJMVLT01`, then the library-generated 12-byte nonce,
then AES-256-GCM ciphertext and its authentication tag. The version and the
canonical native vault path identity are authenticated additional data. Moving
the vault to a different path is not a supported backup or migration method.
The keyring service is `com.ejmlabs.harness.vault.v1`; its account is a SHA-256
digest of the canonical native path encoded as URL-safe base64. A missing key
for existing ciphertext is an error, never permission to replace the key.

The encrypted JSON has a version, optional provider state, scoped secret
records, permanent migration markers, and provider-migration phase. Product,
purpose, and identity form an exact three-part secret identifier. Identities
must include the origin/account dimensions required by their consumer.

Limits are 1 MiB and 64 registrations for provider state, 128 generic secrets
of at most 16 KiB each, 128 permanent host migration markers, 4 MiB total
plaintext, and 8 MiB maximum encrypted input before allocation. Plaintext and
keys use zeroizing owned buffers. No plaintext payload is staged on disk.

The preliminary crypto budget is 10 ms p95 at 4 MiB on the measured Mac, with
no dedicated idle process, timer, thread, or polling. Async native key access uses
the host's Tokio blocking pool. The complete save/reopen budget is 1 second
excluding a user unlock prompt, and the maximum-store working-set
budget is 32 MiB above idle. The synthetic macOS persistence and process-RSS
measurements above exclude real key services and do not use an application idle
baseline. Complete key-service latency, above-idle working set and native target
measurements remain acceptance work.

## Transactions and migration

Every read or update holds the stable `.lock`, including first key creation,
refresh exchanges, and persistence. Async acquisition stops on cancellation or
after 30 seconds from the initial request, including time spent waiting for the
native key service. File-lock contention uses a 25 ms retry interval.

The blocking worker owns the prepared transaction through each synchronous
native call. Cancelling or dropping the async wait does not stop an already-started
OS call or release its file lock. The worker checks cancellation and the deadline
before later key or durable changes and before delivering a transaction. An
in-flight key save may still finish; its bootstrap marker remains recoverable
without replacing that key. Competing acquisition remains bounded while the
worker holds the lock. Runtime shutdown may wait for that native call to return.
The synchronous `lock_blocking` boundary checks cancellation and its deadline
between calls but cannot interrupt them; keep it off async and UI event threads.
The existing native stores expose synchronous operations without a cancellation
token. This uses Tokio's maintained blocking pool and cancellation primitives,
with no extra runtime, helper process or replacement key-store adapter. See
[Tokio's blocking-task limits](https://docs.rs/tokio/1.53.3/tokio/task/fn.spawn_blocking.html).

Transactions save only on explicit commit. First-time key setup persists and reopens an empty encrypted vault
before exposing its first transaction. An existing key with missing ciphertext
is a recovery error, preserving migration history instead of silently starting
an empty store. A bounded nonsecret `.bootstrap` marker is persisted before key
creation and retired after the empty encrypted vault is verified, before any
credential transaction is exposed. If setup stops after key creation, the marker
permits a retry with that same key. It never permits replacing the key or resetting
a previously admitted vault. Secret deletion preserves migration markers.

macOS mode checks also use the maintained native ACL reader. An extended allow
entry on a vault directory, lock, legacy source, or encrypted file is an error,
even if the visible mode is `0700` or `0600`. Replacement files are checked
before bytes are written. The adapter refuses unsafe entries without claiming
that earlier exposure has been repaired.

Windows requests write-through file publication and bootstrap retirement through
the maintained adapter. It does not synchronize directory handles, and native
power-loss behavior remains unverified. No stronger durability claim is made.

Provider migration reuses the legacy lock. It writes and reopens the encrypted
prepared state, publishes a nonsecret `state.json` version barrier, then records
completion. A crash before the barrier leaves the legacy file authoritative;
a retry rereads it while holding the same lock. A crash after the barrier uses
the verified encrypted state. Completed migration rejects a restored legacy
file and missing ciphertext. It never imports old credentials after logout.

Host keyring and registry migration require older clients to be stopped. Those
old writers cannot participate in the new lock. Hosts must make that upgrade
boundary explicit and defer an unconfirmed import. Verify the encrypted record
before publishing new references or retiring old records. Downgrade is limited
to versions that understand this vault; never export rotating tokens to an old
plaintext snapshot.

## Key service and recovery

macOS uses Keychain, Windows uses Credential Manager, and Linux uses an unlocked
user Secret Service through D-Bus. Persistent use requires that service to be
available. A headless Linux user must provision and unlock a Secret Service in
their user session, or explicitly select a host's supported memory-only mode.
There is no automatic plaintext or file-key fallback.

Unlock the original key service when it is locked. If its key was deleted or
replaced, preserve the vault and report recovery-required state. There is no
passphrase, cross-device sync, or portable-backup recovery in this version.
OS-user protection does not isolate arbitrary code running as that user or
prevent replay of an entire valid older vault by that user. Deleting legacy
files does not guarantee erasure from snapshots or storage hardware.

Native Windows ACL, reparse, interrupted-write, real key-service and lifecycle
acceptance remain required before platform support or release claims.
