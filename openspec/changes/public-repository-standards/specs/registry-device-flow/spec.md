# Spec Delta

## Purpose

Make the registry device authorization flow agree across native clients and the consent service.

## ADDED Requirements

### Requirement: Device client identity agrees

The native registry client and consent service MUST use the registered ejm-cli device client identifier for device code requests and token polling. Unregistered identifiers MUST be rejected.

#### Scenario: Native registry login starts

- **WHEN** ejm-cli requests and polls a device login
- **THEN** both requests use the identifier accepted by the consent service

#### Scenario: An unregistered client starts login

- **WHEN** an unknown client identifier is submitted
- **THEN** the service rejects it without granting tokens
