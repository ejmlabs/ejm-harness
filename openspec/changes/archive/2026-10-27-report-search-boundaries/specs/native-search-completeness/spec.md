# Spec Delta

## Purpose

Keep bounded native search results honest about content omitted by traversal limits.

## ADDED Requirements

### Requirement: Search reports boundary-limited results

Native search SHALL identify results as incomplete when its depth or total byte boundary may have omitted searchable content. It SHALL retain its existing bounded traversal and output.

#### Scenario: Directory at the depth boundary

- **WHEN** search encounters a searchable directory at its depth limit
- **THEN** the result reports that the depth limit may have omitted content

#### Scenario: Total byte boundary reached in the last file

- **WHEN** search consumes its total byte budget while reading a file
- **THEN** the result reports the byte limit even if no subsequent entry is visited

#### Scenario: Scan remains below boundaries

- **WHEN** the walk completes without a limit or skipped entry
- **THEN** the result does not report an incomplete scan
