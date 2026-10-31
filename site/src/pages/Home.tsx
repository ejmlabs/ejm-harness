import {
    Button,
    Callout,
    CodeBlock,
    Disclosure,
    Row,
    Rows,
    Section,
    Stat,
    Table,
    type TableColumn,
} from "@ejm/ui";
import {
    Actions,
    Columns,
    ContentGroup,
    ContentHeading,
    InlineCode,
    InlineLink,
    MetricGrid,
} from "@ejm/ui/layout/SiteContent";
import { HarnessHeroWindow, ProductHeroStage } from "@ejm/demos/hero";

const features = [
    {
        title: "Waiting costs nothing",
        description:
            "A session awaiting a model holds no process. Sessions are light state that share one provider, tool registry, store and governor.",
    },
    {
        title: "One governor for all work",
        description:
            "Commands reserve CPU, memory, processes and output before they start. Model requests and local jobs have separate limits, and a model cannot raise them.",
    },
    {
        title: "Fair queues",
        description:
            "Requests rotate between sessions, first in first out within each. A dropped waiter leaves at once, and a lease frees every reservation when it ends.",
    },
    {
        title: "Journal first, never replay",
        description:
            "A bounded SQLite journal records every event. After a crash, the engine recovers the run and never repeats an effect whose outcome is uncertain.",
    },
    {
        title: "Native file tools",
        description:
            "Read, list, search, write and patch without spawning a shell. Shell access is optional and keeps normal Bash semantics when granted.",
    },
    {
        title: "Guarantees stated plainly",
        description:
            "Each restriction is reported as enforced, monitored or unsupported. Native execution runs as you and is not a sandbox.",
    },
];

const embedRust = `use std::sync::Arc;
use ejm_harness::{Harness, HarnessConfig, ReplayProvider, SessionOptions};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let harness = Harness::new(
        HarnessConfig::default(),
        Arc::new(ReplayProvider::echo()),
        vec![],
    )?;
    let session = harness.create_session(SessionOptions {
        cwd: std::env::current_dir()?,
        title: "First agent".into(),
    }).await?;
    let mut run = harness.start(&session.id, "Hello".into()).await?;
    println!("{}", run.wait().await?.text);
    harness.shutdown().await?;
    Ok(())
}`;

const pythonSdk = `async with AsyncHarness(
    config={"provider": {"kind": "echo"}}
) as harness:
    session = await harness.create_session(
        title="First agent"
    )
    run = await session.start("Hello")
    async for event in run:
        if event["type"] == "text_delta":
            print(event["text"], end="")`;

const build = `cargo build --release --workspace --locked
cargo test --workspace --locked`;

const packageColumns: readonly TableColumn[] = [
    { key: "package", label: "Package" },
    { key: "responsibility", label: "Responsibility" },
];

const packages = [
    ["ejm-harness", "Sessions, agent loop, providers, context, events, recovery"],
    ["ejm-harness-exec", "Shared resource admission and command supervision"],
    ["ejm-harness-auth", "Sign-in, verified identity, credential storage, and serialized renewal"],
    ["ejm-harness-coding", "Native read, list, search, write, patch, and optional shell tools"],
    ["ejm-harness-worker", "Persistent JSONL bridge for language SDKs"],
    ["sdk/typescript", "Typed async API and TypeScript tool callbacks"],
    ["sdk/python", "Sync and async Python API and tool callbacks"],
    ["ejm-harness-bench", "Deterministic concurrency and command workloads"],
].map(([name, responsibility]) => ({
    id: name,
    package: <InlineCode>{name}</InlineCode>,
    responsibility,
}));

