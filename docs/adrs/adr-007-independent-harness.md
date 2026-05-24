# ADR-007: APM Harness Independent from AML

## Status
Accepted

## Context
AML needs a distribution mechanism. APM (Agent Package Manager) provides this, but coupling them would limit AML's applicability.

## Decision
AML and APM are independent layers:
- AML only parses and executes skill tags
- APM handles package discovery, installation, and versioning
- Neither depends on the other's internals

## Rationale
- AML can be used without APM (embedded in any system)
- APM can distribute non-AML content
- Clear separation of concerns
- Easier testing and development of each layer

## Consequences
- No `import` or `require` statements in AML
- Runtime must receive skill implementations through its registry API
- APM integration is a wrapper/adapter, not core functionality
