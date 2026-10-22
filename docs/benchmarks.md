# Performance evidence

The acceptance target is 100 managed agents using hosted model APIs on four
CPU cores, 8 GB RAM, and an SSD. Concurrency means many waiting agents share a
small pool of local commands. It does not mean 100 concurrent builds.

## Reproduce

Build both sibling projects in release mode, then run from ejm-harness:

```sh
cargo build --release --locked -p ejm-harness-bench
(cd ../ejm-cli && cargo build --release --locked)
python3 scripts/benchmark.py --agents 100 --iterations 20 --output docs/evidence/benchmark.json
```

The benchmark uses deterministic provider responses, not paid APIs. The parked
workload proves all agents are managed together, then cancels them. The mixed
workload performs three fixed Bash checks per agent, streams text, journals
results, and replays every event. A two millisecond sampler observes executing
jobs in the actual governor. This is a sample, not a proof of a global peak;
separate admission tests establish the maximum concurrency invariant.

The Python monitor samples engine RSS and summed descendant RSS no faster than
20 milliseconds. It excludes its own process. Summed RSS double counts shared
pages and can miss short peaks. This is not a kernel enforced memory ceiling.
It also measures process creation to first composer paint through a real PTY,
using unconfigured TUI startup with product and provider environment cleared,
a 100x30 terminal, and a reused state directory. It submits no inference request.

## Recorded local result

The final release run on 2026-10-18 used a Mac16,8, arm64, 12 logical CPUs,
24 GiB RAM, macOS 26.5.2, and Rust 1.99.1. The
[raw report](evidence/benchmark.json) includes both binary SHA-256 hashes,
all startup samples, resource limits, and host details.

| Measurement                           | Result             |
| ------------------------------------- | ------------------ |
| Simultaneously managed sessions       | 100                |
| Bash commands completed               | 300                |
| Mixed workload execution phase        | 1.210 s            |
| Sampled peak executing local jobs     | 4                  |
| Sampled parked engine RSS             | 22.91 MiB          |
| Sampled mixed engine RSS              | 25.72 MiB          |
| Sampled mixed process-tree RSS        | 38.03 MiB          |
| First composer paint with fresh state | 24.197 ms          |
| Warm first composer paint, p50 / p95  | 16.672 / 41.312 ms |
| Warm startup samples                  | 20                 |
| Hosted model API calls                | 0                  |

Warm samples ranged from 11.280 to 79.015 ms. These are local fixture results
from one run on this host. The process-tree sample includes the engine and its
command descendants; it excludes ejm-cli, which the startup test measures
separately. It is not the combined engine-plus-TUI acceptance metric below.
The historical startup reports used the removed demo flag. The current runner
launches the ordinary unconfigured TUI and measures first paint without starting
an inference request.

The [prior snapshot](evidence/benchmark.before-context-review.json) preserves
the same workload before the context review. Its execution phase was 2.103 s,
warm first-paint p95 was 23.050 ms, and fresh-state paint was 18.837 ms. These
single runs show variation in execution and startup timings and cannot
attribute that variation to the changes.

These measurements exclude a TypeScript or Python host, remote model latency,
model spend, large builds, and coding quality. They do not establish a result on
the target old laptop. Linux and Windows need their own execution evidence.

## Acceptance budgets

| Metric                          | Initial budget                              | Required evidence                                 |
| ------------------------------- | ------------------------------------------- | ------------------------------------------------- |
| Engine plus TUI RSS             | Below 256 MiB                               | Defined mixed workload on target hardware         |
| In-memory tool output           | Below 16 MiB globally                       | Accounting invariants plus allocation measurement |
| Warm first composer paint       | p95 below 300 ms                            | Repeated real PTY runs on target hardware         |
| Local preparation               | p95 below 100 ms, excluding queues/provider | Instrumented per-step workload                    |
| Received text to terminal paint | p95 below 50 ms                             | Timestamped provider and paint tracing            |
| Stop acknowledgement            | p95 below 50 ms                             | Timestamped input and acknowledgement tracing     |

The last three timings require instrumentation beyond a run completion timer.
Cancellation cleanup can take longer than acknowledgement. Resource gates and
cleanup correctness remain required when latency budgets pass.

For a performance change, record the before and after workload on the same host
with the same compiler and build profile. Report variance and regressions.
Comparisons with Pi, Codex, or Claude Code require pinned versions and equivalent
provider/model/task settings. No comparative speed claim has been established.

## Adaptive capacity fixtures

