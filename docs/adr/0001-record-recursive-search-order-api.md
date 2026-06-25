# ADR 0001: Record the recursive search order API

- Status: Accepted
- Date: 2026-06-25

## Context

PostgreSQL supports recursive Common Table Expression (CTE) search ordering with
`SEARCH BREADTH FIRST` and `SEARCH DEPTH FIRST` clauses. The crate already
models recursive CTE construction through `RecursiveCTEExt`, `RecursiveParts`,
and `WithRecursive`, so callers need a way to opt in to search ordering without
constructing query internals directly.

## Decision

In the context of exposing PostgreSQL recursive CTE search ordering, facing the
concern that callers need a stable public API whilst the generated SQL carries
backend-specific details, we decided for a public `SearchStyle` enum and a
chainable `WithRecursive::with_search` method backed by an internal
`SearchConfig`, and against exporting `SearchConfig` or requiring callers to
assemble raw `SEARCH` fragments themselves, to achieve a small typed API that
fits the existing builder flow, accepting that future PostgreSQL search clause
options may require extending the method or adding a second builder step.

## Consequences

- `SearchStyle` is the only public type needed to select breadth-first or
  depth-first search order.
- `SearchConfig` remains crate-private, so the crate can change the SQL
  generation storage without creating a public SemVer commitment.
- Tests for recursive CTE SQL generation should cover each `SearchStyle`
  variant because the mapping is part of the public API contract.
