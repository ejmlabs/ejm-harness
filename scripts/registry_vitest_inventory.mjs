// Collect runtime identities, including skipped and parameterized tests.
import { createVitest } from "vitest/node";

const config = process.argv[2];
if (config && config !== "vitest.auth.config.ts") throw new Error("Unsupported test configuration");
const runner = await createVitest("test", {
  watch: false,
  reporters: [],
  ...(config ? { config } : {}),
});
try {
  const specifications = await runner.globTestSpecifications();
  const { testModules, unhandledErrors } = await runner.collectTests(specifications);
  if (unhandledErrors.length || testModules.length !== specifications.length) {
    throw new Error("Vitest collection failed");
  }
  const cases = [];
  for (const module of testModules) {
    if (
      module.errors().length ||
      [...module.children.allSuites()].some((suite) => suite.errors().length)
    ) {
      throw new Error("Vitest module collection failed");
    }
    for (const test of module.children.allTests()) {
      cases.push({ file: module.moduleId, name: test.fullName });
    }
  }
  if (!cases.length) throw new Error("Vitest collection was empty");
  process.stdout.write(JSON.stringify(cases));
} finally {
  await runner.close();
}
