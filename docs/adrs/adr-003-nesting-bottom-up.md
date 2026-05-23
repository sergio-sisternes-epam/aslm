# ADR-003: Full Nesting with Bottom-Up Execution

## Status
Accepted

## Context
Skills may depend on the output of other skills. Nesting enables composition but requires a defined execution order.

## Decision
- Full nesting support (skills inside skills)
- Default execution is bottom-up: inner skills execute first
- Results of inner skills are injected into the outer scope
- Alternative policies (wrapper, sequential) available via `policy` attribute

## Rationale
- Bottom-up is the most intuitive default (dependencies resolve first)
- Wrapper policy enables security/sandbox patterns
- Sequential enables pipeline patterns
- Explicit policy attribute prevents ambiguity

## Consequences
- Execution order is deterministic and predictable
- Circular dependencies are structurally impossible (tree structure)
- Wrapper skills must explicitly handle their children
