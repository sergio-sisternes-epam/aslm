# ADR-005: Definitions Purely in SML Syntax

## Status
Accepted

## Context
Interfaces and implementations need to be defined somewhere. Options:
- In APM manifest (apm.yml)
- In separate config files
- In SML syntax itself

## Decision
Interface and implementation definitions are expressed purely through SML syntax using the `define` attribute.

## Rationale
- Self-contained: SML documents carry their own definitions
- No dependency on APM manifest for runtime semantics
- Definitions can be embedded in system prompts alongside invocations
- Keeps SML independent from any specific packaging system

## Consequences
- Parser must distinguish definition nodes from invocation nodes
- Definition nodes are non-executable (registration phase only)
- The `define` attribute presence determines node type
