# ADR-002: Support Explicit Scoping

## Status
Accepted

## Context
Skills need to know what they operate on. Options:
- Separate `scope` attribute pointing to content
- Content-as-scope (everything inside the tag is the scope)

## Decision
The content inside a `<skill>` tag IS the scope the skill operates on.

## Rationale
- Natural and readable — no indirection
- Matches how humans think about "what this skill should work on"
- Enables composition: nested skill results become part of the outer scope
- No need for external references or ID linking

## Consequences
- Skills cannot operate on content outside their tag boundaries
- Large scopes may make prompts verbose (acceptable trade-off)
