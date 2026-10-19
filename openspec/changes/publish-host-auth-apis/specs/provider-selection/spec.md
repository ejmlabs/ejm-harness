# Spec Delta

## Purpose

Let hosts configure a model provider between turns while retaining readable conversation history and preventing incompatible opaque reasoning from crossing provider identities.

## ADDED Requirements

### Requirement: Unconfigured hosts preserve prompts

The engine SHALL allow creation without a provider and SHALL reject submitted turns before persisting a prompt or run-start event until a provider is configured.

#### Scenario: Submit before sign-in

- **WHEN** a host submits or queues a prompt without a configured provider
- **THEN** submission fails and the durable session history remains unchanged

### Requirement: Provider changes require idle managed work

The engine SHALL reject provider changes while any managed run or its queued work remains active and SHALL retain the existing provider after a rejected change.

#### Scenario: Another session is busy

- **WHEN** a host changes providers while a different session has active or queued work
- **THEN** the change fails and the active run retains its provider

#### Scenario: Change between turns

- **WHEN** every managed run is idle and a valid provider configuration is supplied
- **THEN** the next turn uses the new provider without changing session identity or tool grants

### Requirement: Durable provider binding protects continuation

The engine SHALL persist a bounded nonsecret binding identity. A changed identity SHALL atomically clear opaque message continuation while preserving readable history and tool evidence. The same identity SHALL preserve continuation across restart.

#### Scenario: Change model or account

- **WHEN** an idle host configures a different binding identity
- **THEN** future requests retain readable history and tool evidence but omit prior opaque continuation

#### Scenario: Resume the same binding

- **WHEN** the engine restarts and configures the same binding identity
- **THEN** previously persisted opaque continuation remains available

#### Scenario: Configuration fails

- **WHEN** binding validation or journal mutation fails
- **THEN** neither the active provider selection nor the stored continuation changes

### Requirement: Existing integrations remain compatible

The engine SHALL preserve existing constructors, public event types and worker wire types.

#### Scenario: Existing host creates a harness

- **WHEN** an existing host supplies its provider through an existing constructor
- **THEN** it can start and complete turns without adopting the new configuration API

#### Scenario: Legacy host reopens a tracked journal

- **WHEN** a host starts an unbound turn on a journal with a previously configured binding
- **THEN** the prior binding and opaque continuation are cleared before admission, and later explicit configuration treats subsequent history as unbound