The staged-helper follow-up retains its separate
[release startup observations](evidence/native-helper-startup.macos-arm64.json)
and [measurement limits](executor.md#native-owner-cost-and-recovery-evidence).
The first sampled fixed-mode matrix at that checkpoint failed in process
sampling at 100 agents. Its
[raw failure](evidence/capacity-fixed.macos-arm64.staged-native.json) remains
available; the automatic matrix did not run. The
[formatting record](evidence/capacity-historical-formatting.macos-arm64.json)
maps preserved original bytes to the formatted historical reports without
changing parsed measurements. Three direct 300-command diagnostics
passed before that sampled failure. The later
[exit-transition investigation](evidence/native-sampler-exit-transitions.macos-arm64.json)
retains the two observed kernel races and focused verification. Its replacement
sampled matrix is recorded below. Earlier successful matrices retain their own
source and executable provenance.

### Staged-helper sampled matrix

The 2026-10-20 replacement [fixed](evidence/capacity-fixed.macos-arm64.zombie-aware-native.json.gz)
and [automatic](evidence/capacity-auto.macos-arm64.zombie-aware-native.json.gz)
reports completed all 36 single-process workloads: three parked and three mixed
repetitions per mode at each count. Each mode completed 46,764 native commands
with no failed model or tool outcomes. Both used the same frozen 7,520,160-byte
release executable, SHA-256
`66618eed04f6887b2c4eba21ed85401130936a47ef7891bf4c58a36458ad2f22`,
eight model slots and four job slots on the Mac16,8 described above. The external
RSS sampler remained enabled throughout both modes.

The [consolidated baseline](evidence/capacity-baseline.macos-arm64.json.gz)
retains the exact commands, release build inputs, host observations, hard
ceilings, all 36 raw samples, and nearest-rank p50/p95 values for 12 cells.
Each sample measures one whole work phase. With three repetitions, p50 is the
middle duration and p95 is the maximum; these describe the samples rather than
a stable tail distribution or per-request latency. CPU and
I/O are own-process counter differences from parking through the final checks.
They exclude child CPU and startup, and cached I/O can be absent. Missing or
regressing counters stay unavailable rather than becoming zero.

The frozen compiler log identifies 215 external dependency packages with
183,367,573 logical source bytes and three workspace dependencies with
1,454,143 source bytes. These totals include unused package files. They measure
source build inputs, not linked binary contributions or resident memory.
Package identity and enabled features come from the frozen build; source sizes
were observed later, and cached dependency contents were not hashed at build
time. The artifact records this provenance limit. This is a baseline for the
recorded executable, with no causal before/after claim or target-hardware pass.

| Managed agents | Fixed mixed median (range)  | Automatic mixed median (range) | Fixed / automatic peak tree RSS |
| -------------- | --------------------------- | ------------------------------ | ------------------------------- |
| 100            | 1.821 s (1.810 to 1.836)    | 2.000 s (1.819 to 3.110)       | 47.66 / 47.56 MiB               |
| 1000           | 13.539 s (13.535 to 13.654) | 13.535 s (13.530 to 19.764)    | 56.13 / 56.61 MiB               |
| 4096           | 91.576 s (62.521 to 92.897) | 92.689 s (92.463 to 98.494)    | 81.92 / 81.30 MiB               |

Every mixed 4096-agent repetition completed 12,288 commands. Automatic median
duration changed by +9.8%, -0.03% and +1.2% at the three counts. The automatic
100-agent work-phase maximum was 69.4% longer than the fixed maximum
(3.110 / 1.836 s); the 1000-agent maximum was 44.7% longer
(19.764 / 13.654 s). These maxima are also the three-sample nearest-rank p95
values. Three samples, sequential mode order and uncontrolled host
activity do not establish stable tails or explain that variation. These results
verify completion for this fixture, not the best safe static setting, low-end
hardware acceptance, CLI interaction overhead or automatic-default activation.
Summed sampled RSS can double-count shared pages and miss short peaks.

### Eight idle native helpers

A separate [eight-helper observation](evidence/native-helper-eight-idle-ready.macos-arm64.json)
started eight isolated fixture clients and retained one ready helper per client.
Across 20 samples in a settled five-second window, summed helper RSS peaked at
53.66 MiB; fixture clients peaked at 55.38 MiB, for 109.03 MiB combined. All
clients were reaped, all eight helper PIDs were absent, and their lifetime
witnesses were available after shutdown.

This used the earlier frozen smoke executable, before the final sampler fix.
The isolated domains do not test the registry's shared eight-generation pool,
startup peaks, retained state under saturation or real CLI/SDK composition.
The provisional 32 MiB helper charge therefore still needs startup calibration.

### Kernel RSS peaks and the shared helper pool

The [kernel-peak and shared-pool report](evidence/native-helper-kernel-peaks-shared-pool.macos-arm64.json.gz)
retains two attempts on 2026-10-20, their exact commands, fixture sources,
binary hashes, kernel-counter checks and cleanup proof. Three command-free
six-second windows recorded kernel lifetime RSS maxima of 6.77, 6.70 and
6.70 MiB. These are conservative startup bounds through later idle time for
the older frozen smoke executable. Six executor inputs differ from the sampler
checkpoint, so those startup bounds do not calibrate later embedding images.
Successful task-name lookups released their Mach ports, and sampled PID birth
identities matched before and after each counter read.

The pool attempt used the frozen benchmark executable from the sampled
matrix above. Eight participants shared one isolated registry and inherited
distinct descriptor limits, which required eight helper domains. Registrations
were staggered one second apart; their native-work gates were aligned. All eight
helpers were committed and running together. Each participant completed 800
commands, for 6,400 successful commands. A ninth participant reached actual
helper admission and received the pool-full error before any ninth endpoint
appeared. The host ancestor charged exactly eight generations, 256 MiB and
eight processes.

| Observation                                  | Recorded value     |
| -------------------------------------------- | ------------------ |
| Sequential RSS scans                         | 20 over 4.752127 s |
| Helper RSS scan-sum high-water               | 68.61 MiB          |
| Fixture-client RSS scan-sum high-water       | 114.77 MiB         |
| Combined RSS scan-sum high-water             | 183.28 MiB         |
| Individual helper kernel lifetime RSS maxima | 8.50 to 8.66 MiB   |

RSS scan sums are sequential observations, not an instantaneous aggregate peak.
They can count shared pages more than once and exclude native-command
descendants. Individual lifetime maxima include startup and active work up to
each read; they cannot be summed into a simultaneous peak. Fixture-client cost
does not establish CLI, worker, SDK or IDE cost. All nine clients were reaped;
all eight helper identities ended and their witnesses became available without
SIGTERM or SIGKILL in the successful attempt.

The first attempt started eight participants together. One was rejected before
any helper started because its initial grant could not fit the 96 MiB runtime
and dispatch baseline. The observed host CPU budget was 11,000 millis and each
participant initially requested 2,000 millis. The runner then masked that
rejection with an invalid eight-helper cleanup assertion. Both failures and
later cleanup proof remain in the report. Staggering registrations isolates
pool enforcement; it does not prove simultaneous first-start acceptance.

These measurements retain the provisional 32 MiB helper estimate. Other
embedding images, bounded retained-state saturation, simultaneous startup,
consumer costs and native Linux/Windows still need evidence. Task 3.11 and
automatic-default activation remain open. General host activity was uncontrolled.

For startup calibration of a real embedding image, the test-only
`native_owner_smoke` example accepts an optional absolute helper executable:

```sh
./target/release/examples/native_owner_smoke witness-client \
  /private/tmp/ejm-helper-calibration-state \
  /private/tmp/ejm-helper-calibration-state/ready.json /absolute/path/to/ejm-cli
```

Use a fresh private state root for each trial and a bounded external observer.
This client writes the ready marker with the helper PID and generation only
after authenticated readiness succeeds. It holds readiness without launching
commands and remains running until stopped. Record the actual helper image and inputs, observe kernel
lifetime peaks, then stop the client and verify both helper identity exit and
lifetime-witness release. Selecting an image prepares the measurement; it
does not establish its memory budget or enable automatic defaults. This adds
no shipped CLI or worker mode.

The ignored Unix control-state fixture uses the selected helper image and sends
no commands:

```sh
EJM_NATIVE_OCCUPANCY_HELPER=/absolute/path/to/ejm-cli \
  CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 \
  cargo test --release --locked --offline -p ejm-harness-exec --lib \
  native::tests::occupancy::real_helper_control_state_saturation \
  -- --ignored --exact --nocapture
```

Run it under the same workload lock and disk guard as the other native
measurements. Its descriptor preflight requires the current open count plus
1,440 descriptors. It fills 256 readiness holds, verifies the 256-entry completed
FIFO and eviction, then proves 1,024 prepared scopes and 32 incomplete handshakes
within four seconds, before the normal five-second protocol deadlines. Overflow
must be rejected, and released slots must be reusable. Bounded JSON stage
records identify the helper for an external memory observer. The fixture drains
control links, verifies the stable lifetime witness and exact helper identity
exit, and retains its state directory when cleanup cannot be proved. It does
not measure command memory or shared registry accounting. The local CLI and
worker measurements below exercise this fixture with release embedding images.

### Real embedding helper memory

The [embedding-memory index](evidence/native-helper-embedding-memory.macos-arm64.json.gz)
and [raw bundle](evidence/native-helper-embedding-memory.macos-arm64.tar.gz)
retain startup and control-state saturation trials before and after the private
transport buffer change on 2026-10-20, on macOS 26.5.2 arm64. Each series used
three fresh trials per production CLI and worker image. The bundle records
release executable hashes, build and source receipts, observer sources, raw
measurements and cleanup evidence, including the original budget miss.

The [current-image index](evidence/native-helper-current-image-memory.macos-arm64.json.gz)
and [raw bundle](evidence/native-helper-current-image-memory.macos-arm64.tar.gz)
add a separate 2026-10-21 UTC calibration after the temporary stable-lock repair.
Three startup and three control-state saturation trials per release image passed
on the same Mac. CLI SHA-256 is
`79193b2d6de3032f6c74d74e8a4d4b466c9b4a4b4cbe0345ce91ef3bc1c1199e`;
worker SHA-256 is
`1ca0a09f388406318f66221c25b80be66bef4331c24fb85e46daa45d55376dcc`.

| Current release helper | Startup lifetime RSS maximum | Saturated control-state lifetime RSS maximum |
| ---------------------- | ---------------------------- | -------------------------------------------- |
| CLI                    | 7.625 MiB                    | 14.703125 MiB                                |
| Worker                 | 7.453125 MiB                 | 14.421875 MiB                                |

The startup windows held authenticated readiness for six command-free seconds.
CLI readiness observations were 0.6151, 0.0576 and 0.0594 seconds; worker
observations were 0.3407, 0.0595 and 0.0676 seconds. These are not controlled
cold/warm comparisons. All six saturation trials passed the same 14 stages,
including 256 readiness clients, 256 completed entries plus eviction, 1,024
prepared scopes and 32 incomplete handshakes. No native commands ran in those
memory trials. All clients were reaped; exact helper identities ended and their
original witnesses released, with the saturation fixture's retained-inode proof.
The 32 MiB estimate remains unchanged. These trials do not calibrate other
images, platforms, command or interpreter peaks, or full process trees.

A separate current-image benchmark pool completed 6,400 commands across eight
participants. Its benchmark SHA-256 is
`a7b83ef181af4e2fb045d3ce174d4c8a18cb8d9fa7b2932d66a5493489493744`.
Eight committed helpers charged the host ancestor 256 MiB and eight processes.
Twenty scans over 4.753665 seconds reached a helper RSS scan-sum high-water of
67.984375 MiB; the corresponding client and combined high-waters were 190.890625
and 258.875 MiB. Sequential sums are not simultaneous peaks and omit command
descendants. The ninth claimant started after the eight exact helpers passed
commitment, identity and held-witness checks, then failed actual pool admission.
All nine clients were reaped, and all eight helper identities ended and released
their witnesses. This does not prove eight saturated CLI or worker helpers.

The bundle retains an earlier failed pool run in which the ninth claimant
reserved a pending slot before one intended participant, its cleanup audit and
the ordering correction. A later retry stopped at source preflight before any
fixture launch. Fresh build receipts then reproduced all four executable hashes.
The pool's runtime source checks passed while separately recorded contributor
docs changed outside the comparison scope. Earlier memory-trial provenance
remains separate. These results close no broader crash, scale or default gate.

The following measurements retain their earlier executable hashes and source
scope.

The buffer-reduced CLI image measured here has SHA-256
`bbdce48e8517f9f55541d52f63495a240712e4b2310c5565a15e0e17e3ccb3a4`;
the worker image has SHA-256
`66c32c353cb25226bdd52fbcc99093403d113508ffdd7b8da1652196992678d0`.
The later startup-lock correction produced a separate CLI checkpoint, SHA-256
`d4f538eb44e2a1c04d96ce2f3674eb8873bb4695993c521d581895a433a0d1e2`.
That checkpoint did not receive startup or saturated peak-memory calibration;
the current-image series above measures separate hashes.
The [CLI coordinated follow-up](https://github.com/ejmlabs/ejm-cli/blob/main/docs/benchmarks.md#startup-lock-race-and-1000-agent-retest)
records its race regression, focused checks and selected progress retest. Those
results do not transfer the earlier memory measurements to its new image hash.

The original saturated helper RSS exceeded the provisional 32 MiB charge:
CLI reached 33.9375 MiB and worker reached 33.75 MiB. The private transport now
uses Tokio's existing codec API to start each read and write buffer at 512 bytes,
instead of 8 KiB each. Buffers still grow for frames. The 32 KiB initial frame
limit, 8 KiB write backpressure boundary and existing streaming compaction stay
unchanged.

| Release helper | Original saturated RSS maximum | Replacement saturated RSS maximum | Replacement startup RSS maximum |
| -------------- | ------------------------------ | --------------------------------- | ------------------------------- |
| CLI            | 33.9375 MiB                    | 14.671875 MiB                     | 7.546875 MiB                    |
| Worker         | 33.75 MiB                      | 14.359375 MiB                     | 7.390625 MiB                    |

These are kernel lifetime RSS maxima through successful reads, not physical
footprint, instantaneous occupancy or peak memory for the process tree. Startup trials held
authenticated readiness for six command-free seconds after the observer saw
the ready marker. Replacement CLI readiness observations were 3.3338, 0.0860
and 0.0901 seconds from client launch; worker observations were 0.2781, 0.0839
and 0.0833 seconds. The first CLI trial was slower than its original trial.
This series establishes no startup speed improvement.

Each of the six replacement saturation trials passed all 14 stages: 256
readiness holds, a 256-entry completed FIFO plus eviction, 1,024 prepared scopes,
32 incomplete handshakes, overflow rejection and slot reuse. Each had successful
kernel RSS reads after the handshake-limit proof. Rejected pre-proof samples
and teardown read errors remain in the raw reports. No native commands ran.
All startup and saturation clients were reaped. Exact helper birth identities
ended and their original lifetime witnesses released; fixture cleanup also
validated the retained inode when its temporary witness path had been removed.
A timeout was never treated as exit proof.

The replacement windows fit the unchanged 32 MiB helper estimate on this Mac.
They do not calibrate other operating systems or embedding images, command or
interpreter peaks, complete consumer process trees, or all eight production
helpers under saturation. Broader crash and scale acceptance, safe static sweeps,
task 3.11 and automatic-default activation remain open. Concurrent site
maintenance was outside the recorded runtime source freeze; the bundle states
the exact source scope. No package or site publication is established.

### Simultaneous participant startup

The [simultaneous-start index](evidence/native-helper-simultaneous-start.macos-arm64.json)
and [raw evidence bundle](evidence/native-helper-simultaneous-start.macos-arm64.tar.gz)
retain three attempts, including failures, on this macOS 26.5.2 arm64 host.
The final frozen release executable is 7,520,160 bytes, SHA-256
`245fb5547789043a33633624c727fa55cc2ed33e4158ed8e0df8bc74b0ee8521`.
All 130 recorded build inputs matched before and after the build and workload.
The retained bundle is now a revised subset with 122 of those inputs, plus
build/focused-check receipts, runners, identity-probe source and every launched
client's stdout and stderr. Eight unrelated package-distribution source files
were omitted during the independent open source scope cleanup. `SCOPE.md` and
the receipt explain the revision. Original run records and input hashes remain
historical evidence. The revised subset does not reproduce the full measured
source tree. Its manifest verifies each retained member. The compressed bundle
is 907,532 bytes.

Each mode launched eight participants without intentional staggering. They
shared one fresh isolated capacity root and used separate session directories.
Each managed 100 agents for eight rounds, with a 500 ms park, one model request
slot, one job slot and a 512 MiB participant ceiling. Inherited descriptor limits ranged
from 4,096 to 4,992 in steps of 128 to require distinct helper domains. The
existing five-second command deadline stayed unchanged. The runner required
each JSON result to report `completed` and 800 commands; a zero exit alone
could not pass.

| Final case | Completed participants | Successful commands | Completion | Helper cleanup |
| ---------- | ---------------------- | ------------------- | ---------- | -------------- |
| Fixed      | 8 of 8                 | 6,400               | 19.028 s   | 1.896 s        |
| Automatic  | 8 of 8                 | 6,400               | 18.597 s   | 1.927 s        |

Launch spans were 13.779 ms and 60.797 ms. Every client identity ended and was
reaped. Each mode observed eight accounted helper identities; all ended and
their witnesses became available. All eight participant witnesses per mode
also released. Cleanup used no termination signals. All 143 sampled registry
snapshots fit their recorded ancestor envelopes, with helper commitments
reaching 256 MiB for eight generations. Discovery was sampled; the test did
not require all eight helpers to remain alive together.

The first earlier attempt recorded an SQLite busy rejection before journal
creation. Its failing SQL call is unproven. An unrelated probe assertion also
aborted automatic launch at two clients. The corrected observer then completed
all fixed participants but only seven automatic participants: the eighth
missed its command deadline before useful execution. Raw logs establish seven
fixed completions in the first attempt even though its aggregate list stopped
early at six. Cleanup scope and the earlier observer errors remain explicit
in the bundle.

The final checkpoint carries the full runtime/dispatch floor at registration,
preserves still-valid sibling helper selections during commitment, and retries
only busy initial WAL setup within its existing 100 ms budget. Nine focused
tests, formatting and strict core Clippy passed. This is a targeted startup
correctness result. One sample per mode, sequential case order and uncontrolled
host activity do not establish throughput improvement. Embedding startup peaks,
retained-state saturation, consumer cost, broad scale and native Linux/Windows
acceptance remain open. The provisional 32 MiB helper estimate and automatic
defaults remain gated.

After this measurement, the workspace lockfile changed only `rustls-webpki`
from 0.103.15 to 0.103.16 and `webpki-root-certs` from 1.0.9 to 1.0.10. The other
129 recorded inputs still matched at the comparison. The bundle retains the
measured lockfile; these native timings do not describe the later dependency
checkpoint.

### Running capacity fixtures

The source benchmark now accepts 100, 1000, and 4096 requested managed runs. This
extends the measurement envelope; it does not establish that every mode admits
4096 runs. A rejected constructor returns JSON with `status: "rejected"`, its
requested count, mode, and error. The legacy constructor retains its 1000-run
validation limit. Rejection is recorded rather than mislabeled as useful work.

The default `--capacity legacy` uses the existing uncoordinated constructor.
Explicit `fixed` and `auto` use the same public prepared-capacity API through the
`test-support` feature's isolated fixture constructor. They require an absolute
`--capacity-root`. The benchmark does not write the user's production host
ledger. Several fixture processes must share one fixture root to test shared
admission. A separate root is a separate test domain, not extra production
capacity.

Build once, then collect repeated identical workloads:

```sh
cargo build --release --locked -p ejm-harness-bench
python3 scripts/benchmark.py --agent-counts 100 1000 4096 --capacity legacy --skip-cli --workload-iterations 3 --output docs/evidence/capacity-legacy.json
python3 scripts/benchmark.py --agent-counts 100 1000 4096 --capacity fixed --skip-cli --workload-iterations 3 --output docs/evidence/capacity-fixed.json.gz
python3 scripts/benchmark.py --agent-counts 100 1000 4096 --capacity auto --skip-cli --workload-iterations 3 --output docs/evidence/capacity-auto.json.gz
python3 scripts/benchmark.py --agents 1000 --processes 2 --capacity auto --skip-cli --output docs/evidence/capacity-two-processes.json
```

The Python runner supplies one temporary shared fixture registry for a complete
invocation. Pass `--capacity-root /absolute/fixture/path` to keep learning state
across invocations. Each participant always gets a separate temporary session
journal. Record whether a run is cold or reuses learned state. Explicit model and
job flags are hard ceilings, so keep them identical across compared modes.
Use `--model-requests` and `--jobs` to perform safe static sweeps. Automatic mode
must be compared with the best safe fixed result under those same ceilings.

Each participant records its actual managed count, independent parked model and
model-wait counts, sampled local execution peak, committed journal timings, and
the time for a bounded 50-session page plus selected tail. Reports also include
binary hash and size, host details, process-tree RSS samples, actual commands,
process count, and individual results. The completion timer counts a successful
workload only after all expected runs and commands finish. It does not reward
failed or retried work.

To retain a completed journal for a product's session picker, history, and
switching fixtures, call the release binary directly with
`--state-dir /absolute/new/fixture/state`. The path must not already exist.

The unpublished benchmark crate also exposes `Workload` and `LocalCheck` for
host acceptance fixtures. `Workload::new` shares the runner's bounded development
provider. `LocalCheck::new` shares its fixed native command with a ten-second
timeout, including queue time, and can attach a prepared context's native owner
with `with_native_owner`. Product fixtures use these public development APIs so
their load matches the runner without copying a provider or tool implementation
into a consumer.

The result reports `retained_state_dir` and `first_session_id`. The fixture uses
the retained directory's parent as the sessions' working directory, calls normal
`Harness::shutdown`, and leaves durable sessions for the next process to open.
Omitting this flag keeps the temporary journal behavior. This proves saved-session
navigation; it does not create hidden live agents inside another process.

The `test-support` feature also exposes `Harness::subscribe_session_traced` and
`SessionSubscription::recv_traced` for in-process terminal fixtures. The latter
returns an event and an optional monotonic commit timestamp. A traced subscriber
opts its channel into one timestamp immediately after each successful transaction
commit, shared by that batch's selected events. Each batch samples traced interest
before commit. Dropping the final traced subscriber ends future interest; a batch
that already sampled it may still take its timestamp. Ordinary subscriptions
alone request no timestamp; an ordinary receiver sharing a traced batch can still
receive its timestamp.
Treat a missing stamp as incomplete evidence, never as receipt time. Timestamps
remain in memory and do not change durable events, schemas or the worker protocol.
Default builds exclude the added clock fields and APIs.

The [CLI fixture method](https://github.com/ejmlabs/ejm-cli/blob/main/docs/benchmarks.md#active-agent-terminal-fixture)
keeps commit-to-terminal-write samples separate from receipt-to-write samples.
This includes publication and queue delay after commit, but excludes journal work
before commit and physical display latency. An independent observer can receive a
marker after its first write. These diagnostics do not establish complete
first-write coverage, trace overhead or the scale acceptance targets.

### Recorded initial capacity evidence

The initial release fixture used the same Mac16,8, Rust 1.99.1, two runtime
threads, eight model slots, and four command slots. The reports retain the binary
hash, size, limits, and all samples. This binary predates the later coordinator,
pending-response, and text-grouping changes, so these results are a baseline.

The [legacy report](evidence/capacity-legacy.json) contains three repetitions per
count and workload:

| Requested agents | Mixed execution range | Commands per run | Bounded page and tail |
| ---------------- | --------------------- | ---------------- | --------------------- |
| 100              | 1.180 to 1.201 s      | 300              | 124 to 148 us         |
| 1000             | 11.860 to 12.342 s    | 3000             | 154 to 183 us         |
| 4096             | Rejected              | 0                | Not measured          |

At 1000 agents, sampled engine RSS was 39.9 to 40.4 MB and sampled process-tree
RSS was 52.9 to 53.3 MB. These are decimal bytes from the report. The legacy
constructor's rejection at 4096 is expected and proves no throughput at that count.
An [initial fixed run](evidence/capacity-fixed.initial-miss.json) parked 1000 agents
but missed its 180-second completion deadline. The original runner lost its
partial raw samples; the surviving record states that limit. Later reports use
atomic checkpoints and retain failing groups.

The [local admission report](evidence/capacity-admission.json) contains three
repetitions for both fairness modes at each count. Worst observed p95 operation
times across those repetitions were:

| Waiters | Fixed enqueue / cancellation | Automatic enqueue / cancellation |
| ------- | ---------------------------- | -------------------------------- |
| 100     | 0.50 / 0.33 us               | 0.83 / 0.58 us                   |
| 1000    | 1.75 / 1.63 us               | 3.83 / 3.83 us                   |
| 4096    | 6.38 / 6.92 us               | 14.29 / 15.79 us                 |

All final local reservation accounts returned to zero. The measured queue work
does not support adding another queue index. It does not measure host registry
coordination or command execution.

### Matched coordinated capacity matrix

The 2026-10-19 [fixed](evidence/capacity-fixed.json.gz),
[automatic](evidence/capacity-auto.json.gz) and
[two-process](evidence/capacity-two-processes.json) reports use one frozen release
binary on the same Mac16,8 host. The binary is 7,368,736 bytes; its full hash is
in each report. Each participant has eight model slots, four job slots and the
same remaining hard ceilings. Each single-process case has three parked and
three mixed repetitions. All workloads completed with their expected outcomes
and command counts.

| Managed agents | Commands per mixed run | Fixed median / observed p95 | Auto median / observed p95 | Fixed / auto peak tree RSS |
| -------------- | ---------------------- | --------------------------- | -------------------------- | -------------------------- |
| 100            | 300                    | 1.288 / 1.309 s             | 1.386 / 1.417 s            | 50.5 / 51.2 MB             |
| 1000           | 3000                   | 13.209 / 13.302 s           | 13.814 / 14.328 s          | 60.7 / 60.9 MB             |
| 4096           | 12,288                 | 58.908 / 59.281 s           | 56.671 / 59.653 s          | 70.7 / 86.6 MB             |

MB denotes decimal bytes. With three samples, observed p95 is the maximum, not
stable tail evidence. Automatic median duration changed by +7.6%, +4.6% and
-3.8% respectively. This corresponds to useful-throughput changes of -7.1%,
-4.4% and +3.9%. Automatic mode used more sampled memory at 4096 agents. The
largest bounded page-plus-tail timer across these cases was 256 microseconds.

The two-process case used separate journals and one shared isolated registry.
Each process managed 1000 agents, 2000 together, and completed 3000 commands.
Their mixed execution phases took 14.678 and 14.418 seconds; the full group,
including setup, park, durable replay and view checks, took 16.382 seconds.
Per-process tree RSS peaks occur at different times and are not a measured
combined peak.

The [derived summary](evidence/capacity-summary.json) records input hashes,
samples, own-process CPU/I/O observations and journal timing differences. The
I/O observations can omit cached activity; own-process CPU excludes children.
These counters cover the interval after parking through final replay/view
checks. Registry state is reused within each invocation, so this matrix does
not isolate cold from learned starts.

The binary predates the final prepared-configuration rejection check. These
fixtures never change the prepared session/context ceilings. The check is
verified separately; this matrix is evidence for the recorded binary. It does
not establish the best safe static setting, changing pressure/quota behavior,
instrumentation overhead, idle CPU, target-laptop acceptance, CLI throughput,
or Linux/Windows native behavior. The original deadline miss remains linked
above. Automatic defaults remain gated on the broader acceptance matrix.

### Journal pressure and queue cost

Increase `--deltas` to send an exact number of small text deltas per response.
Zero preserves the original fixture's word chunks. For example:

```sh
target/release/ejm-harness-bench --agents 100 --rounds 0 --deltas 128 --park-ms 0
target/release/ejm-harness-bench --agents 4096 --admission-only --capacity fixed
target/release/ejm-harness-bench --agents 4096 --admission-only --capacity auto
```

Journal counters separate queue wait, serialized writer work, and successful
event-append commit time. They keep fixed-size histograms instead of raw content
or unbounded traces. Writer work also includes message commits and selected-event
publication. A large queue sum is time summed across waiters, not elapsed wall
time. Read the [timing contract](session-views.md#journal-durability-and-measurements)
before attributing throughput to disk synchronization.

The admission-only workload holds one local job slot, polls each pending
reservation once, then drops futures in reverse order. It reports per-operation
p50, p95, maximum, total nanoseconds, and final accounting. It measures queue
lookup and worst-position cancellation without native commands, providers,
journal writes, host grants, or task-spawn scheduling noise. Its `auto` choice
selects the local fairness policy only. It does not claim multi-process
coordination.

The [initial journal-pressure report](evidence/capacity-journal-pressure.baseline.json)
holds three legacy runs with 100 agents, no command rounds, and 128 text deltas
per response. Each run committed 13,200 events. Execution took 613, 614, and
1170 ms. Successful append commit time during the execution phase was 524 to
887 ms; total serialized writer work was 599 to 1057 ms. Sampled process-tree
RSS was 26.5 to 26.8 MB. These are differences between `journal_at_park` and
`journal_after_work`, excluding journal work during setup. This artificial
workload makes append commit dominant and supports evaluating bounded text
grouping. It does not establish the benefit of an implementation.

The coordinated reports above include current fixed and automatic timers plus
own-process CPU/I/O observations. CLI input, Stop and paint timing,
instrumentation overhead, old-laptop acceptance and cross-platform lifecycle
evidence remain separate checks.

#### Controlled journal grouping comparison

The 2026-10-19 [grouping report](evidence/capacity-journal-groups.json.gz) compares
release binaries built from one frozen private source copy. Only
`APPEND_BATCH_EVENTS` differs: one event versus at most 32. Normalized source
hashes match. Live sources were not changed. The report records source and binary
hashes, all 32 samples, journal counters, CPU/I/O observations, and sampled RSS.
Variant order alternated, with other owned builds and workloads paused.

| Workload                            | Runs per variant | Limit 1 median / p95 | Limit 32 median / p95 |
| ----------------------------------- | ---------------- | -------------------- | --------------------- |
| 100 agents, 128 deltas, no commands | 3                | 644 / 757 ms         | 333 / 388 ms          |
| 100 agents, three commands each     | 3                | 1184 / 1189 ms       | 1187 / 1188 ms        |
| One agent, 128 deltas, no commands  | 10               | 7 / 17 ms            | 7 / 9 ms              |

These are execution-phase timers, excluding the parked setup. Grouping reduced
the pressure fixture's median by 48.3%; median execution-phase append transactions
fell from 13,092 to 3,444. The mixed fixture's median rose by 3 ms, or 0.3%. The
one-agent median stayed at 7 ms. With these sample counts, nearest-rank p95 is
the largest observed sample and does not establish stable tail latency.

This isolates collection size within the same journal actor. It does not compare
the actor with the older closure implementation. The source snapshot predates
additive `Harness` and `ToolContext` native-owner forwarding, and this fixture uses
the legacy constructor without that owner. The result supports grouping under
this artificial journal load on one Mac; it does not establish automatic capacity
throughput, a general CPU or I/O improvement, target-laptop acceptance, or native
Linux/Windows performance. Durable barriers and failed-transaction correctness
remain separate test contracts.

### Recurring tracing comparison

The 2026-10-20 [raw tracing report](evidence/capacity-telemetry.macos-arm64.json.gz)
and [summary](evidence/capacity-telemetry-summary.macos-arm64.json) compare frozen
release binaries on one 12-logical-core Apple Silicon Mac with 24 GiB of memory
and macOS 26.5.2. Five alternating on/off pairs ran per workload with 100 managed
runs and fixed model/tool limits of 8/4. Other owned builds and workloads were
paused during the samples. Normalized source hashes match after applying the
embedded tracing-only patch.

The [formatting record](evidence/capacity-historical-formatting.macos-arm64.json)
preserves the original and formatted report hashes. The summary's input hash
identifies the original telemetry bytes; parsed measurements are unchanged.

The off variant elides recurring model/tool timing and outcome observations and
journal timing clocks. Both retain scheduling counters, credit release,
provider accounting, journal metric storage, grouping, collection clocks and
FULL durability. Fixed mode keeps control choices equal when feedback is
elided. This comparison does not isolate automatic tuning or all status costs.

| Workload                        | On/off work medians | Paired median change | Individual paired changes |
| ------------------------------- | ------------------- | -------------------- | ------------------------- |
| 128 deltas per run, no commands | 501 / 506 ms        | -1.01%               | -4.75% to +4.98%          |
| Three commands per run          | 1352 / 1348 ms      | +0.52%               | -1.32% to +10.76%         |

All 20 invocations matched managed counts, commands, durable events, configured
limits and final resource accounting. Pressure invocations committed 13,200
events; mixed invocations completed 300 commands and committed 3,200 events.
The two slower instrumented mixed samples were +10.76% and +6.95%. Five pairs
per workload cannot resolve a tracing cost below this spread or establish zero
overhead, a speed gain, stable tails or a performance-budget pass. Work timers
exclude the intentional one-second provider park; lane histograms include it.

The on/off binaries are 7,385,872 and 7,369,360 bytes. Exact commands, patch,
source/binary hashes, raw CPU/I/O/RSS samples and access requirements are in the
raw report. RSS sampling can miss short peaks and double-count shared pages;
own-process CPU excludes children. A zero-sample attempt stopped below the
unchanged 512 MiB free-disk guard and remains in the report. The successful batch
completed without a disk-guard failure. No paid provider was called.

### Database selection

Keep SQLite until a matched workload identifies the database engine as the
remaining constraint. Session journals already use one bounded writer per
journal. Host coordination records coarse participant grants and
provider charges; ordinary local admission and streamed text do not write the
shared capacity registry. Thousands of managed agents do not require thousands
of independent database writers.

The controlled grouping comparison above reduced the journal-pressure median
from 644 to 333 ms without changing engines. The mixed command median stayed
near 1.18 s. This supports reducing unnecessary commits before introducing a
new storage dependency; it does not establish that SQLite cannot become a
bottleneck under another workload.

A separate long CLI fixture exhausted its inherited 512 MiB main-database
budget. A replacement 4096-agent headless/interactive pair with a 2 GiB test-only
limit completed without failed turns. This distinguishes a configured storage
limit from engine throughput; it is not an engine comparison. The production
default remains 512 MiB. See the [journal limits](session-views.md#journal-durability-and-measurements)
before selecting a workload's storage envelope. The CLI owns the raw reports in
its `docs/evidence/capacity-live-4096.*.json` files, including the retained failure.

| Candidate                                         | Fit and remaining work                                                                                                                                                                                                        |
| ------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| SQLite through the existing `rusqlite` dependency | Retains indexed session queries, transactions, migrations and shared-process coordination. Measure lock wait, transaction work and commit time separately.                                                                    |
| LMDB through `heed`                               | A compact key-value candidate for a separate comparison. It still serializes writers and requires explicit memory-map and reader-lifetime management. Indexed relational projections and migrations would need new contracts. |
| `redb`                                            | A Rust embedded key-value candidate. Its shared multiprocess mode is experimental, which needs resolution before it could own the host registry.                                                                              |

The candidates have not been benchmarked against the harness. Any comparison
must preserve durability, crash recovery, bounded memory, private-state checks
and the same multi-process workload. Report startup, idle and peak memory,
binary/dependency size, useful throughput, tail latency and I/O. No database
migration has been started.

Primary references: [SQLite WAL](https://www.sqlite.org/wal.html),
[`heed` environment options](https://docs.rs/heed/latest/heed/struct.EnvOpenOptions.html)
and [`redb` database API](https://docs.rs/redb/latest/redb/struct.Database.html).
