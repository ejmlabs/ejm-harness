import { HarnessError } from "./errors.js";
import type { Harness } from "./harness.js";
import type { Event, RunStarted, RunSummary, Session as SessionInfo } from "./protocol.js";

export class Session {
  readonly info: SessionInfo;
  private harness: Harness;
  constructor(harness: Harness, info: SessionInfo) {
    this.harness = harness;
    this.info = info;
  }
  get id(): string {
    return this.info.id;
  }

  async start(prompt: string, options: { signal?: AbortSignal } = {}): Promise<Run> {
    options.signal?.throwIfAborted();
    const after = this.harness.cursor(this.id);
    const started = await this.harness.request<RunStarted>("run.start", {
      session_id: this.id,
      prompt,
    });
    return new Run(this.harness, started, after, options.signal);
  }
  async events(after = 0, limit = 64): Promise<Event[]> {
    return this.harness.events(this.id, after, limit);
  }
  async invokePackageCommand(
    packageName: string,
    name: string,
    args: unknown = {},
  ): Promise<unknown> {
    return this.harness.invokePackageCommand(packageName, name, this.id, args);
  }
  async renderPackageWidget(
    packageName: string,
    name: string,
    args: unknown = {},
  ): Promise<unknown> {
    return this.harness.renderPackageWidget(packageName, name, this.id, args);
  }
}

export class Run implements AsyncIterable<Event> {
  readonly id: string;
  readonly sessionId: string;
  private harness: Harness;
  private after: number;
  private summary?: RunSummary;
  private result?: Promise<RunSummary>;

  constructor(harness: Harness, started: RunStarted, after = 0, signal?: AbortSignal) {
    this.harness = harness;
    this.id = started.run_id;
    this.sessionId = started.session_id;
    this.after = after;
    if (signal) this.cancelOnAbort(signal);
  }

  async *[Symbol.asyncIterator](): AsyncIterator<Event> {
    let after = this.after;
    for (;;) {
      const events = await this.harness.events(this.sessionId, after);
      if (!events.length) {
        await this.harness.changed(this.sessionId);
        continue;
      }
      for (const event of events) {
        after = event.sequence;
        if (event.run_id !== this.id) continue;
        if (event.type === "run_finished") this.summary = event.summary;
        yield event;
        if (event.type === "run_finished") return;
      }
    }
  }

  async wait(): Promise<RunSummary> {
    if (this.summary) return this.summary;
    this.result ??= (async (): Promise<RunSummary> => {
      for await (const event of this) if (event.type === "run_finished") return event.summary;
      throw new HarnessError("protocol", "run ended without a summary");
    })();
    return this.result;
  }
  async cancel(): Promise<void> {
    if (!this.summary) await this.harness.request("run.cancel", this.target());
  }
  async steer(text: string): Promise<void> {
    await this.harness.request("run.steer", { ...this.target(), text });
  }
  async queue(text: string): Promise<void> {
    await this.harness.request("run.queue", { ...this.target(), text });
  }

  private target(): { session_id: string; run_id: string } {
    return { session_id: this.sessionId, run_id: this.id };
  }

  /** Cancel the run when the signal aborts, until the run finishes. */
  private cancelOnAbort(signal: AbortSignal): void {
    const abort = (): void => {
      void this.cancel().catch(() => {
        /* the reader owns disconnect handling */
      });
    };
    signal.addEventListener("abort", abort, { once: true });
    if (signal.aborted) abort();
    void this.wait()
      .catch(() => {
        /* wait() still exposes the failure to its caller */
      })
      .finally(() => {
        signal.removeEventListener("abort", abort);
      });
  }
}
