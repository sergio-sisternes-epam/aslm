# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] — 2026-06-10

### Added

- `define="contract"` on `<skill>` nodes for reusable, versioned data-shape contracts with nested `<field>` declarations.
- Contract-aware registry validation for unknown `contract:` references, unknown `extends=` parents, and contract inheritance cycles.
- Python document accessors for contract definitions via `definitions()`, `contracts()`, and `get_contract()`.
- Conformance fixtures covering positive contract authoring and eight negative contract error cases.

### Changed

- Interface and return type validation now accepts `type="contract:<name>"` references.
- Bare `required` is accepted on `<field>` and `<param>` declarations; other bare attributes are rejected during validation.
- Definitions executor passthrough now treats contract definitions as non-executable, returning no output.

## [0.2.0] — 2026-05-25

### Added

- `extends=` attribute on `<skill define="interface">` nodes for interface inheritance
  (ADR-011). Enables semantically correct multi-level AML hierarchies where one
  interface specialises another (e.g. `dde-simple extends diagram-driven-execution`).
  The attribute is metadata and validation only in this release — resolution remains
  unchanged.
- Registry validates `extends` references: unknown parent → `ExtendsUnknownInterface`
  error; cycles → `ExtendsInterfaceCycle` error.

### Changed

- `SkillRegistry::register_interface` gains an `extends: Option<String>` parameter.
- Python bindings: `register_interface(name, extends=None, description=None)`.

### Deprecated

- Using `implements=` on `<skill define="interface">` nodes. This now produces a
  validation warning. Migrate to `extends=`. The attribute will become a hard
  error in a future release.

## [0.1.0] — 2026-05-25

### Added

- Initial Rust parser and executor (`aml-core`)
- Python bindings via PyO3 (`aml-python`)
- AML language specification in `docs/spec/`
- Conformance test suite in `tests/conformance/`
- Directive tags: `<tool>`, `<session>`, `<agent>`
- Typed parameter declarations, DDE node declarations, wrapping skill refs
- Optional `<aml version="...">` root wrapper
