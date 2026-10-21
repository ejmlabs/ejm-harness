import { Band } from "@ejm/ui/layout/Band";
import { Row, Rows } from "@ejm/ui/layout/Rows";
import { SectionHead } from "@ejm/ui/layout/SectionHead";
import { Stat } from "@ejm/ui/layout/Stat";
import { ExternalLink } from "@ejm/ui/shell/ExternalLink";
import { PROJECTS } from "@ejm/site/projects";
import { Button } from "@ejm/ui/components/ui/button";
import {
    CommandLines,
    ContentFlow,
    MetricGrid,
    Note,
    ReadingColumn,
    SectionBody,
} from "@ejm/ui/layout/SiteContent";

const repo = PROJECTS.harness.repo;
const cli = PROJECTS.cli;
const ide = PROJECTS.ide;

// Copy stays in the built HTML. The recorded player is composed outside Static in Home.
export function HeroIntro() {
    return (
        <p>
            ejm-harness is open source under the MIT license. Rust hosts embed its libraries;
            TypeScript and Python use a persistent Rust worker. Sessions share provider access,
            tools, resource limits and an event journal.
        </p>
    );
}

export function HeroActions() {
    return (
        <>
            <Button asChild arrow>
                <a href="/docs/user-guide/#requirements">Build and run</a>
            </Button>
            <Button asChild variant="link">
                <a href="#contribute">Contribute</a>
            </Button>
            <Button asChild variant="link">
                <ExternalLink href={repo}>Source</ExternalLink>
            </Button>
        </>
    );
}

export function How() {
    return (
        <>
            <Band id="how" tone="mist" aria-labelledby="how-title">
                <SectionHead
                    kicker="01 / How it works"
                    title="Sessions, tools and recovery."
                    id="how-title"
                >
                    <p>
                        Hosts configure providers, tools, limits and storage. The engine schedules
                        runs and records their events in SQLite.
                    </p>
                </SectionHead>
                <SectionBody>
                    <Rows aria-label="What the engine does">
                        <Row
                            title="Session state"
                            description="A session awaiting a model holds no process. Sessions are light state that share one provider, tool registry, store and governor."
                        />
                        <Row
                            title="Resource reservations"
                            description="Commands reserve CPU, memory, processes and output before they start. Model requests and local jobs have separate limits, and a model cannot raise them."
                        />
                        <Row
                            title="Queue scheduling"
                            description="Requests rotate between sessions, first in first out within each. A dropped waiter leaves at once, and a lease frees every reservation when it ends."
                        />
                        <Row
                            title="Event journal and recovery"
                            description="A bounded SQLite journal records every event. After a crash, the engine recovers the run and never repeats an effect whose outcome is uncertain."
                        />
                        <Row
                            title="Native file tools"
                            description="Read, list, search, write and patch without spawning a shell. Shell access is optional and keeps normal Bash semantics when granted."
                        />
                        <Row
                            title="Reported restrictions"
                            description="Each restriction is reported as enforced, monitored or unsupported. Native execution runs as you and is not a sandbox."
                        />
                    </Rows>
                </SectionBody>
            </Band>
        </>
    );
}

export function Rest() {
    return (
        <>
            <Band id="limits" tone="paper" aria-labelledby="limits-title">
                <SectionHead
                    kicker="02 / Limits"
                    title="Default resource limits."
                    id="limits-title"
                >
                    <p>
                        The engine sets separate limits for sessions, model requests and local jobs.
                    </p>
                </SectionHead>
                <SectionBody>
                    <MetricGrid>
                        <Stat figure="100" label="managed sessions" />
                        <Stat figure="8" label="concurrent model requests" />
                        <Stat figure="4" label="local jobs" />
                        <Stat figure="1" label="heavy job" />
                    </MetricGrid>
                </SectionBody>
                <Note>
                    An offline benchmark on 18 October 2026 ran 100 sessions with three Bash checks
                    each on a Mac16,8 with 12 logical CPUs and 24 GiB of RAM. Sampled engine memory
                    was 25.72 MiB and sampled peak local jobs was 4. Remote model latency is
                    excluded; results depend on hardware and workload.{" "}
                    <ExternalLink href={`${repo}/blob/main/docs/benchmarks.md`}>
                        Benchmark methods and results
                    </ExternalLink>
                </Note>
            </Band>

            <Band id="languages" tone="mist" aria-labelledby="languages-title">
                <SectionHead
                    kicker="03 / Languages"
                    title="Rust and SDK interfaces."
                    id="languages-title"
                >
                    <p>
                        Rust hosts embed the crates. TypeScript and Python share one persistent Rust
                        worker, and neither is needed by a Rust host.
                    </p>
                </SectionHead>
                <SectionBody>
                    <Rows aria-label="Ways to use the engine">
                        <Row
                            title="Rust"
                            description="Depend on crates/ejm-harness from your own Cargo project. Add ejm-harness-coding when the host needs the standard coding tools."
                            meta="Provider, Tool, ContextPolicy, RunStrategy"
                            href="/docs/developer-guide/#rust"
                        />
                        <Row
                            title="TypeScript"
                            description="A typed async API with tools as ordinary functions, schema validation, progress and cancellation."
                            meta="sdk/typescript"
                            href="/docs/user-guide/#first-run"
                        />
                        <Row
                            title="Python"
                            description="Sync and async APIs, with tools as plain functions and credentials passed over stdin, never through arguments."
                            meta="sdk/python"
                            href="/docs/user-guide/#first-run"
                        />
                    </Rows>
                </SectionBody>
            </Band>

            <Band id="source" tone="sage" aria-labelledby="source-title">
                <SectionHead
                    kicker="04 / Source"
                    title="Build from the repository."
                    id="source-title"
                >
                    <p>
                        The source is public under MIT. Build with the pinned Rust toolchain.
                        Published packages, prebuilt workers and a live extension registry are not
                        available yet.
                    </p>
                </SectionHead>
                <SectionBody>
                    <ReadingColumn>
                        <ContentFlow>
                            <CommandLines
                                commands={[
                                    "cargo build --release --workspace --locked",
                                    "cargo test --workspace --locked",
                                ]}
                            />
                            <p id="contribute">
                                Contributions to code and docs are welcome. Read the contribution
                                guide, report a reproducible issue, or discuss a proposed change in
                                the repository.
                            </p>
                        </ContentFlow>
                    </ReadingColumn>
                </SectionBody>
                <SectionBody>
                    <Rows aria-label="Source, contributions and related projects">
                        <Row
                            title="Contribution guide"
                            description="Repository policy, local setup and required checks."
                            href={`${repo}/blob/main/CONTRIBUTING.md`}
                            external
                        />
                        <Row
                            title="Issue tracker"
                            description="Report a problem with steps to reproduce it, or discuss a proposed change."
                            href={`${repo}/issues`}
                            external
                        />
                        <Row
                            title={cli.name}
                            description="The terminal coding agent built on ejm-harness."
                            meta={cli.host}
                            href={cli.url}
                            external
                        />
                        <Row
                            title={ide.name}
                            description="The open source desktop code editor and agent interface built on ejm-harness."
                            meta={ide.host}
                            href={ide.url}
                            external
                        />
                        <Row
                            title="Source on GitHub"
                            description="Source code, tests and development history."
                            meta="ejmlabs/ejm-harness"
                            href={repo}
                            external
                        />
                    </Rows>
                </SectionBody>
            </Band>
        </>
    );
}
