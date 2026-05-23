# ADR-007: APM Harness Independent from SML

## Status
Accepted

## Context
SML needs a distribution mechanism. APM (Agent Package Manager) provides this, but coupling them would limit SML's applicability.

## Decision
SML and APM are independent layers:
- SML only parses and executes skill tags
- APM handles package discovery, installation, and versioning
- Neither depends on the other's internals

## Rationale
- SML can be used without APM (embedded in any system)
- APM can distribute non-SML content
- Clear separation of concerns
- Easier testing and development of each layer

## Consequences
- No `import` or `require` statements in SML
- Runtime must receive skill implementations through its registry API
- APM integration is a wrapper/adapter, not core functionality
