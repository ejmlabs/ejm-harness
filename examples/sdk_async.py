"""Run an agent through the Python SDK without a hosted model call."""

import asyncio

from ejm_harness import AsyncHarness


async def main() -> None:
    async with AsyncHarness(config={"provider": {"kind": "echo"}}) as harness:
        session = await harness.create_session(title="SDK example")
        run = await session.start("Hello from ejm-harness")
        async for event in run:
            if event["type"] == "text_delta":
                print(event["text"], end="", flush=True)
        result = await run.wait()
        print(f"\n{result['status']}")


if __name__ == "__main__":
    asyncio.run(main())
