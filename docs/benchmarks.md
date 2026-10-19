# Performance evidence

The acceptance target is 100 managed agents using hosted model APIs on four
CPU cores, 8 GB RAM, and an SSD. Concurrency means many waiting agents share a
small pool of local commands. It does not mean 100 concurrent builds.

## Reproduce

The RSS and PTY benchmark runner requires Linux or macOS.

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
using an offline provider, 100x30 terminal, and a reused state directory.

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
