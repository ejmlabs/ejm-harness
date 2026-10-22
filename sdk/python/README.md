# ejm-harness for Python

<a href="https://harness.ejmlabs.com/"><img src="https://harness.ejmlabs.com/android-chrome-512.png" alt="Harness" width="96" height="96" /></a>

Dependency-free sync and async APIs for one persistent Rust worker shared across sessions. Requires Python 3.11 or newer and a compatible `ejm-harness-worker` on `PATH`, in `EJM_HARNESS_WORKER_PATH`, or at `worker_path`.

On Windows the default worker name is `ejm-harness-worker.exe`. Explicit
`worker_path` and environment paths are used as supplied. Build the native
worker from the same checkout; PowerShell setup and platform verification limits
are in the [SDK guide](https://github.com/ejmlabs/ejm-harness/blob/main/docs/sdk.md).

```python
import os

from ejm_harness import Harness

with Harness(config={
    "state_dir": ".ejm-harness",
    "provider": {
        "kind": "responses",
        "api_key": os.environ.get("OPENAI_API_KEY", ""),
        "model": os.environ.get("OPENAI_MODEL", ""),
    },
}) as harness:
    session = harness.create_session(".")
    run = session.start("Hello")
    for event in run:
        if event["type"] == "text_delta":
            print(event["text"], end="", flush=True)
    result = run.wait()
```

Use `async with AsyncHarness(...)` and await its methods in an existing event loop. `Tool(definition, handler)` registers sync or async host tools. Handlers receive progress and cooperative cancellation. Sync handlers use at most four threads per host.

Start and wait for 100 sessions concurrently without caller batching. Request admission allows 56 ordinary requests plus 8 reserved run controls, with bounded queues of 256 ordinary requests or 4 MiB and 8 controls or 1 MiB. Request timeouts include queue time. Sent request timeouts close the transport because the effect may be uncertain.

Cancelled callbacks keep their tool reservation and workspace effect lock until the handler stops. If cleanup takes more than five seconds, the host closes its worker and accepts no further work. Python cannot kill a running tool thread. Handlers must cooperate with cancellation and stop their own external work before a new host opens the same workspace.

Set `OPENAI_API_KEY` and `OPENAI_MODEL` through your host configuration. The
example sends requests to that model and uses its billing. Configure an HTTP or
trusted package provider explicitly; omission returns a setup error. The worker
no longer accepts `echo` or `replay` provider descriptors. This source release
does not bundle worker binaries or download them during installation. See
[the SDK guide](https://github.com/ejmlabs/ejm-harness/blob/main/docs/sdk.md) for
source setup, limits, effect grants, and recovery.

Load a trusted package through explicit configuration:

```python
with Harness(config={
    "provider": {
        "kind": "responses",
        "api_key": os.environ.get("OPENAI_API_KEY", ""),
        "model": os.environ.get("OPENAI_MODEL", ""),
    },
    "packages": [{
        "root": "/path/to/trusted-package",
        "grants": {"commands": ["report"], "widgets": ["status"]},
    }],
}) as harness:
    session = harness.create_session()
    print(harness.list_packages())
    print(session.invoke_package_command("@owner/package", "report", {}))
    print(session.render_package_widget("@owner/package", "status", {}))
```

The async API provides the same methods. Package tools and hooks use the existing Rust run loop. A package provider requires a named provider grant and `{"kind": "package", "package": "@owner/package", "name": "model"}`. One process per language serves all configured packages and sessions. Grants select exports; trusted native package code retains the worker user's operating system access. `reload_package()` requires idle runs and an unchanged manifest and grants. Widget methods return data for the caller to render.

Package authors export named functions from their entry module. Annotate the second argument with `ExtensionContext`: metadata uses mapping access such as `context["session_id"]`, with `is_cancelled()`, `raise_if_cancelled()`, async `progress()` and `delta()`, and `progress_sync()` and `delta_sync()` for synchronous handlers. `tool_output(content, data=..., is_error=False, artifact=None)` builds a tool result. Bundle dependencies with the package; installation does not run pip or a custom bootstrap.

## Capacity and session views

Capacity coordination is opt-in: pass `config={"capacity": {"mode": "auto"}}`,
or choose `"fixed"` for coordinated fixed targets. Omitting capacity keeps the
legacy limits within one worker. Automatic defaults remain gated on native
owner-crash recovery checks. Optional policy ceilings include `max_sessions`,
`max_model_requests`, `max_jobs`, and `max_memory_bytes`; explicit legacy limits
still apply. `capacity_status()` returns cached host targets and observations,
or `None` for legacy composition. `reset_capacity_learning()` clears aggregate
profiles and returns their count; it leaves grants, credentials, and sessions
intact.

`scheduling_status()` returns cached local managed, runnable, waiting and callback
counts through the additive `scheduling` worker capability. Async callers await
it; sync callers use it directly. It works with both legacy and coordinated
workers, and leaves `status()` unchanged. Runnable includes hooks and journal
work; callback counts can include internal admission and do not prove native
cleanup. See [field meanings](https://github.com/ejmlabs/ejm-harness/blob/main/docs/host-capacity.md#cached-scheduling-status).

HTTP providers can also set `provider_capacity` with a stable host-assigned
`account_uuid`, optional quota `family` (default `shared`), and optional
`max_in_flight`, `requests`, and `tokens` limits. Rates use `limit` and `window_ms`.
This requires explicit coordinated capacity and a current worker. Reuse the UUID
across participating processes sharing one account quota; never derive it from
credentials. Unknown provider hints remain unknown, and external clients are
outside this local accounting.

The sync and async APIs expose `sessions_page({"limit": 50})` with an opaque
`next` cursor and `resolve_session(prefix)` for an unambiguous identity.
`session.view(bounds={"max_events": 0})` reads current authority without
transcript history. `session.events_page(through, after=0)` replays through a
captured durable cursor, bounded by 200 events and 256 KiB per page. Advance by
the returned `cursor` until `complete`; inspect `omitted` for excluded history.
An oversized event leaves the cursor before that event. Increase `max_bytes`
within 256 KiB, or use a fresh view and show its omission marker.
These additive methods require a current worker. See the
[SDK guide](https://github.com/ejmlabs/ejm-harness/blob/main/docs/sdk.md#capacity-and-bounded-session-views)
for all ceilings, failure behavior, and recovery limits.

## License

Licensed under [MIT](LICENSE). Source archives and wheels include the full license text.
