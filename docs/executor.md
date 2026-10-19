# ejm-harness execution

`ejm-harness-exec` supplies one resource governor and a native command executor. Every
session, child agent, native tool, and executor in a ejm-harness instance must
use the same `Arc<Governor>`. Creating a governor per agent defeats the limits.

## Admission

The governor accounts for active jobs, heavy jobs, CPU estimates in millicpus,
memory estimates, process estimates, and retained output bytes. It rejects
reservations that cannot fit the configured aggregate limits. Zero-valued
dimensions are allowed for native tools that need no such resource. Commands
must reserve positive CPU, memory, processes, and output.

Defaults permit four active jobs and one heavy job, with aggregate reservations
of 2000 millicpus, 512 MiB memory, 32 processes, and 1 MiB output. The queue holds
at most 256 requests. These are conservative defaults, not measured capacity
claims. Host applications should classify builds and tests as heavy and choose
their reservations. A model cannot increase configured limits.

Requests rotate between queued sessions, with FIFO order within each session.
A session's head cannot be bypassed by smaller reservations. This prevents
starvation and accepts some head-of-line blocking in exchange. A dropped waiter
is removed immediately. A lease releases every reserved dimension on drop.

## Native execution and capabilities

Native execution inherits the host environment with explicit per-command
overrides and uses an explicit working directory. Ordinary command execution
closes stdin. `Executor::run_with_input` accepts at most 8 MiB of private bytes,
writes them while both output pipes drain, then closes stdin. Input stays out of
serialized requests and artifacts. Its environment removals apply before explicit
request overrides. It provides no interactive terminal.
It never changes the host process's cwd or environment. Direct argv execution
does not interpret a shell language. A shell tool must select and invoke the
actual shell.

Capabilities report three states:

| State         | Meaning                                                          |
| ------------- | ---------------------------------------------------------------- |
| `enforced`    | The executor implements the restriction within its stated scope  |
| `monitored`   | The executor observes or reacts, with sampling or OS limitations |
| `unsupported` | The backend supplies no such restriction                         |

Native execution enforces admission, retained output limits, and its shared
artifact accounting. Resident memory and process count are sampled when the OS
supports process inspection. Exceeding the command's reservation triggers
termination and an error. Sampling can miss short-lived processes or transient
spikes. CPU reservations govern admission; the native backend does not set CPU
quotas. A single lazy sampler serves active commands across the governor. It
does not start a monitor process, shell, or thread for each managed agent and
does not inspect process environments or command-line arguments.

Linux sampling reads bounded `/proc/<pid>/stat` records and closes each file
immediately. `procfs-core` parses those records; our adapter retains the 16 KiB
read cap and uses raw start ticks to check process identities. It does not change
the host's resource limits or retain a descriptor for every host process.
Other supported systems use native process inspection through `sysinfo`.
Safe Rustix APIs supply Unix process signals, page size, and user identity.

The wall deadline includes queue time and is clamped to `max_job_ms`, which
defaults to 120 seconds. Cancellation or a deadline triggers process cleanup.
The deadline is reported as monitored because a native process in an
uninterruptible kernel operation may defer termination.

On Unix, commands enter their own process group before exec. Cleanup signals
that group, escalates to kill, and waits for the direct child. Observed
descendants are also killed after checking their recorded identities, including
children whose original parent has exited. Background children are cleaned up
even after an otherwise successful command. The caller cannot leave a daemon
behind by appending `&`.

Windows commands start suspended, join a kill-on-close Job Object, and resume
only after assignment. Cleanup kills the Job, reaps the root, and retains the
Job handle until a successful membership query reports no processes. Failed
queries keep cleanup pending. A Job assignment failure terminates the suspended
child and fails launch; the executor never falls back to an uncontained child.