export default function Home() {
    return (
        <>
            <ProductHeroStage
                eyebrow="engine"
                title="ejm-harness runs a hundred agents."
                actions={
                    <>
                        <Button asChild>
                            <a href="#get">Build from source</a>
                        </Button>
                        <Button asChild variant="secondary">
                            <a href="#embed">Embed in Rust</a>
                        </Button>
                    </>
                }
                footnote={
                    <>
                        Built on it:{" "}
                        <InlineLink href="https://cli.ejmlabs.com/" external>
                            ejm-cli
                        </InlineLink>{" "}
                        and{" "}
                        <InlineLink href="https://ide.ejmlabs.com/" external>
                            ejm-ide
                        </InlineLink>
                        .
                    </>
                }
                windows={<HarnessHeroWindow />}
            >
                A reusable Rust engine for coding agents. Sessions that wait on a model own no shell
                and no thread, and every command, tool and child agent is admitted, bounded and
                journaled.
            </ProductHeroStage>

            <Section
                id="how"
                kicker="how it works"
                title="The engine keeps the hard parts."
                intro="Run many sessions through one resource governor, keep a durable event journal, and extend the engine from Rust, TypeScript or Python."
            >
                <Rows aria-label="What the engine does">
                    {features.map((f) => (
                        <Row key={f.title} title={f.title} description={f.description} />
                    ))}
                </Rows>
            </Section>

            <Section
                id="embed"
                kicker="embed in rust"
                title="One harness owns the provider, tools, governor and journal."
                intro={
                    <p>
                        During source development, depend on{" "}
                        <InlineCode>crates/ejm-harness</InlineCode> from your own Cargo project. Add{" "}
                        <InlineCode>ejm-harness-coding</InlineCode> when your host needs the
                        standard coding tools.
                    </p>
                }
            >
                <Columns
                    primary={
                        <ContentGroup>
                            <ContentHeading>Runs offline, with no account.</ContentHeading>
                            <p>
                                The replay provider replies with{" "}
                                <InlineCode>Demo: Hello</InlineCode> and makes no network calls.
                                Real provider credentials stay host configuration.
                            </p>
                        </ContentGroup>
                    }
                    secondary={
                        <CodeBlock
                            code={embedRust}
                            lang="rust"
                            label="Embedding the harness in a Rust host"
                        />
                    }
                />
            </Section>

            <Section
                id="sdks"
                kicker="sdks"
                title="TypeScript and Python share one worker."
                intro="Both SDKs share one persistent Rust worker across their sessions. There is no Node or Python dependency for Rust hosts or ejm-cli."
            >
                <Columns
                    primary={
                        <ContentGroup>
                            <ContentHeading>Custom tools stay ordinary functions.</ContentHeading>
                            <p>
                                Tools are plain functions in your language, with schema validation,
                                progress and cooperative cancellation. Credentials travel over
                                stdin, never through arguments.
                            </p>
                        </ContentGroup>
                    }
                    secondary={
                        <CodeBlock code={pythonSdk} lang="python" label="Using the Python SDK" />
                    }
                />
            </Section>

            <Section
                id="light"
                kicker="footprint"
                title="Light by design."
                intro="Agent capacity, model capacity and local job capacity are separate. A hundred waiting agents share a small pool of local work instead of a shell each."
            >
                <MetricGrid>
                    <Stat figure="100" label="sessions managed together" />
                    <Stat
                        figure="8"
                        label="model requests at once by default, with 4 local jobs and 1 heavy job"
                    />
                    <Stat figure="26" label="MiB sampled engine memory running 300 Bash checks" />
                    <Stat figure="0" label="hosted model calls in tests and benchmarks" />
                </MetricGrid>
                <Callout>
                    Measured on 18 October 2026 on a Mac16,8 with 12 logical CPUs and 24 GiB RAM,
                    using deterministic provider responses. A hundred sessions each ran three Bash
                    checks; sampled engine resident memory was 25.72 MiB and sampled peak local jobs
                    was 4. A modern Mac does not prove the four core, 8 GB target, and this run
                    excludes remote model latency and coding quality.
                </Callout>
            </Section>

            <Section
                id="packages"
                kicker="packages"
                title="Small packages with one job each."
                intro={
                    <p>
                        Swap behavior at the edges with <InlineCode>Provider</InlineCode>,{" "}
                        <InlineCode>Tool</InlineCode>, <InlineCode>ContextPolicy</InlineCode> and{" "}
                        <InlineCode>RunStrategy</InlineCode>. The engine keeps control of grants,
                        budgets, validation and journaling, and a repository cannot grant itself a
                        tool.
                    </p>
                }
            >
                <Table
                    label="The main packages"
                    caption="The main packages and what each is responsible for"
                    columns={packageColumns}
                    rows={packages}
                    rowHeader="package"
                    rowKey="id"
                    truncate={false}
                />
            </Section>

            <Section
                id="get"
                kicker="get it"
                title="Build ejm-harness from source."
                intro={
                    <p>
                        The repository contains source packages. Registry releases and prebuilt
                        worker binaries have not been published. It uses the pinned Rust toolchain
                        and is MIT licensed. Persistent credentials require an OS key service; Linux
                        builds also need D-Bus development headers. Read the{" "}
                        <InlineLink href="/docs/host-auth/">host authentication guide</InlineLink>.
                    </p>
                }
            >
                <Columns
                    primary={
                        <ContentGroup>
                            <ContentHeading>Tests never call paid models.</ContentHeading>
                            <p>
                                Tests use replay fixtures. The terminal app built on the engine is{" "}
                                <InlineLink href="https://cli.ejmlabs.com/" external>
                                    ejm-cli
                                </InlineLink>
                                , and the desktop app is{" "}
                                <InlineLink href="https://ide.ejmlabs.com/" external>
                                    ejm-ide
                                </InlineLink>
                                .
                            </p>
                        </ContentGroup>
                    }
                    secondary={
                        <CodeBlock
                            code={build}
                            lang="sh"
                            label="Building and testing the harness"
                        />
                    }
                />
            </Section>

            <Section id="faq" kicker="questions" title="Before you build on it.">
                <Disclosure summary="Does it need an account or a server?">
                    <p>
                        No. Neither ejm-harness nor ejm-cli requires a hosted workspace account or
                        server.
                    </p>
                </Disclosure>
                <Disclosure summary="Is native execution a sandbox?">
                    <p>
                        No. It runs as the current user. The executor reports each restriction as
                        enforced, monitored or unsupported, and rejects required protections that
                        are unsupported before launch.
                    </p>
                </Disclosure>
                <Disclosure summary="Which model providers work?">
                    <p>
                        Chat Completions compatible streaming APIs, including DeepSeek endpoints,
                        the Anthropic Messages API, native OpenAI Responses with an API key or
                        ChatGPT sign-in, and the official Claude Code subscription adapter. The{" "}
                        <InlineLink href="/docs/host-auth/#claude">Claude route</InlineLink>{" "}
                        requires a separately installed official client and retains its managed
                        provider policy.
                    </p>
                </Disclosure>
                <Disclosure summary="Do I need Node or Python?">
                    <p>
                        Only if you use that SDK. Rust hosts and ejm-cli have no Node or Python
                        dependency.
                    </p>
                </Disclosure>
            </Section>

            <Section
                title="See it in an app."
                emphasis
                intro="ejm-cli and ejm-ide are separate apps built on ejm-harness. Both are open source under the MIT license."
                actions={
                    <Actions>
                        <Button asChild>
                            <a href="https://cli.ejmlabs.com/" rel="external">
                                Try ejm-cli
                            </a>
                        </Button>
                        <Button asChild variant="secondary">
                            <a href="https://ide.ejmlabs.com/" rel="external">
                                See ejm-ide
                            </a>
                        </Button>
                    </Actions>
                }
            />
        </>
    );
}
