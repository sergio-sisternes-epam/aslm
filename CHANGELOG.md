# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial Rust parser and executor (`aml-core`)
- Python bindings via PyO3 (`aml-python`)
- AML language specification in `docs/spec/`
- Conformance test suite in `tests/conformance/`
- `extends=` attribute on `<skill define="interface">` nodes for interface inheritance
  (ADR-011). Enables semantically correct multi-level AML hierarchies where one
  interface specialises another. The attribute is metadata and validation only
  in this release — resolution remains unchanged.

### Deprecated

- Using `implements=` on `<skill define="interface">` nodes. This now produces a
  validation warning. Migrate to `extends=`. The attribute will become a hard
  error in a future release.
