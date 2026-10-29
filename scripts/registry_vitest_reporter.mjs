import { closeSync, openSync, writeFileSync } from "node:fs";

export default class EvidenceReporter {
  onInit() {
    this.output = openSync(process.env.EJM_REGISTRY_VITEST_EVIDENCE, "w");
    this.write({ kind: "start" });
  }

  write(record) {
    writeFileSync(this.output, `${JSON.stringify(record)}\n`);
  }

  onTestCaseResult(test) {
    const result = test.result();
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

  onTestRunEnd(modules, errors, reason) {
    const incomplete = errors.length > 0 || !["passed", "failed"].includes(reason) || modules.some(
      (module) => module.errors().length || [...module.children.allSuites()].some((suite) => suite.errors().length),
    );
    this.write({ kind: "end", incomplete });
    closeSync(this.output);
  }
}
