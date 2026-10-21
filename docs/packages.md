# ejm-harness packages

ejm-cli demonstrates the package system. Other Rust, TypeScript, and Python
hosts can use the same public contracts without terminal dependencies or a
hosted workspace account. Registry source supports self-service publishing,
but no public registry endpoint is configured or verified as deployed.

## Contracts and ownership

| Component                              | Responsibility                                                             |
| -------------------------------------- | -------------------------------------------------------------------------- |
| `ejm-harness::Hook`                    | Ordered run, context, tool, result, and completion interception            |
| `ejm-harness-package-spec`             | Portable manifest and bounded archive validation                           |
| `ejm-harness-packages::PackageManager` | Exact snapshots, hashes, local discovery, and persisted grants             |
| `ejm-harness-packages::RuntimePool`    | At most one shared Node host and one shared Python host                    |
| `ejm-harness-packages::RegistryClient` | Fixed-origin HTTPS, account flow, immutable upload, verified download      |
| `ejm-harness-worker` and SDKs          | Explicit package configuration and typed host calls                        |
| ejm-cli                                | Installation, trust, slash commands, themes, prompts, skills, and widgets  |
| Cloudflare registry                    | Account ownership, immutable metadata and archives, quotas, and moderation |

Native Rust extensions are normal crates implementing public `Tool`, `Provider`,
or `Hook` ports. No unsafe dynamic library ABI is required. TypeScript and Python
packages export named functions. All languages use the same run loop.

## Author a package

Put `ejm.package.json` at the package root. This name is required across local
directories, archives, npm packages, and portable Python wheels. There is no
legacy alias. Rename earlier manifests and repack archives before installing
them; the new bytes need new sizes and hashes.

For example:

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

Registry sources require an explicitly configured client. Rust hosts pass
`Some(&client)` to `install_source` for an exact registry source, or `None` for
local, npm, PyPI, and Git sources. There is no default origin. Create the client
with `RegistryClient::new(origin)`. HTTPS or loopback HTTP, fixed origins,
redirect refusal, bounded responses, and exact-origin credentials remain
enforced. Selecting a new origin does not transfer tokens or authenticate it.

Current protected ejm-cli source needs a coordinated API and configuration update
before using this contract. Do not infer a registry endpoint from its older
compiled default. Check the CLI revision and configuration before registry
login, installation, publication, or credential migration. Local and exact
third-party sources need no registry configuration in the harness API.

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

For an existing private legacy session, stop older clients and confirm the exact
registry origin. Rust hosts use `migrate_registry_token(root, origin, true)`
after that confirmation. CLI users must first check that their revision selects
that same origin; the older compiled default is not configuration for a new
service. Never copy a session to another origin. An unconfirmed handoff refuses before key access or source
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

Package hashes establish integrity. Publishing under a namespace establishes
account ownership. Neither is a claim that community code is safe. Read the
[registry security and operations guide](../registry/README.md) before hosting
or changing registry trust controls.
