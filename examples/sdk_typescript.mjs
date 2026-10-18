import { Harness } from "../sdk/typescript/dist/index.js";

const harness = await Harness.create({ provider: { kind: "echo" } });
try {
  const session = await harness.createSession({ title: "SDK example" });
  const run = await session.start("Hello from ejm-harness");
  for await (const event of run) {
    if (event.type === "text_delta") process.stdout.write(event.text);
  }
  const result = await run.wait();
  console.log(`\n${result.status}`);
} finally {
  await harness.close();
}
