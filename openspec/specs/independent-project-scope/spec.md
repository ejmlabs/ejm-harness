# independent-project-scope Specification

## Purpose

Keep the open source harness independent in source identity, distribution, service configuration, and contributor decisions.

## Requirements

### Requirement: Independent open source identity

Repository source, docs, examples, contributor instructions, metadata, fixtures, and distributed artifacts SHALL identify only this project and its legitimate dependencies. They SHALL contain no references, endpoints, product contracts, or reserved identities from unrelated proprietary products.

#### Scenario: Review repository and release inputs

- **WHEN** maintainers inspect source, generated release inputs, and nested archives
- **THEN** this project's identity and package contracts remain independent of unrelated proprietary products

### Requirement: Honest registry availability

Registry source SHALL use project-owned resource placeholders without claiming a live service. Test and schema samples SHALL use reserved example origins. Deployment configuration SHALL require an operator-supplied origin and SHALL preserve private auth bindings and resource isolation.

#### Scenario: Prepare the registry source

- **WHEN** an operator reads its source configuration and deployment guide
- **THEN** no live registry is implied and the operator must supply an authorized HTTPS origin before deployment

### Requirement: Retained evidence states revisions

Retained measurement records SHALL preserve their original conclusions and limits. If a bundled source snapshot changes to remove unrelated identity, its receipt SHALL identify the revision and update hashes and sizes without claiming byte identity with measured source.

#### Scenario: Clean a retained measurement bundle

- **WHEN** maintainers revise bundled source identity
- **THEN** the retained receipt records revised bytes and the measurement scope remains explicit
