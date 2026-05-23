# ADR-008: Core Runtime in Rust + PyO3

## Status
Accepted

## Context
The SML runtime needs to be fast, safe, and accessible from Python (the dominant AI/ML ecosystem language).

## Decision
Implement the core runtime in Rust with Python bindings via PyO3.

## Rationale
- **Performance**: Rust provides zero-cost abstractions and no GC pauses
- **Safety**: Memory safety without garbage collection
- **Error reporting**: Source spans and structured errors are natural in Rust
- **Python access**: PyO3 provides seamless Python bindings
- **Distribution**: maturin builds cross-platform wheels easily
- **Correctness**: Strong type system catches bugs at compile time

## Consequences
- Contributors need Rust knowledge for core changes
- Build requires Rust toolchain + maturin
- Cross-compilation needed for all target platforms
- Two test suites (Rust unit tests + Python integration tests)
