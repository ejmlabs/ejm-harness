# ejm-harness packages

ejm-cli demonstrates the package system. Other Rust, TypeScript, and Python
hosts can use the same public contracts without terminal dependencies or a
hosted workspace account. Registry source supports self-service publishing,
but no public registry endpoint is configured or verified as deployed.

The registry browser pages share ejm-ui's native HTML theme and controls. Device
approval still requires a code you requested and confirms the full revocable
session permissions. See the [registry browser build](../registry/README.md#shared-browser-presentation)
for local assets, checks and deployment limits.

## Contracts and ownership

| Component                              | Responsibility                                                             |
| -------------------------------------- | -------------------------------------------------------------------------- |
| `ejm-harness::Hook`                    | Ordered run, context, tool, result, and completion interception            |
| `ejm-harness-package-spec`             | Portable manifest and bounded archive validation                           |
| `ejm-harness-packages::PackageManager` | Exact snapshots, hashes, local discovery, and persisted grants             |
| `ejm-harness-packages::RuntimePool`    | At most one shared Node host and one shared Python host                    |
| `ejm-harness-packages::RegistryClient` | Fixed-origin HTTPS, account flow, immutable upload, verified download      |
| `ejm-harness-worker` and SDKs          | Explicit package configuration and typed host calls                        |
| ejm-cli                                | Install/trust UI, command input, themes, prompts, skills, widget rendering |
| Cloudflare registry                    | Account ownership, immutable metadata and archives, quotas, and moderation |

Native Rust extensions are normal crates implementing public `Tool`, `Provider`,
or `Hook` ports. No unsafe dynamic library ABI is required. TypeScript and Python
packages export named functions. All languages use the same run loop.

Shared package APIs own installation mechanics, grants, callback execution,
admission, language-host lifecycle and cleanup. Hosts choose trust and
configuration, accept command input and render results or widgets. Themes,
prompts and display models remain host presentation. Reuse the public runtime
instead of copying its registry transport, process pools or grant policy into a
frontend. The worker and SDKs adapt these same contracts. Apply the
[package boundary rules](../AGENTS.md#package-boundaries) and check affected owner
and consumer contracts when those interfaces change.

## Author a package

Put `ejm.package.json` at the package root:

The manifest name is required across local directories, archives, npm packages,
and portable Python wheels. Rename earlier manifests and repack archives before
installing them. This changes archive bytes, sizes, and hashes.

```json
{
    "schema_version": 1,
    "name": "@example/workflow",
    "version": "1.0.0",
    "description": "A project workflow",
    "runtime": { "kind": "node", "entry": "extension.ts" },
    "exports": {
        "tools": [
            {
                "name": "word_count",
                "description": "Count words in supplied text",
                "input_schema": {
                    "type": "object",
                    "properties": { "text": { "type": "string" } },
                    "required": ["text"],
                    "additionalProperties": false
                },
                "effect": "read",
                "version": "1.0.0",
                "handler": "countWords"
            }
        ],
        "hooks": [{ "event": "run_start", "handler": "prepare" }],
        "providers": [{ "name": "offline", "handler": "respond" }],
        "commands": [{ "name": "hello", "description": "Say hello", "handler": "hello" }],
        "themes": ["themes/midnight.json"],
        "widgets": [{ "name": "status", "handler": "status" }],
        "skills": ["skills/review/SKILL.md"],
        "prompts": ["prompts/review.md"]
    }
}
```

Omit `runtime` for packages containing only assets. Code packages use Node 24 or
later, or Python 3.11 or later. Node can strip supported TypeScript syntax.
Bundle dependencies and compiled output when distributing code that needs them.
The manager does not run dependency installation or lifecycle scripts.

TypeScript authors can import `ExtensionContext` and `ToolOutput` types from
`ejm-harness`. Python authors can use `ExtensionContext`, `ToolOutput`, and
`tool_output` from `ejm_harness`. A type-only import does not load the SDK at
runtime. Runtime context supplies cooperative cancellation, progress, and
provider text deltas.

### Callback arguments and results

| Export         | Arguments                                 | Result                                                     |
| -------------- | ----------------------------------------- | ---------------------------------------------------------- |
| Tool           | Declared tool arguments                   | `ToolOutput`                                               |
| Provider       | `ModelRequest`                            | `ModelResponse`; optionally stream through context `delta` |
| `run_start`    | `{prompt}`                                | Prepared prompt string                                     |
| `before_model` | `{messages}`                              | Prepared message array                                     |
| `before_tool`  | `{call}`                                  | `{action:"execute",arguments}` or `{action:"skip",output}` |
| `after_tool`   | `{call,output}`                           | Prepared `ToolOutput`                                      |
| `run_end`      | `{summary}`                               | No result                                                  |
| Command        | Host-defined JSON; CLI supplies `{input}` | JSON or text                                               |
| Widget         | Host-defined JSON; CLI supplies `{}`      | CLI accepts text or `{text}`                               |

Hooks run in registration order, with at most 32 registered hooks. The engine
revalidates transformed tool arguments and context. Hooks cannot rename calls,
grant tools, forge tool exchanges, remove the latest intent or failure evidence,
or alter provider continuation. The original confirmed tool result remains
durable separately from its prepared model view. Completion observers share a
one-second bound after durable completion and cannot change the outcome.

## Install and grant

```sh
ejm-cli package install ./my-package
ejm-cli package install ./my-package.tgz
ejm-cli package --registry "$EJM_REGISTRY_URL" install @publisher/package@1.2.3
ejm-cli package install npm:@publisher/package@1.2.3
ejm-cli package install pypi:distribution==1.2.3
ejm-cli package install 'git:https://github.com/publisher/package.git#FULL_COMMIT_HASH'
ejm-cli package list
ejm-cli package enable @example/workflow --trust \
  --tool word_count --hook run_start --grant-provider offline --command hello --grant-widget status
ejm-cli --package-provider @example/workflow:offline \
  --theme @example/workflow:midnight --widget @example/workflow:status
```

`package`, `packages`, and `pkg` refer to the same command. Installs and updates
create an exact archive snapshot and start disabled. `enable --trust` records
grants for that version. Updating clears them. There is no automatic startup
download, project-file activation, or dependency execution. Remote references
take precedence over repository filenames. Use `local:` or `./` to install a
local file whose name resembles a remote reference.

Registry operations require `--registry` or `EJM_REGISTRY_URL` with an approved
HTTPS origin. Loopback HTTP is accepted for local development. There is no
default origin. The CLI validates supplied configuration before opening package
state, reading credentials, or contacting a service. Local, npm, PyPI, and Git
sources work without registry configuration. A Rust host passes `Some(&client)`
to `install_source` for registry sources or `None` for other sources.
`RegistryClient::new(origin)` keeps requests fixed to that origin without
redirects. Tokens remain scoped to their exact vault origin; selecting another
origin does not transfer credentials or authenticate that service.

Resolution uses maintained HTTP and archive libraries. No Git, npm, uv, Python,
package lifecycle script, or build backend runs during installation. Metadata is
limited to 256 KiB and downloads to 8 MiB before anything is written. Redirects
and credentials in download URLs are rejected. Download and decode admission use
the host's shared governor.

- `git:` accepts public GitHub repositories pinned to a full 40-character commit.
  It reads the matching HTTPS commit archive. Other Git hosts, Git credentials,
  LFS materialization, and submodules require a prepared local bundle.
- `npm:` uses the public npm registry, exact semver, and a matching SHA-512
  integrity field. It does not read `.npmrc`, install dependencies, or use npm's cache.
- `pypi:` uses public PyPI, its exact reported version, and the index's SHA-256
  digest and byte length. It accepts one unyanked `py3-none-any` or
  `py2.py3-none-any` wheel. Native wheels, source distributions, `.pth` startup
  hooks, and `.data` relocation layouts require a prepared local bundle.
  Wheels must contain exactly one manifest within eight directory levels.
  Python version requirements and imports remain the package author's and host's
  responsibility. Installation does not add the wheel to global `sys.path`.

Private registries and other unsupported layouts can be bundled locally, inspected,
and installed with `local:`. Download integrity establishes the selected bytes;
it does not establish that community code is safe.

Names are lowercase ASCII `@publisher/package`; versions use canonical semver.
Archives are `.tar.gz`, with a 16 KiB manifest, 8 MiB compressed limit, 32 MiB
expanded limit, and 1,024 entries. Traversal, links, special files, duplicate or
case-conflicting paths, hidden components, Windows aliases, and absent declared
assets fail before installation. Archive permissions never become local modes.

Installed files are checked against the pinned archive before loading. Atomic
snapshot and lockfile writes preserve the prior registration after a failed
install. Remove and disable affect future hosts; active hosts retain their
snapshot until shutdown. Unreferenced snapshots are retained for active hosts.

Install, enable, disable and remove hold a package-store mutation lock. Its guard
attempts an explicit unlock when the operation returns or unwinds, even if a
duplicate file description remains open. Acquisition and contention errors stay
unchanged, and Windows retains its pinned lock anchor. Abrupt process death can
bypass guard cleanup. Native lifetime witnesses remain locked until ownership
ends.

The Windows store uses pinned owner-only directories and a stable mutation lock.
New staging files inherit an owner-only ACL, and lockfile publication is relative
to the retained directory handle. Junctions and other reparse points fail
verification. Existing directories with a wider ACL are refused; choose a
private application-data location rather than a shared folder. Archive reads
validate the opened regular file and refuse a final symlink. Source directories
may contain spaces and Unicode; archive entry names retain the portable ASCII
contract. Native Windows attack and durability acceptance remains outstanding;
see [the platform evidence](native-process-support.md).

### CLI interaction

- `/extensions` lists enabled packages and registration grants.
- `/hello INPUT` invokes an unambiguous granted command. Qualified names use
  `/@publisher/package:hello INPUT`. Built-in commands keep precedence.
- `/theme NAME` selects an enabled theme. `NO_COLOR` remains authoritative.
- `/prompt NAME INPUT` and `/skill NAME INPUT` add the asset and input to a run.
- `/reload @publisher/package` reloads fixed exports while all runs and callbacks
  are idle. Manifest or grant changes require restarting the host.
- `/widgets pause` and `/widgets resume` control selected widgets.

Commands and widgets run in background tasks. The CLI admits one package command
at a time, displays at most four widgets, and refreshes widgets at most once a
second. Widget failure pauses refresh until explicit retry. Widget text is
bounded to 2 KiB each; command text is bounded to 64 KiB. Terminal control
sequences remain untrusted text. Theme colors cannot change terminal modes or
keyboard behavior.

## Reuse from a host

Rust hosts instantiate `RuntimePool`, load `InstalledPackage` with `PackageGrants`,
then inject its tools, providers, and hooks through `Harness::with_hooks` or
`Harness::with_policies_and_hooks`. `PackageContext` supplies the existing
governor for commands and widgets. Call `pool.shutdown().await` after harness
shutdown and before dropping the Tokio runtime.

Hosts opting into shared capacity prepare `ejm_harness::CapacityContext` before package code loads, construct `RuntimePool::with_capacity(config, capacity.clone())`, then use that same context in `Harness::with_capacity`. `RuntimePool::new` keeps its legacy behavior and all existing `RuntimeConfig` literals remain valid. Commands, hooks, widgets, and tools in a coordinated pool reject a different governor.

The coordinated path sends each language generation through the prepared
context's lazy native owner, with duplex pipes and the requesting process's cwd
and environment. The host executable must dispatch `native::dispatch_helper()`
before ordinary startup, or prepare capacity with an explicit compatible helper
through `CapacityContext::prepare_with_native_helper`. Scope links trigger
cleanup on client loss while the owner survives. No interpreter starts while a
host only reads capacity status.

A coordinated pool stages the complete startup peak before launching each lazy language host: one process, 64 MiB of resident memory, a transient tool slot, and 16 MiB of import scratch plus bounded output. Once admitted, it transfers the resident memory and process charge to the host supervisor without changing aggregate usage or waiting for a second lease. The supervisor retains those charges and its capacity context through native cleanup. Callbacks reserve the scratch estimate without charging resident memory again; completing one releases its execution slot while the host's resident charge remains. These are admission estimates, not hard native memory enforcement or measured interpreter peaks. Packages can run arbitrary native effects as the user.

The working helper-accounting implementation obtains a separate coarse helper
commitment after staging the full startup demand and before activating its
lease. If the grant changes or activation must wait, the pool releases its
unused readiness guard and retains the same queue ticket. The interpreter
charge above does not cover the helper's provisional 32 MiB baseline. That charge stays
independent of the creating client until supported lifetime evidence permits
release. See [helper commitments](host-capacity.md#helper-commitments) for pool
bounds and protocol migration. Startup-peak calibration and crash/pool checks
remain open, so automatic defaults stay gated.

Reload reaps the old idle host before admitting a replacement, so this runtime does not overlap host generations. Failed imports stop the affected host. Cancellation while waiting for admission launches no interpreter. Shutdown attempts every initialized language host; cleanup errors retain native ownership and capacity while the supervisor retries. Keep Tokio alive while cleanup is pending. Native owner-crash recovery remains a separate release gate, and Unix process groups do not prove containment of escaped descendants.

If provider lookup, adapter registration, or final harness construction fails
after package loading, call `pool.shutdown().await` before returning from host
setup. A failed `Harness::with_capacity` call does not own or shut down a shared
pool. Preserve the setup error and report any cleanup error separately. Keep the
pool, prepared capacity context, and Tokio runtime available while cleanup is
pending. Dropping a load or reload future requests cancellation; its supervisor
retains the import and resident reservations until supported native cleanup ends.
Load and reload errors retain the original import failure in their error chain
when cleanup also reports that it is pending.

For normal shutdown, request harness cancellation first, await its execution
cleanup, then shut down the package hosts. Attempt both cleanup paths even when
one fails, and retain the prepared capacity context through both attempts:

```rust
async fn shutdown_packages(
    harness: &ejm_harness::Harness,
    pool: &ejm_harness_packages::RuntimePool,
) -> anyhow::Result<()> {
    harness.request_shutdown();
    let runs = harness.shutdown().await;
    let hosts = pool.shutdown().await;
    runs?;
    hosts
}
```

A cleanup timeout means the caller stopped waiting. It does not authorize a
resident memory or process refund. A paused native owner retains its lifetime
lock and active scope charges; it can finish cleanup when it resumes.
Confirmed owner death permits retirement of that generation's scheduling
authority before fresh capacity observations restore admission. It does not
prove descendant cleanup or workspace safety. Affected imports and callbacks
fail, and recovering package registrations requires an explicit reload.

### Initial package-host measurement

The local macOS arm64 release fixture ran three sequential samples for each
language on 2026-10-20 with Node 23.10.0 and Python 3.15.0. Other owned builds and
workloads were paused. Each sample prepared isolated capacity, started a fresh
native owner, imported a fixture package, sampled two seconds of idle use, then
verified host cleanup and native-owner idle exit.

| Metric                                             | Node                        | Python                      |
| -------------------------------------------------- | --------------------------- | --------------------------- |
| Capacity preparation, median                       | 32.29 ms                    | 25.09 ms                    |
| Cold owner, interpreter startup and import, median | 88.15 ms                    | 105.58 ms                   |
| Idle interpreter RSS, median                       | 45.4 MiB                    | 27.1 MiB                    |
| Idle interpreter CPU, sampled                      | 0% of one core              | 0% of one core              |
| Idle native-owner CPU, observed range              | 9.49% to 12.60% of one core | 9.01% to 10.50% of one core |
| Host shutdown, median                              | 2.14 ms                     | 2.34 ms                     |

All six samples retained one 64 MiB resident reservation, one process charge,
and zero active execution jobs while idle. The native owner failed the target
of less than 1% of one core during low load. That version scanned host process
topology and memory every 100 ms while the resident host existed. The initial
report remains in
[package-host overhead evidence](evidence/package-host-overhead.macos-arm64.initial.json),
including source and binary hashes, raw samples, caller and owner measurements,
and the failed target.

Three samples and two second CPU intervals provide limited local evidence.
Interpreter RSS varied between 45.3 and 45.5 MiB for Node and 9.7 and 27.3 MiB for
Python. These are idle samples; startup and complete process-tree memory peaks
remain unmeasured. Caller measurements include the test runtime and capacity
coordinator. Linux and Windows measurements remain open.

### Native macOS sampler measurement

A second run used `libproc` 0.14.11 for macOS process sampling. It kept the
100 ms monitor interval and host-wide PID enumeration, with one typed native
observation per readable PID. The sampler reads no process arguments,
environments, or thread records. Three sequential samples per language used
the same release fixture and an owned-workload quiet window on 2026-10-20.

| Metric                                             | Node                       | Python                     |
| -------------------------------------------------- | -------------------------- | -------------------------- |
| Capacity preparation, median                       | 28.58 ms                   | 29.00 ms                   |
| Cold owner, interpreter startup and import, median | 81.87 ms                   | 114.46 ms                  |
| Idle interpreter RSS, median                       | 45.5 MiB                   | 27.3 MiB                   |
| Idle interpreter CPU, sampled                      | 0% of one core             | 0% of one core             |
| Idle native-owner CPU, observed range              | 1.81% to 1.93% of one core | 1.63% to 1.72% of one core |
| Host shutdown, median                              | 1.68 ms                    | 1.27 ms                    |

The owner used less CPU than the initial version, but still failed the target
of less than 1% of one core. Every idle host retained the same 64 MiB and
one-process reservation, with zero active execution jobs. Host cleanup and
native-owner idle exit passed in every sample.

Cold owner, interpreter startup and import ranged from 81.22 to 252.85 ms for
Node and 109.31 to 115.73 ms for Python. These six samples do not establish
stable latency tails, startup memory peaks, or behavior on another host.
The [native-sampler report](evidence/package-host-overhead.macos-arm64.native-sampler.json)
preserves all samples and the failed CPU target. Its hashes were captured
after measurement; one capacity source edited later has no run hash. The
initial report remains unchanged. Linux and Windows measurements remain open.

### Scoped macOS memory sampling

A third release run kept host-wide PID and birth/parent observations every
100 ms, but read resident memory only for owned roots, retained identities and
their discovered descendants. It used the same `libproc` 0.14.11 dependency,
three sequential samples per language and two-second idle intervals during
an owned-workload quiet window on 2026-10-20.

| Metric                                             | Node                       | Python                     |
| -------------------------------------------------- | -------------------------- | -------------------------- |
| Capacity preparation, median                       | 32.72 ms                   | 33.33 ms                   |
| Cold owner, interpreter startup and import, median | 82.71 ms                   | 117.54 ms                  |
| Idle native-owner RSS, median                      | 7.25 MiB                   | 7.38 MiB                   |
| Idle interpreter RSS, median                       | 45.4 MiB                   | 27.2 MiB                   |
| Idle native-owner CPU, observed range              | 0.70% to 0.80% of one core | 0.75% to 0.78% of one core |
| Host shutdown, median                              | 1.55 ms                    | 1.50 ms                    |

All six short samples met the below-1% owner CPU target and verified interpreter
cleanup and native-owner idle exit. Each interpreter retained 64 MiB and one
process reservation while idle, with zero active execution jobs. The helper's
separate accounting gap remains. Cold owner/startup/import ranged from 81.54 to
290.66 ms for Node and 114.52 to 118.24 ms for Python. Three samples per language
do not establish latency tails, startup or process-tree memory peaks, or the
separate 60-second CLI idle CPU target.

The [scoped-sampler report](evidence/package-host-overhead.macos-arm64.scoped-sampler.json)
records the release binaries, all samples and matching source hashes before
and after build and measurement. Both earlier failed CPU reports remain above.
The memory reads are narrower; process topology is still host-wide. Sampling
can miss descendants reparented before discovery, and a birth check followed
by a signal is not atomic. Native sampling is monitored behavior.

### Sustained idle measurement

The same native helper binary then ran one configured 60-second idle sample
per language, using a rebuilt release fixture. The two samples, startup,
shutdown and owner idle exit took 124.73 seconds in total. The shared build
lock was held throughout the measurement; unrelated host activity remained
uncontrolled.

| Metric                     | Node               | Python             |
| -------------------------- | ------------------ | ------------------ |
| Native-owner CPU, one core | 0.978736%          | 1.145687%          |
| Native-owner idle RSS      | 6.58 MiB           | 6.72 MiB           |
| Interpreter CPU, one core  | 0%                 | 0%                 |
| Owner CPU target below 1%  | Passed this sample | Failed this sample |

The sustained owner CPU gate failed. Both samples retained the expected
interpreter reservation with zero active jobs and verified interpreter cleanup,
reservation release and native-owner idle exit. Source and binary hashes
matched before and after build and measurement. The
[long idle report](evidence/package-host-idle.macos-arm64.scoped-sampler.json)
preserves the failed target, samples and provenance. One sample per language
does not establish repeatability, and these package-process measurements do
not establish settled CLI CPU, startup memory peaks or another OS's behavior.

The ignored development fixture can be rerun after building the native helper:

```sh
cargo build -p ejm-harness-exec --example native_owner_helper --release --locked
cargo test -p ejm-harness-packages --test runtime --release --locked \
  lifecycle::measure_coordinated_language_host_overhead -- \
  --ignored --exact --nocapture --test-threads=1
cargo test -p ejm-harness-packages --test runtime --release --locked \
  lifecycle::measure_coordinated_language_host_idle -- \
  --ignored --exact --nocapture --test-threads=1
```

`RuntimeConfig` defaults to `node` and to `python` on Windows or `python3` on
macOS and Linux. Hosts may pass explicit interpreter paths. An installed
interpreter must meet the package runtime's documented version; no interpreter
is bundled or downloaded by the runtime.

Worker configuration accepts `packages: [{root, grants}]`. This is an explicit
host trust decision and may use a development directory. A package provider is
`{kind:"package",package:"@example/workflow",name:"offline"}`. Listing is paged;
commands and widgets require a real session ID. Configure imports share a
20-second bound. The SDKs expose package listing, reload, commands, and widgets
through this worker, so no second engine is involved.

## Security and resource boundaries

### Registry credentials

Registry account sessions use the shared encrypted credential vault in
`<package-root>/credentials`. The exact registry origin identifies a scoped
`ejm-harness-packages` / `registry-session` record. `save_registry_token`,
`load_registry_token`, and `delete_registry_token` are synchronous host
boundaries; call them on a blocking worker when the host has a UI or async loop.
They require the current user's OS key service. They never create a plaintext
`registry-session.json` or fall back to one when vault access fails.

For an existing private legacy session, stop older clients and run
`ejm-cli package migrate-credentials --old-clients-stopped` for the same registry
origin. Rust hosts use `migrate_registry_token(root, origin, true)` after the
same confirmation. An unconfirmed handoff refuses before key access or source
changes. Migration validates the source's privacy and origin, commits its
encrypted record and permanent marker, reopens the authenticated state, then
retires the source. Interrupted cleanup can be retried. A marker survives
logout and replacement, preventing any stale reimport from a leftover source.
An existing vault with a missing key requires recovery of the original OS
record. Read [credential storage](auth.md#storage-and-renewal) for key setup,
limits, headless Linux behavior, and the storage trust boundary. Native OS
acceptance remains required before support claims.

### Native package code

**Native packages run arbitrary code as the current user.** Export grants
control harness registration. They do not restrict filesystem or network access,
secret access, subprocesses, or language allocations. Use an external sandbox
when a hard security boundary is required.

One lazy process per language is shared across sessions and packages. Idle
hosts do not consume active command slots. Each tool, hook, command, and widget
call uses the host governor. Provider calls use model-request admission. The
runtime limits pending calls, frame bytes, output queues, import count, and
cleanup time. A dropped caller retains admission and effect ownership until
acknowledgement or actual language-host reap. An uncooperative extension stops
its language host, which can affect other packages in that process. Idle reload
rebuilds the language host to reclaim module state. Python bootstrap runs in
isolated mode so repository files cannot shadow standard-library imports.

Language hosts use the shared native child owner. Unix hosts enter a fresh
process group; Windows hosts join a Job Object before running user code.
Shutdown retains pending calls, admission, and effect guards through cleanup.
Windows cleanup must confirm an empty Job, including descendants after root
exit. A failed membership query keeps cleanup pending; a shutdown timeout means
the host must retain its Tokio runtime. These mechanisms do not isolate trusted
package code from host files or networks. See
[native process support](native-process-support.md) for the full contract and
native verification limits.

Package hashes establish integrity. Publishing under a namespace establishes
account ownership. Neither is a claim that community code is safe. Read the
[registry security and operations guide](../registry/README.md) before hosting
or changing registry trust controls.
