"""SDK acceptance tests against the actual Rust worker, without model charges."""

from __future__ import annotations

import asyncio
import json
import os
import tempfile
import threading
import unittest
from pathlib import Path
from typing import Any, cast
from unittest.mock import Mock, patch

from ejm_harness import (
    AsyncHarness,
    Harness,
    HarnessError,
    Tool,
    ToolContext,
    ToolOutput,
    tool_output,
)
from ejm_harness.protocol import (
    ConfigureParams,
    ProviderConfig,
    ProviderQuotaConfig,
    ToolDefinition,
)

WORKER = os.environ.get(
    "EJM_HARNESS_WORKER_PATH",
    str(
        Path(__file__).resolve().parents[3]
        / "target/debug"
        / ("ejm-harness-worker.exe" if os.name == "nt" else "ejm-harness-worker")
    ),
)
DEFINITION: ToolDefinition = {
    "name": "sdk_lookup",
    "description": "Look up a test value",
    "effect": "read",
    "version": "1",
    "input_schema": {
        "type": "object",
        "properties": {"value": {"type": "string"}},
        "required": ["value"],
    },
}
IDLE_SCHEDULING = {
    "managed_runs": 0,
    "runnable_runs": 0,
    "model_waiting": 0,
    "model_preparing": 0,
    "provider_waiting": 0,
    "model_running": 0,
    "tool_dispatch_waiting": 0,
    "workspace_waiting": 0,
    "tool_resource_waiting": 0,
    "tool_running": 0,
}


def recorded_responses(calls: int = 1) -> list[dict[str, Any]]:
    return [
        {
            "tool_calls": [
                {
                    "id": f"python-call-{index}",
                    "name": "sdk_lookup",
                    "arguments": {"value": "hello"},
                }
            ]
        }
        for index in range(calls)
    ] + [{"content": "finished"}]


def provider_fixture(state: str, responses: list[dict[str, Any]] | None = None) -> ConfigureParams:
    root = Path(state) / "fixture-provider"
    root.mkdir(exist_ok=True)
    (root / "ejm.package.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "name": "@fixture/sdk-recorded",
                "version": "1.0.0",
                "description": "Internal SDK provider fixture",
                "runtime": {"kind": "python", "entry": "entry.py"},
                "exports": {"providers": [{"name": "model", "handler": "model"}]},
            }
        )
    )
    (root / "entry.py").write_text(f"""
import json
responses = json.loads({json.dumps(responses)!r})
async def model(request, context):
    if responses is not None:
        if not responses:
            raise RuntimeError("fixture responses exhausted")
        return responses.pop(0)
    user = next(message for message in reversed(request["messages"]) if message["role"] == "user")
    return {{"content": "Fixture: " + user["content"]}}
""")
    return {
        "state_dir": state,
        "packages": [{"root": str(root), "grants": {"providers": ["model"]}}],
        "provider": {"kind": "package", "package": "@fixture/sdk-recorded", "name": "model"},
    }


def callback_config(state: str, calls: int = 1) -> ConfigureParams:
    return provider_fixture(state, recorded_responses(calls))


def python_package(root: Path) -> ConfigureParams:
    manifest = {
        "schema_version": 1,
        "name": "@fixture/sdk-python",
        "version": "1.0.0",
        "description": "Offline Python SDK package fixture",
        "runtime": {"kind": "python", "entry": "entry.py"},
        "exports": {
            "providers": [{"name": "model", "handler": "model"}],
            "hooks": [{"event": "run_start", "handler": "prepare"}],
            "commands": [
                {"name": "count", "description": "Inspect shared state", "handler": "count"}
            ],
            "widgets": [{"name": "status", "handler": "widget"}],
        },
    }
    (root / "ejm.package.json").write_text(json.dumps(manifest))
    (root / "entry.py").write_text("""
import os
calls = 0
def prepare(args, ctx):
    return args["prompt"] + "|python"
async def model(request, ctx):
    return {"content": "custom:" + request["messages"][-1]["content"]}
def count(args, ctx):
    global calls
    calls += 1
    return {"calls": calls, "pid": os.getpid(), "session_id": ctx["session_id"]}
async def widget(args, ctx):
    return {"kind": "text", "content": args.get("label", "status")}
""")
    return {
        "state_dir": str(root / "state"),
        "packages": [
            {
                "root": str(root),
                "grants": {
                    "providers": ["model"],
                    "hooks": ["run_start"],
                    "commands": ["count"],
                    "widgets": ["status"],
                },
            }
        ],
        "provider": {"kind": "package", "package": "@fixture/sdk-python", "name": "model"},
    }


