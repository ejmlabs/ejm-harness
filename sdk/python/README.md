# ejm-harness for Python

Dependency-free sync and async APIs for one persistent Rust worker shared across sessions. Requires Python 3.11 or newer and a compatible `ejm-harness-worker` on `PATH`, in `EJM_HARNESS_WORKER_PATH`, or at `worker_path`.

```python
from ejm_harness import Harness

with Harness(config={"state_dir": ".ejm-harness"}) as harness:
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

The default provider returns `Demo: ...` without model charges. Configure a hosted provider for real work. This source release does not bundle worker binaries or download them during installation. See [the SDK guide](https://github.com/ejmlabs/ejm-harness/blob/main/docs/sdk.md) for source setup, limits, effect grants, and recovery.

Load a trusted package through explicit configuration:

```python
with Harness(config={
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

## License

Licensed under [MIT](LICENSE). Source archives and wheels include the full license text.
