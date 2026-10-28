# Design

## Context

Search uses the `ignore` walker with a depth limit and reads files through a directory capability. Result text informs the next model request and must fit the engine output bound.

## Goals / Non-Goals

**Goals:** Report potentially omitted content without increasing resource limits.

**Non-Goals:** Change grants, ignore semantics, or the public Rust interface.

## Decisions

Keep traversing other entries after encountering a depth-limited directory, retaining a depth notice unless a later limit ends the scan. Report the byte boundary as soon as it is reached, including on the final entry. This is conservative at an exact boundary and avoids an extra byte read beyond the budget. Keep existing text and structured truncation fields.

## Risks / Trade-offs

An empty directory at the depth limit or an exact byte fit may produce a conservative notice. This avoids extra traversal or reads that exceed the established bounds.