class AsyncSdkTests(unittest.IsolatedAsyncioTestCase):
    async def test_missing_and_removed_providers_fail_before_packages_and_journals(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            state = Path(folder) / "state"
            with self.assertRaisesRegex(HarnessError, "provider is required"):
                await AsyncHarness.create(
                    worker_path=WORKER,
                    config={
                        "state_dir": str(state),
                        "packages": [{"root": "/unused-provider-fixture", "grants": {}}],
                    },
                )
            for kind in ["echo", "replay"]:
                config = cast(
                    "ConfigureParams",
                    {"state_dir": str(state), "provider": {"kind": kind, "responses": []}},
                )
                with self.assertRaises(HarnessError) as caught:
                    await AsyncHarness.create(worker_path=WORKER, config=config)
                self.assertEqual(caught.exception.code, "invalid_request")
            self.assertFalse(state.exists())

    async def test_provider_quota_requires_capability_and_coordinated_mode(self) -> None:
        quota: ProviderQuotaConfig = {"account_uuid": "00000000-0000-4000-8000-000000000001"}
        request = AsyncHarness._request

        async def without_quota(
            harness: AsyncHarness, method: str, params: object, *, during_close: bool = False
        ) -> object:
            result = await request(harness, method, params, during_close=during_close)
            if method == "hello":
                value = cast("dict[str, Any]", result)
                return {
                    **value,
                    "capabilities": [
                        item for item in value["capabilities"] if item != "provider_capacity"
                    ],
                }
            return result

        with (
            patch.object(AsyncHarness, "_request", without_quota),
            self.assertRaisesRegex(HarnessError, "supports provider_capacity"),
        ):
            await AsyncHarness.create(
                worker_path=WORKER,
                config={"capacity": {"mode": "auto"}, "provider_capacity": quota},
            )
        with self.assertRaisesRegex(HarnessError, "explicit capacity mode"):
            await AsyncHarness.create(worker_path=WORKER, config={"provider_capacity": quota})

    async def test_capacity_opt_in_requires_worker_capability(self) -> None:
        request = AsyncHarness._request

        async def without_capacity(
            harness: AsyncHarness, method: str, params: object, *, during_close: bool = False
        ) -> object:
            result = await request(harness, method, params, during_close=during_close)
            if method == "hello":
                return {**cast("dict[str, object]", result), "capabilities": []}
            return result

        with (
            patch.object(AsyncHarness, "_request", without_capacity),
            self.assertRaisesRegex(HarnessError, "supports capacity"),
        ):
            await AsyncHarness.create(worker_path=WORKER, config={"capacity": {"mode": "auto"}})

    async def test_bounded_views_preserve_authority_and_page_the_full_set(self) -> None:
        with tempfile.TemporaryDirectory() as state:
            async with AsyncHarness(worker_path=WORKER, config=provider_fixture(state)) as harness:
                self.assertIsNone(await harness.capacity_status())
                first = await harness.create_session(title="page first")
                second = await harness.create_session(title="page second")
                page = await harness.sessions_page({"limit": 1})
                self.assertEqual(len(page["sessions"]), 1)
                self.assertIsNotNone(page["next"])
                following = await harness.sessions_page({"limit": 1, "cursor": page["next"]})
                self.assertEqual(len(following["sessions"]), 1)
                self.assertNotEqual(page["sessions"][0]["id"], following["sessions"][0]["id"])
                self.assertEqual((await harness.resolve_session(second.id)).id, second.id)
                with self.assertRaises(HarnessError):
                    await harness.sessions_page({"query": "different", "cursor": page["next"]})
                run = await first.start("bounded offline history")
                summary = await run.wait()
                view = await first.view(bounds={"max_events": 1})
                self.assertEqual(len(view["events"]), 1)
                self.assertTrue(view["omitted"])
                self.assertIsNone(view["run_id"])
                transition = view["last_transition"]
                assert transition is not None
                self.assertEqual(transition["run_id"], run.id)
                self.assertEqual(transition["outcome"], summary["status"])
                authority = await harness.session_view(first.id, bounds={"max_bytes": 0})
                self.assertEqual(authority["events"], [])
                after = 0
                finished = False
                for _ in range(32):
                    events = await first.events_page(
                        view["durable_cursor"], after=after, bounds={"max_events": 1}
                    )
                    self.assertLessEqual(len(events["events"]), 1)
                    finished |= any(event["type"] == "run_finished" for event in events["events"])
                    if events["complete"]:
                        break
                    self.assertGreater(events["cursor"], after)
                    after = events["cursor"]
                self.assertTrue(finished)

    async def test_scheduling_status_requires_capability_before_sending_method(self) -> None:
        request = AsyncHarness._request
        status_calls = 0

        async def without_scheduling(
            harness: AsyncHarness, method: str, params: object, *, during_close: bool = False
        ) -> object:
            nonlocal status_calls
            if method == "scheduling.status":
                status_calls += 1
            result = await request(harness, method, params, during_close=during_close)
            if method == "hello":
                value = cast("dict[str, Any]", result)
                return {
                    **value,
                    "capabilities": [
                        item for item in value["capabilities"] if item != "scheduling"
                    ],
                }
            return result

        with (
            tempfile.TemporaryDirectory() as state,
            patch.object(AsyncHarness, "_request", without_scheduling),
        ):
            async with AsyncHarness(worker_path=WORKER, config=provider_fixture(state)) as harness:
                with self.assertRaisesRegex(HarnessError, "supports scheduling"):
                    await harness.scheduling_status()
                self.assertEqual(status_calls, 0)
                self.assertEqual((await harness.status())["active_agents"], 0)

    async def test_invalid_capacity_fails_before_package_loading(self) -> None:
        with self.assertRaisesRegex(HarnessError, "model ceiling"):
            await AsyncHarness.create(
                worker_path=WORKER,
                config={
                    "provider": {
                        "kind": "responses",
                        "api_key": "fixture-key-never-sent",
                        "model": "fixture-model",
                    },
                    "capacity": {"mode": "fixed", "max_model_requests": 0},
                    "packages": [{"root": "/unused-capacity-fixture", "grants": {}}],
                },
            )

    async def test_live_hints_evict_old_cursors_and_release_waiters(self) -> None:
        harness = AsyncHarness()
        stdout = asyncio.StreamReader()
        lines = [
            json.dumps(
                {
                    "method": "event",
                    "params": {
                        "type": "text_delta",
                        "session_id": f"historical-{index}",
                        "run_id": "fixture-run",
                        "sequence": index + 1,
                        "at_ms": 0,
                        "text": "hint",
                    },
                }
            )
            for index in range(1001)
        ]
        lines.append(json.dumps({"id": "after-hints", "result": True}))
        stdout.feed_data(("\n".join(lines) + "\n").encode())
        stdout.feed_eof()
        process = Mock(spec=asyncio.subprocess.Process)
        process.stdout = stdout
        process.returncode = 1
        harness._process = process
        response: asyncio.Future[object] = asyncio.get_running_loop().create_future()
        harness._pending["after-hints"] = response
        try:
            await harness._read()
            self.assertTrue(response.result())
            self.assertNotIn("historical-0", harness._cursors)
            self.assertEqual(harness._cursors["historical-1000"], 1001)
            self.assertEqual(len(harness._cursors), 1000)
        finally:
            harness._process = None
            await harness.close()

        with tempfile.TemporaryDirectory() as state:
            async with AsyncHarness(worker_path=WORKER, config=provider_fixture(state)) as harness:
                waiting = asyncio.create_task(harness._changed("active"))
                await asyncio.sleep(0)
                harness._changes["active"].set()
                await asyncio.wait_for(waiting, 1)
                self.assertEqual(harness._changes, {})
                cancelled = asyncio.create_task(harness._changed("cancelled"))
                await asyncio.sleep(0)
                cancelled.cancel()
                with self.assertRaises(asyncio.CancelledError):
                    await cancelled
                self.assertEqual(harness._changes, {})

    async def test_package_configuration_requires_worker_capability(self) -> None:
        request = AsyncHarness._request

        async def legacy_hello(
            harness: AsyncHarness, method: str, params: object, *, during_close: bool = False
        ) -> object:
            result = await request(harness, method, params, during_close=during_close)
            if method == "hello":
                return {**cast("dict[str, object]", result), "capabilities": []}
            return result

        with (
            patch.object(AsyncHarness, "_request", legacy_hello),
            self.assertRaisesRegex(HarnessError, "update ejm-harness-worker"),
        ):
            await AsyncHarness.create(
                worker_path=WORKER,
                config={"packages": [{"root": "/unused-package", "grants": {}}]},
            )

    async def test_python_packages_use_shared_runtime_and_explicit_grants(self) -> None:
        package = "@fixture/sdk-python"
        with tempfile.TemporaryDirectory(prefix="ejm-harness-package-sdk-py-") as folder:
            root = Path(folder)
            async with AsyncHarness(worker_path=WORKER, config=python_package(root)) as harness:
                session = await harness.create_session(root)
                self.assertEqual((await harness.list_packages())[0]["manifest"]["name"], package)
                run = await session.start("request")
                self.assertEqual((await run.wait())["text"], "custom:request|python")
                first = cast(
                    "dict[str, Any]", await session.invoke_package_command(package, "count")
                )
                second = cast(
                    "dict[str, Any]", await session.invoke_package_command(package, "count")
                )
                self.assertEqual((first["calls"], second["calls"]), (1, 2))
                self.assertEqual(first["pid"], second["pid"])
                self.assertEqual(first["session_id"], session.id)
                self.assertEqual(
                    await session.render_package_widget(package, "status", {"label": "ready"}),
                    {"kind": "text", "content": "ready"},
                )
                with self.assertRaisesRegex(HarnessError, "not granted"):
                    await session.invoke_package_command(package, "ungranted")
                self.assertEqual((await harness.status())["active_agents"], 0)
                await harness.reload_package(package)
                refreshed = cast(
                    "dict[str, Any]", await session.invoke_package_command(package, "count")
                )
                self.assertEqual(refreshed["calls"], 1)
                self.assertNotEqual(refreshed["pid"], first["pid"])
                self.assertEqual(
                    (await (await session.start("again")).wait())["text"], "custom:again|python"
                )

    @unittest.skipIf(os.name == "nt", "Windows shared native execution remains gated")
    async def test_coordinated_capacity_composes_package_execution_and_shutdown(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ejm-harness-capacity-sdk-py-") as folder:
            root = Path(folder)
            config = python_package(root)
            config["capacity"] = {
                "mode": "auto",
                "max_sessions": 2,
                "max_model_requests": 1,
                "max_jobs": 2,
                "max_memory_bytes": 256 * 1024 * 1024,
            }
            async with AsyncHarness(
                worker_path=WORKER, config=config, request_timeout=10
            ) as harness:
                async with asyncio.timeout(30):
                    capacity = await harness.capacity_status()
                    assert capacity is not None
                    self.assertEqual(capacity["mode"], "auto")
                    self.assertTrue(capacity["coordinated"])
                    self.assertEqual(capacity["hard_model_requests"], 1)
                    self.assertEqual(capacity["hard_resources"]["max_jobs"], 2)
                    self.assertEqual(capacity["hard_resources"]["memory_bytes"], 256 * 1024 * 1024)
                    async with asyncio.timeout(5):
                        while True:
                            resident = await harness.governor()
                            if resident["running"] == 0:
                                break
                            await asyncio.sleep(0.025)
                    self.assertEqual(resident["running"], 0)
                    self.assertGreaterEqual(resident["reserved_memory_bytes"], 64 * 1024 * 1024)
                    self.assertEqual(resident["reserved_processes"], 1)

                    session = await harness.create_session(root)
                    run = await session.start("coordinated package")
                    self.assertEqual(
                        (await run.wait())["text"], "custom:coordinated package|python"
                    )
                    first = cast(
                        "dict[str, Any]",
                        await session.invoke_package_command("@fixture/sdk-python", "count"),
                    )
                    second = cast(
                        "dict[str, Any]",
                        await session.invoke_package_command("@fixture/sdk-python", "count"),
                    )
                    self.assertEqual((first["calls"], second["calls"]), (1, 2))
                    self.assertEqual(first["pid"], second["pid"])
                    self.assertEqual(first["session_id"], session.id)
                    async with asyncio.timeout(5):
                        while True:
                            current = await harness.capacity_status()
                            assert current is not None
                            governor = await harness.governor()
                            # Helper commitments are host aggregates, not private client counts.
                            helpers = current.get("native_helpers", {})
                            if (
                                helpers.get("committed", 0) > 0
                                and helpers.get("memory_bytes", 0) > 0
                                and (await harness.status())["active_agents"] == 0
                                and governor["running"] == 0
                            ):
                                self.assertEqual(
                                    governor["reserved_memory_bytes"],
                                    resident["reserved_memory_bytes"],
                                )
                                self.assertEqual(
                                    governor["reserved_processes"], resident["reserved_processes"]
                                )
                                break
                            await asyncio.sleep(0.025)
            await harness.close()
            with self.assertRaises(HarnessError) as caught:
                await harness.capacity_status()
            self.assertIn(caught.exception.code, {"closed", "disconnected"})

    async def test_native_api_key_providers_configure_without_model_requests(self) -> None:
        providers: list[ProviderConfig] = [
            {
                "kind": "responses",
                "api_key": "fixture-key-never-sent",
                "model": "fixture-model",
                "reasoning_effort": "high",
            },
            {"kind": "anthropic", "api_key": "fixture-key-never-sent", "model": "fixture-model"},
        ]
        for provider in providers:
            with (
                self.subTest(provider=provider["kind"]),
                tempfile.TemporaryDirectory(prefix="ejm-harness-sdk-py-") as state,
            ):
                async with AsyncHarness(
                    worker_path=WORKER, config={"state_dir": state, "provider": provider}
                ) as harness:
                    await harness.create_session(state)
                    self.assertEqual((await harness.status())["active_agents"], 0)

    async def test_native_provider_configuration_errors_keep_credentials_private(self) -> None:
        providers: list[ProviderConfig] = [
            {
                "kind": "responses",
                "api_key": "private-fixture-key",
                "model": "fixture-model",
                "reasoning_effort": "unknown",
            },
            {"kind": "anthropic", "api_key": "private-fixture-key\n", "model": "fixture-model"},
        ]
        for provider in providers:
            with (
                self.subTest(provider=provider["kind"]),
                tempfile.TemporaryDirectory(prefix="ejm-harness-sdk-py-") as state,
            ):
                with self.assertRaises(HarnessError) as caught:
                    await AsyncHarness.create(
                        worker_path=WORKER, config={"state_dir": state, "provider": provider}
                    )
                self.assertEqual(caught.exception.code, "configuration")
                self.assertNotIn("private-fixture-key", str(caught.exception))

    async def test_buffered_invocation_cannot_run_after_transport_failure(self) -> None:
        calls = []

        async def execute(_arguments: object, _context: ToolContext) -> str:
            calls.append("unexpected")
            return "unexpected"

        harness = AsyncHarness(tools=[Tool(DEFINITION, execute)])
        stdout = asyncio.StreamReader()
        stdout.feed_data(
            json.dumps(
                {
                    "id": "late",
                    "method": "tool.invoke",
                    "params": {
                        "session_id": "session",
                        "run_id": "run",
                        "call_id": "call",
                        "name": DEFINITION["name"],
                        "arguments": {"value": "hello"},
                        "deadline_ms": 2**50,
                    },
                }
            ).encode()
            + b"\n"
        )
        stdout.feed_eof()
        process = Mock(spec=asyncio.subprocess.Process)
        process.stdout = stdout
        process.returncode = 1
        harness._process = process
        harness._failure = HarnessError("closed", "transport already failed")
        await harness._read()
        if harness._tasks:
            await asyncio.gather(*harness._tasks, return_exceptions=True)
        self.assertEqual(calls, [])
        harness._pool.shutdown(wait=False)

    async def test_one_hundred_concurrent_starts_and_waits(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ejm-harness-sdk-py-") as state:
            async with AsyncHarness(worker_path=WORKER, config=provider_fixture(state)) as harness:
                sessions = await asyncio.gather(
                    *(harness.create_session(state) for _ in range(100))
                )
                runs = await asyncio.gather(*(session.start("hello") for session in sessions))
                results = await asyncio.gather(*(run.wait() for run in runs))
                self.assertEqual(len(results), 100)
                self.assertTrue(all(result["status"] == "completed" for result in results))
                self.assertEqual((await harness.status())["active_agents"], 0)

    async def test_request_overflow_keeps_control_capacity(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ejm-harness-sdk-py-") as state:
            async with AsyncHarness(worker_path=WORKER, config=provider_fixture(state)) as harness:
                run = await (await harness.create_session(state)).start("hello")
                requests = [asyncio.create_task(harness.status()) for _ in range(400)]
                await asyncio.sleep(0)
                await run.cancel()
                results = await asyncio.gather(*requests, return_exceptions=True)
                errors = [result for result in results if isinstance(result, HarnessError)]
                self.assertTrue(errors)
                self.assertTrue(all(error.code == "busy" for error in errors))
                self.assertIsNone(harness._failure)

    async def test_late_wait_and_shared_sessions(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ejm-harness-sdk-py-") as state:
            async with AsyncHarness(worker_path=WORKER, config=provider_fixture(state)) as harness:
                first = await harness.create_session(state)
                second = await harness.create_session(state)
                run = await first.start("hello from Python")
                await asyncio.sleep(0.05)
                events = [event async for event in run]
                self.assertTrue(any(event["type"] == "text_delta" for event in events))
                self.assertEqual((await run.wait())["text"], "Fixture: hello from Python")
                self.assertEqual(
                    (await (await second.start("second")).wait())["status"], "completed"
                )
                self.assertEqual(len(await harness.list_sessions()), 2)
                self.assertEqual((await harness.status())["active_agents"], 0)

    async def test_async_callback_progress(self) -> None:
        calls: list[str] = []

        async def execute(arguments: dict[str, Any], context: ToolContext) -> ToolOutput:
            calls.append(arguments["value"])
            await context.progress("looked up value")
            return tool_output("found", data={"found": True})

        with tempfile.TemporaryDirectory(prefix="ejm-harness-sdk-py-") as state:
            async with AsyncHarness(
                worker_path=WORKER, config=callback_config(state), tools=[Tool(DEFINITION, execute)]
            ) as harness:
                run = await (await harness.create_session(state)).start("lookup")
                events = [event async for event in run]
                self.assertEqual((await run.wait())["text"], "finished")
                self.assertEqual(calls, ["hello"])
                self.assertTrue(any(event["type"] == "progress" for event in events))
                self.assertTrue(
                    any(
                        event["type"] == "tool_finished" and event["output"]["data"]["found"]
                        for event in events
                    )
                )

    async def test_cancel_reaches_callback(self) -> None:
        invoked = asyncio.Event()
        cancelled = asyncio.Event()

        async def execute(_arguments: object, context: ToolContext) -> str:
            invoked.set()
            try:
                await context.wait_cancelled()
            finally:
                cancelled.set()
            raise asyncio.CancelledError

        with tempfile.TemporaryDirectory(prefix="ejm-harness-sdk-py-") as state:
            async with AsyncHarness(
                worker_path=WORKER, config=callback_config(state), tools=[Tool(DEFINITION, execute)]
            ) as harness:
                self.assertEqual(await harness.scheduling_status(), IDLE_SCHEDULING)
                run = await (await harness.create_session(state)).start("wait")
                await asyncio.wait_for(invoked.wait(), timeout=5)
                self.assertEqual(
                    await harness.scheduling_status(),
                    {**IDLE_SCHEDULING, "managed_runs": 1, "tool_running": 1},
                )
                self.assertEqual(
                    set(await harness.status()),
                    {"active_agents", "model_requests", "tools", "waiting_model_requests"},
                )
                await run.cancel()
                self.assertEqual((await run.wait())["status"], "cancelled")
                await asyncio.wait_for(cancelled.wait(), timeout=1)
                for _ in range(100):
                    if (await harness.governor())["running"] == 0:
                        break
                    await asyncio.sleep(0.01)
                self.assertEqual((await harness.governor())["running"], 0)
                self.assertEqual(await harness.scheduling_status(), IDLE_SCHEDULING)

    async def test_cancelled_sync_write_keeps_workspace_ownership(self) -> None:
        started = threading.Event()
        second_started = threading.Event()
        release = threading.Event()
        calls = 0

        def execute(_arguments: object, _context: ToolContext) -> str:
            nonlocal calls
            calls += 1
            if calls == 1:
                started.set()
                release.wait(timeout=3)
            else:
                second_started.set()
            return "finished"

        with tempfile.TemporaryDirectory(prefix="ejm-harness-sdk-py-") as state:
            config = callback_config(state, calls=2)
            definition: ToolDefinition = {**DEFINITION, "effect": "write"}
            async with AsyncHarness(
                worker_path=WORKER, config=config, tools=[Tool(definition, execute)]
            ) as harness:
                first, second = await asyncio.gather(
                    harness.create_session(state), harness.create_session(state)
                )
                run = await first.start("first write")
                self.assertTrue(await asyncio.to_thread(started.wait, 2))
                try:
                    await run.cancel()
                    self.assertEqual((await run.wait())["status"], "cancelled")
                    self.assertEqual((await harness.governor())["running"], 1)
                    next_run = await second.start("second write")
                    await asyncio.sleep(0.1)
                    self.assertFalse(second_started.is_set())
                    release.set()
                    self.assertEqual((await next_run.wait())["status"], "completed")
                    self.assertTrue(second_started.is_set())
                finally:
                    release.set()

    async def test_cancelled_sync_callbacks_keep_four_job_limit(self) -> None:
        started = threading.Event()
        release = threading.Event()
        fifth_started = threading.Event()
        lock = threading.Lock()
        calls = 0
        active = 0
        peak = 0

        def execute(_arguments: object, _context: ToolContext) -> str:
            nonlocal calls, active, peak
            with lock:
                calls += 1
                number = calls
                active += 1
                peak = max(peak, active)
                if calls == 4:
                    started.set()
            try:
                if number <= 4:
                    release.wait(timeout=3)
                else:
                    fifth_started.set()
                return "finished"
            finally:
                with lock:
                    active -= 1

        with tempfile.TemporaryDirectory(prefix="ejm-harness-sdk-py-") as state:
            config = callback_config(state, calls=5)
            async with AsyncHarness(
                worker_path=WORKER, config=config, tools=[Tool(DEFINITION, execute)]
            ) as harness:
                sessions = await asyncio.gather(*(harness.create_session(state) for _ in range(5)))
                runs = await asyncio.gather(*(session.start("hold") for session in sessions[:4]))
                self.assertTrue(await asyncio.to_thread(started.wait, 2))
                try:
                    await asyncio.gather(*(run.cancel() for run in runs))
                    await asyncio.gather(*(run.wait() for run in runs))
                    self.assertEqual((await harness.governor())["running"], 4)
                    fifth = await sessions[4].start("fifth")
                    await asyncio.sleep(0.1)
                    self.assertFalse(fifth_started.is_set())
                    release.set()
                    self.assertEqual((await fifth.wait())["status"], "completed")
                    self.assertEqual(peak, 4)
                finally:
                    release.set()

    async def test_full_journal_fails_wait_and_preserves_recoverable_history(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ejm-harness-sdk-py-") as state:
            config = provider_fixture(state)
            config["max_store_bytes"] = 1024 * 1024
            async with AsyncHarness(worker_path=WORKER, config=config) as harness:
                session = await harness.create_session(state)
                with self.assertRaises(HarnessError):
                    for _ in range(30):
                        run = await session.start("x" * 16_000)
                        await asyncio.wait_for(run.wait(), timeout=2)
                for _ in range(100):
                    if harness._failure is not None:
                        break
                    await asyncio.sleep(0.01)
                self.assertIsNotNone(harness._failure)
            try:
                async with AsyncHarness(worker_path=WORKER, config=config):
                    pass
            except HarnessError:
                pass
            config["max_store_bytes"] = 8 * 1024 * 1024
            async with AsyncHarness(worker_path=WORKER, config=config) as recovered:
                sessions = await recovered.list_sessions()
                self.assertEqual(len(sessions), 1)
                self.assertNotEqual(sessions[0].info["status"], "running")
                history = []
                after = 0
                while events := await sessions[0].events(after):
                    history.extend(events)
                    after = events[-1]["sequence"]
                self.assertEqual(history[-1]["type"], "run_finished")

    async def test_uncooperative_sync_callback_closes_transport_after_grace(self) -> None:
        started = threading.Event()
        release = threading.Event()

        def execute(_arguments: object, _context: ToolContext) -> str:
            started.set()
            release.wait(timeout=10)
            return "late"

        with tempfile.TemporaryDirectory(prefix="ejm-harness-sdk-py-") as state:
            async with AsyncHarness(
                worker_path=WORKER, config=callback_config(state), tools=[Tool(DEFINITION, execute)]
            ) as harness:
                run = await (await harness.create_session(state)).start("hold")
                self.assertTrue(await asyncio.to_thread(started.wait, 2))
                try:
                    await run.cancel()
                    self.assertEqual((await run.wait())["status"], "cancelled")
                    for _ in range(600):
                        if harness._failure is not None:
                            break
                        await asyncio.sleep(0.01)
                    with self.assertRaises(HarnessError):
                        await harness.status()
                finally:
                    release.set()


class SyncSdkTests(unittest.TestCase):
    def test_sync_capacity_and_view_facades(self) -> None:
        with (
            tempfile.TemporaryDirectory() as state,
            Harness(worker_path=WORKER, config=provider_fixture(state)) as harness,
        ):
            self.assertIsNone(harness.capacity_status())
            self.assertEqual(harness.scheduling_status(), IDLE_SCHEDULING)
            session = harness.create_session(title="sync view")
            page = harness.sessions_page({"limit": 1})
            self.assertEqual(page["sessions"][0]["id"], session.id)
            self.assertEqual(harness.resolve_session(session.id).id, session.id)
            run = session.start("sync bounded history")
            run.wait()
            view = session.view(bounds={"max_events": 0})
            self.assertEqual(view["events"], [])
            transition = view["last_transition"]
            assert transition is not None
            self.assertEqual(transition["run_id"], run.id)
            self.assertTrue(harness.session_view(session.id)["events"])
            self.assertTrue(session.events_page(view["durable_cursor"])["complete"])
            self.assertTrue(harness.events_page(session.id, view["durable_cursor"])["complete"])

    def test_sync_package_command_and_widget_facades(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ejm-harness-package-sdk-sync-py-") as folder:
            root = Path(folder)
            with Harness(worker_path=WORKER, config=python_package(root)) as harness:
                session = harness.create_session(root)
                package = harness.list_packages()[0]["manifest"]["name"]
                command = cast("dict[str, Any]", session.invoke_package_command(package, "count"))
                self.assertEqual(command["calls"], 1)
                self.assertEqual(
                    session.render_package_widget(package, "status"),
                    {"kind": "text", "content": "status"},
                )
                harness.reload_package(package)

    def test_sync_callback_and_iteration(self) -> None:
        def execute(arguments: dict[str, Any], context: ToolContext) -> str:
            context.report_progress("sync progress")
            return str(arguments["value"])

        with (
            tempfile.TemporaryDirectory(prefix="ejm-harness-sdk-py-") as state,
            Harness(
                worker_path=WORKER, config=callback_config(state), tools=[Tool(DEFINITION, execute)]
            ) as harness,
        ):
            session = harness.create_session(state)
            run = session.start("lookup")
            self.assertTrue(any(event["type"] == "progress" for event in run))
            self.assertEqual(run.wait()["text"], "finished")
            self.assertEqual(harness.list_sessions()[0].id, session.id)


if __name__ == "__main__":
    unittest.main()
