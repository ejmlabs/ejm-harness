# Spec Delta

## Purpose

Let hosts use a Claude subscription through the official Claude Code client while keeping conversation and tool execution in the shared harness.

## ADDED Requirements

### Requirement: Official client owns subscription authentication

The harness SHALL delegate Claude subscription sign-in, status and sign-out to an unmodified official client. It SHALL expose only bounded nonsecret status, honor cancellation and deadlines, and never collect or copy subscription credentials.

#### Scenario: Sign in with an existing subscription

- **WHEN** a host starts Claude sign-in
- **THEN** the official client completes its own authentication flow
- **AND** the harness returns safe status without reading its credential store

#### Scenario: Wrong authentication source

- **WHEN** the official client reports API-key or third-party authentication
- **THEN** subscription configuration fails with guidance instead of silently changing billing

### Requirement: Auth cleanup retires completed Unix ownership

The auth supervisor SHALL retain its Unix process-group identity through pending cleanup and errors. After group termination and child reaping succeed, it SHALL retire that numeric identity before later asynchronous work so dropping the completed owner cannot signal a reused group identifier. Windows Job cleanup ownership SHALL remain unchanged.

#### Scenario: Auth cleanup succeeds

- **WHEN** the auth supervisor successfully terminates its captured Unix group and reaps its child
- **THEN** it retires the stored group identity before awaiting output-reader cleanup
- **AND** dropping the completed owner does not signal that retired identity

#### Scenario: Cleanup is interrupted

- **WHEN** termination fails or cleanup is abandoned while awaiting child reaping
- **THEN** the Unix group identity remains armed for the existing cleanup path

### Requirement: Harness retains tools and conversation state

The Claude provider SHALL return canonical model responses and proposed tool calls to the harness. It SHALL disable the client's built-in tools, MCP tools, session persistence and ordinary customizations. Managed provider policy SHALL remain effective and its native execution limits SHALL be documented.

#### Scenario: Model proposes a tool

- **WHEN** Claude returns a valid structured tool proposal
- **THEN** the existing harness loop validates and admits that tool before execution
- **AND** the proposal alone executes no harness tool

### Requirement: Bounded supervised inference

Claude model processes SHALL share the host's executor governor, accept bounded private stdin, honor cancellation and time limits, and reject failed, malformed or truncated output. Inputs and credentials SHALL stay out of command arguments and diagnostics.

#### Scenario: Cancel a waiting model process

- **WHEN** a host cancels an in-flight Claude request
- **THEN** process cleanup retains its admission ownership until the supervised child is reaped

#### Scenario: Invalid client output

- **WHEN** the client fails or emits malformed, oversized or incomplete structured output
- **THEN** the provider returns an error without executing a proposed tool or exposing raw output

### Requirement: Existing provider contracts remain available

ChatGPT subscription access, Anthropic API keys and OpenAI-compatible API keys SHALL remain separate supported routes.

#### Scenario: Select an API provider

- **WHEN** a host configures an existing API-key provider
- **THEN** it uses the existing HTTP provider contract without requiring Claude Code
