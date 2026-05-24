# Contributing to AML

Thank you for your interest in contributing! This document explains how to get involved.

## Code of Conduct

Please be respectful and constructive in all interactions. See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) once available.

## How to Contribute

### Reporting Bugs

Open a [bug report](https://github.com/sergio-sisternes-epam/aml/issues/new?template=bug_report.md) with a clear description and a minimal reproduction case.

### Suggesting Features

Open a [feature request](https://github.com/sergio-sisternes-epam/aml/issues/new?template=feature_request.md) describing the problem you want to solve and your proposed solution.

### Submitting Changes

1. **Fork** the repository and create a branch from `main`:
   ```bash
   git checkout -b feat/your-feature-name
   ```
2. **Set up** the development environment (see below).
3. **Make your changes** with tests and documentation.
4. **Ensure** all checks pass locally.
5. **Open a pull request** against `main` with a clear description.

## Development Setup

### Prerequisites

- Rust (stable toolchain) — install via [rustup](https://rustup.rs/)
- Python ≥ 3.9 — for the Python bindings
- [maturin](https://www.maturin.rs/) — for building the PyO3 extension

### Building

```bash
# Build the Rust workspace
cargo build

# Build the Python extension (development mode)
pip install maturin
maturin develop --manifest-path crates/aml-python/Cargo.toml
```

### Running Tests

```bash
# Rust unit and integration tests
cargo test

# Python tests
pip install -e ".[dev]"
pytest
```

### Linting & Formatting

```bash
# Rust
cargo fmt --check
cargo clippy -- -D warnings

# Python
ruff check .
ruff format --check .
```

All checks are enforced in CI and must pass before a PR can be merged.

## Commit Conventions

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <short summary>

[optional body]

[optional footer(s)]
```

Common types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `perf`.

Examples:
- `feat(parser): add support for nested skill tags`
- `fix(executor): handle empty param nodes correctly`
- `docs: update quick start example`

## Coding Style

- **Rust**: follow `rustfmt` defaults; clippy warnings are errors in CI.
- **Python**: follow `ruff` defaults (PEP 8 compatible).
- Write doc comments on all public APIs.
- Keep commits atomic — one logical change per commit.

## Licence

By contributing, you agree that your contributions will be licensed under the [Apache 2.0 Licence](LICENSE).
