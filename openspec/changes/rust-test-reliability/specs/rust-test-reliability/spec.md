## ADDED Requirements

### Requirement: Detect inconsistent Rust test outcomes

The reliability workflow SHALL run the Cargo suite and a structured Rust test runner independently twice, compare test identities and outcomes, and fail when any run fails or evidence is incomplete.

#### Scenario: A test fails once and passes once

- **WHEN** the two structured runs produce a pass and a failure for the same test
- **THEN** the workflow names the inconsistent test and fails

#### Scenario: A run cannot complete

- **WHEN** a run or its report is missing or incomplete
- **THEN** the workflow fails rather than claiming reliability

#### Scenario: A test is omitted from both reports

- **WHEN** a platform's discovered test is missing or skipped in either report
- **THEN** the workflow fails, even if both reports omit or skip it

#### Scenario: An unadmitted pull request

- **WHEN** a pull request has not been admitted by a human
- **THEN** the separate reliability workflow does not run it
