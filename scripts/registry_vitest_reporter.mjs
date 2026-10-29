import { closeSync, openSync, writeFileSync } from "node:fs";

export default class EvidenceReporter {
  onInit() {
    this.output = openSync(process.env.EJM_REGISTRY_VITEST_EVIDENCE, "w");
    this.firstFailure = false;
    this.write({ kind: "start" });
  }

  write(record) {
    writeFileSync(this.output, `${JSON.stringify(record)}\n`);
  }

  onTestCaseResult(test) {
    const result = test.result();
    this.firstFailure ||= result.state === "failed";
    const error = result.errors?.[0];
    const message = typeof error?.stack === "string" ? error.stack : error?.message;
    this.write({
      kind: "case",
      file: test.module.moduleId.slice(0, 1025),
      name: test.fullName.slice(0, 1025),
      status: result.state,
      ...(typeof message === "string" ? {
        message: message.length > 8192
          ? `${message.slice(0, 4096)}\n...\n${message.slice(-4096)}`
          : message,
        truncated: message.length > 8192,
      } : {}),
    });
  }

  errors(errors, test) {
    if (this.firstFailure || !errors.length) return;
    this.firstFailure = true;
    const error = errors[0];
    const message = typeof error?.stack === "string" ? error.stack : error?.message;
    this.write({
      kind: "diagnostic",
      test: test.slice(0, 1024),
      message: typeof message === "string" && message.length > 8192
        ? `${message.slice(0, 4096)}\n...\n${message.slice(-4096)}`
        : typeof message === "string" ? message : "",
      truncated: typeof message === "string" && message.length > 8192,
    });
  }

  onTestSuiteResult(suite) {
    this.errors(suite.errors(), suite.name);
  }

  onTestModuleEnd(module) {
    this.errors(module.errors(), module.moduleId);
  }

  onHookEnd(hook) {
    if (hook.name === "beforeAll" || hook.name === "afterAll") {
      this.errors(hook.entity.errors(), hook.entity.name ?? hook.entity.moduleId);
    }
  }

  onTestRunEnd(modules, errors, reason) {
    this.errors(errors, "Unhandled Vitest error");
    for (const module of modules) {
      this.errors(module.errors(), module.moduleId);
      for (const suite of module.children.allSuites()) this.errors(suite.errors(), suite.name);
    }
    const incomplete = errors.length > 0 || !["passed", "failed"].includes(reason) || modules.some(
      (module) => module.errors().length || [...module.children.allSuites()].some((suite) => suite.errors().length),
    );
    this.write({ kind: "end", incomplete });
    closeSync(this.output);
  }
}
