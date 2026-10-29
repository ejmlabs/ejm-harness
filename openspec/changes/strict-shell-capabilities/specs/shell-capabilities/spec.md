## ADDED Requirements

### Requirement: Host-selected shell restrictions

The coding tool registration API SHALL allow the host to require enforced process-tree cancellation for every shell invocation. The default registration SHALL preserve existing behavior.

Worker and SDK hosts SHALL be able to require enforced process-tree cancellation for shell invocations through an optional configuration flag that defaults to false.

SDKs SHALL require the worker to advertise support for the strict option before
sending configuration when it is enabled. Workers without that capability
remain compatible when the option is disabled.

#### Scenario: Enforced cleanup unavailable

- **GIVEN** a host requires enforced process-tree cancellation for the registered shell
- **WHEN** the native executor reports that capability as monitored or unsupported
- **THEN** the shell command SHALL fail before admission and process launch.

#### Scenario: Existing registration

- **GIVEN** the host uses the existing coding tool registration API
- **WHEN** a shell command is granted and executed
- **THEN** no additional restriction SHALL be required by default.