Process groups and sampling are not escape-proof containment. A child can change
its group and become reparented between samples. The common capability contract
therefore keeps process-tree cancellation at `monitored`. See
[native process support](native-process-support.md) for the package choice,
low-level child API, nested-Job behavior, and native verification limits.
The native executor supplies no filesystem or network isolation and is never
called a security sandbox.

Every true field in `RequiredCapabilities` requires an enforced guarantee.
An unsupported or monitored requested restriction fails before admission and
launch. Linux cgroups, isolated helpers, and external
container/VM executors can be added behind explicit backend capability checks.
No backend is silently substituted for a requested isolation mode.

## Cancellation ownership

After admission, a Rust supervisor task owns the command and its lease. Dropping
`Executor::run`, including an engine cancelling a tool future, cancels that
supervisor. The supervisor retains the lease until child cleanup and output
drainer cleanup finish. It does not retry a command after cancellation or failure.
Host applications must keep the Tokio runtime alive to complete cleanup during
shutdown. After preventing new submissions and signalling every run's
cancellation token, call `Governor::wait_idle()` to await all queue removals and
lease releases. This barrier includes supervisors whose caller future was
dropped as well as native blocking tools holding leases.

Hosts that serialize workspace effects can use `Executor::run_with_retained`
to move an owned guard into the supervisor. The guard stays alive through child
and drainer cleanup even when the caller future is dropped. Ordinary `run`
uses the same path with an empty retained value.

Private-input execution uses the same supervisor. A child that ignores stdin
cannot block cancellation or output draining; its input writer is reaped with
the other pipe tasks. Hosts should pass credentials through their dedicated
auth interfaces rather than model input.

## Output and artifacts

Stdout and stderr drain concurrently in fixed-size chunks. Each receives half
the command's output reservation as a retained tail. Invalid UTF-8 is replaced
without allowing the returned strings to exceed that reservation. Noisy output
cannot grow the resident transcript without bound. `truncated` reports loss in
either retained tails, artifact output, or final pipe draining.

Output also spools into a private, newly created `exec-<uuid>.log` file. Records
identify the stream and original byte count. The last record may be incomplete
when the shared quota is reached. Raw output can contain binary data; consumers
must render it safely rather than executing terminal control sequences.

The retained artifact quota is shared across all executors using the governor,
including different artifact roots. Accounting charges actual record bytes and
a 4 KiB allowance per file so tiny artifacts cannot create unlimited files.
Existing regular files are counted when each root is first registered. Roots
must be dedicated directories owned by the host; external writes are outside
this accounting contract. Reopening an already registered root does not count
its files twice. Removing files externally does not reclaim credits until a
fresh governor scans the directory. Failed writes retain charged credits
conservatively. There is no automatic artifact deletion.

Once the artifact quota is exhausted, the executor keeps draining pipes and
retaining bounded tails. It does not block the child on a full output buffer.
Disk write errors fail the command after process cleanup. The quota bounds
accounted file bytes, not filesystem-specific physical disk allocation.

`peak_memory_bytes` and `peak_processes` are observed peaks for attributed live
processes. Zero means no sample was available. Fast commands may finish between
samples, and shared memory can be counted in more than one process. These values
must not be described as kernel-enforced limits or exact resource usage.

## Verification

The focused test suite checks session rotation, aggregate reservation dimensions,
queue limits, cancellation/drop release, one heavy job, 100 managed admissions,
dual-pipe noisy output, invalid UTF-8, shared artifact quotas, prelaunch rejection,
spawn errors, queue deadlines, ordinary descendant cleanup, abandoned
caller cleanup, parent exit with inherited pipes, and sampled process abuse.
Linux stat adapter fixtures cover parentheses and non-UTF-8 process names,
raw start ticks, malformed resident-page counts, and incomplete records. They run on
every host; actual Linux execution still needs Linux platform verification.
The lifecycle fixtures also compile for Windows, but local macOS runs do not
prove Windows Job cleanup or native Windows terminal behavior.
No hosted model or external service is used. OS isolation and old-machine
capacity claims need separate platform evidence.
