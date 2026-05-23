# ADR-004: Interface vs Implementation Model

## Status
Accepted

## Context
Skills need to be invocable without coupling to specific implementations. Different environments may have different concrete tools for the same abstract capability.

## Decision
Adopt an interface/implementation model:
- **Interface**: abstract capability (e.g. "unit-testing")
- **Implementation**: concrete realisation (e.g. "pytest-runner")
- Resolution maps interfaces to implementations using hints

## Rationale
- Enables polyglot skills (same interface, different language implementations)
- Allows runtime flexibility (swap implementations without changing prompts)
- Clear separation of "what" (interface) from "how" (implementation)
- Hint-based resolution is powerful yet simple

## Consequences
- Registry must track interface→implementation mappings
- Resolution must be deterministic (fail on ambiguity)
- Implementations must declare which interface they fulfil
