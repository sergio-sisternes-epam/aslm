# SML Error Model

## Overview

SML defines a structured error model covering parsing, validation, resolution,
and execution. All errors include source spans for precise tooling integration.

## Error Categories

### Parse Errors

Raised during Phase 1 (Parse). These indicate syntactically invalid SML.

| Error | Cause | Span points to |
|---|---|---|
| `UnclosedTag` | `<skill ...>` without matching `</skill>` | Opening tag |
| `MismatchedClose` | `</skill>` doesn't match the nearest open tag | Both open and close |
| `MalformedAttribute` | Attribute syntax error (missing `=`, unclosed quote) | Attribute position |
| `UnexpectedEOF` | Document ends mid-tag | Last valid position |

Parse errors are **unrecoverable** — the document cannot proceed past Phase 1.

### Validation Errors

Raised during Phase 2 (Validate). These indicate well-formed but semantically
invalid SML.

| Error | Cause | Span points to |
|---|---|---|
| `ConflictingAttributes` | Mutually exclusive attributes present | Node with conflicts |
| `MissingRequiredAttribute` | Required attribute for node type is absent | Node |
| `InvalidAttributeValue` | Value doesn't match expected type/range | Attribute value |
| `UnknownDefineValue` | `define` is neither "interface" nor "implementation" | `define` attribute |

Validation errors are **unrecoverable** — the document cannot proceed.

### Registration Errors

Raised during Phase 3 (Register Definitions).

| Error | Severity | Cause |
|---|---|---|
| `DuplicateInterface` | Error | Same interface name registered twice in same package |
| `DuplicateImplementation` | Error | Same implementation name registered twice in same package |
| `OrphanImplementation` | Warning | Implementation's `implements` references unknown interface |

`DuplicateInterface` and `DuplicateImplementation` are **unrecoverable**.
`OrphanImplementation` is a warning — execution proceeds (the interface may
be registered by a later-loaded package).

### Resolution Errors

Raised during Phase 4 (Resolve).

| Error | Cause | Recovery |
|---|---|---|
| `NotFound` | No implementation or interface with given name | None — fatal for node |
| `NoImplementation` | Interface exists but no impl matches hints | None — fatal for node |
| `Ambiguous` | Multiple implementations match; no default | None — fatal for node |
| `ImplInterfaceMismatch` | `impl` doesn't implement declared `interface` | None — fatal for node |

Resolution errors are **fatal for the affected node**. Whether they are fatal
for the document depends on the node's `on-failure` attribute:
- `on-failure="halt"` → fatal for document.
- `on-failure="skip"` → node is skipped; document continues.
- `on-failure="partial"` → parent marked partial; document continues.

### Execution Errors

Raised during Phase 5 (Execute).

| Error | Cause | Retryable? |
|---|---|---|
| `SkillFailed` | Implementation returned a failure status | Yes |
| `Timeout` | Execution exceeded the `timeout` duration | Yes |
| `DepthLimitExceeded` | Nesting depth exceeds maximum (default 16) | No |
| `CapabilityDenied` | Skill requires capability not in allowed set | No |
| `PolicyViolation` | Wrapper skill denied child execution | No |

Retryable errors trigger the retry mechanism if `retries > 0`.
Non-retryable errors immediately propagate per `on-failure` mode.

## Error Structure

All errors share a common structure:

```rust
struct SmlError {
    kind: ErrorKind,          // Category + specific variant
    message: String,          // Human-readable description
    span: SourceSpan,         // Location in source document
    context: Vec<ErrorNote>,  // Additional context (e.g. "while resolving...")
}

struct SourceSpan {
    start: usize,             // Byte offset from document start
    end: usize,               // Byte offset (exclusive)
    line: usize,              // 1-based line number
    column: usize,            // 1-based column number
}

struct ErrorNote {
    message: String,
    span: Option<SourceSpan>, // Optional secondary location
}
```

## Error Reporting

### For CLIs and tooling

Errors are formatted with source context:

```
error[E0301]: no implementation found for interface 'unit-testing-coverage'
  --> prompt.sml:15:3
   |
15 |   <skill interface="unit-testing-coverage" language="go">
   |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: interface 'unit-testing-coverage' has 2 implementations but none match language="go"
   = hint: available implementations: python-pytest-v2 (language=python), js-jest-v1 (language=javascript)
```

### For programmatic use

The Python API exposes structured error objects:

```python
from sml import parse, SmlError, ErrorKind

try:
    tree = parse(prompt)
except SmlError as e:
    print(e.kind)        # ErrorKind.UnclosedTag
    print(e.span.line)   # 15
    print(e.message)     # "unclosed <skill> tag"
    print(e.context)     # [ErrorNote(...)]
```

## Aggregated Errors

Some phases can report multiple errors at once (e.g. validation may find
several invalid nodes). In these cases, ALL errors are collected and reported
together rather than failing on the first.

```rust
struct SmlErrors {
    errors: Vec<SmlError>,    // All errors found
    warnings: Vec<SmlError>,  // Non-fatal issues
}
```

Processing stops at the **first phase with errors**. For example, if validation
finds 3 errors, resolution does not run — all 3 errors are reported together.

## Warning Semantics

Warnings do NOT prevent execution. They are:
- Collected during processing.
- Included in the execution trace.
- Reported to the caller after successful execution.
- Available for lint tools to surface.

Current warning conditions:
- Unknown attributes (forward compatibility).
- `OrphanImplementation` (interface may be loaded later).
- Clamped values (`retries > 10`, `timeout > 1h`).
- Collision with warning (last-registered wins).
