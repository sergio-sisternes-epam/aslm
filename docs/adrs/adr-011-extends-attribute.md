# ADR-011: `extends=` Attribute for Interface Inheritance

## Status
Accepted

## Context

AML has a two-level model for skill relationships:

- `define="interface"` — an abstract contract (e.g. `diagram-driven-execution`)
- `define="implementation"` — a concrete realisation linked via `implements=`

Multi-level capability hierarchies are common in practice. The DDE project
defines a three-level structure:

```
diagram-driven-execution        ← top-level interface
        ├── dde-simple          ← sub-interface (mode contract)
        └── dde-advanced        ← sub-interface (mode contract)
              └── (implementations)
```

`dde-simple` and `dde-advanced` are **interfaces**, not implementations — they
cannot be invoked directly. They narrow the parent contract with mode-specific
constraints. In UML terms this is **interface inheritance** (extends), not
**class realisation** (implements).

Before this ADR, the only way to express the parent link was:

```xml
<skill define="interface" name="dde-simple"
       implements="diagram-driven-execution">
```

This is semantically incorrect. `implements=` means "I am a concrete executable
that satisfies this contract." Using it on a `define="interface"` node implies
the interface is directly invocable, which is wrong and confuses tooling and
human reviewers alike.

## Decision

Add an `extends=` attribute to `<skill define="interface">` nodes:

```xml
<skill define="interface" name="dde-simple"
       extends="diagram-driven-execution">
```

**Semantics:**
- `extends` is valid only on `define="interface"` nodes.
- It names the parent interface that this interface specialises.
- The `extends` graph is **metadata and validation only** for this release.
  Implementations registered for a child interface are NOT automatically
  candidates when resolving an invocation targeting the parent interface.
  This avoids contract-compatibility problems without a full subtype rule-set.

**Validation rules (new):**
- `extends=""` → hard error (empty parent name).
- `extends="X"` and `implements="Y"` (different values) on same InterfaceDef → hard error.
- `implements=` on `define="interface"` (without a matching `extends=`) → deprecation warning; will become a hard error in a future release.

**Registry rules (new):**
- `SkillRegistry::validate()` checks that `extends` references a known interface
  (`ExtendsUnknownInterface` error if not).
- Cycle detection: self-extension and transitive cycles produce
  `ExtendsInterfaceCycle` errors.

## Rationale

- **Semantic correctness** — `extends` is the correct term for interface
  specialisation in every major type system (Java, TypeScript, UML). Using
  `implements` on abstract nodes is misleading.
- **Tooling safety** — A runtime or graph traversal tool that sees
  `implements=` on a `define="interface"` node could incorrectly treat the
  interface as directly invocable.
- **Precedent** — Every multi-level AML hierarchy (capability > sub-capability
  > implementation) hits this gap. Fixing it now prevents the problem from
  compounding as skills grow more compositional.
- **Metadata-only for v1** — Transitive resolution (child impls satisfy parent
  invocations) requires contract-compatibility rules (parameter sets, return
  types, tool constraints) that are not yet specified. Scoping `extends` to
  metadata avoids premature complexity.

## Migration Path for Existing Consumers

Replace:
```xml
<skill define="interface" name="child" implements="parent">
```
With:
```xml
<skill define="interface" name="child" extends="parent">
```

This is a non-breaking change for any parser that does not yet validate
attribute names. Existing documents using `implements=` on interface nodes
will emit a deprecation **warning** (not an error) during this release.

## Consequences

- The `InterfaceDefinition` AST node gains two new fields: `extends` and
  `legacy_implements`.
- `SkillRegistry::InterfaceEntry` gains `extends: Option<String>`.
- `SkillRegistry::register_interface` gains an `extends` parameter.
- `SkillRegistry::validate()` now also checks the `extends` graph for
  unknown parents and cycles.
- Python bindings: `register_interface` gains an optional `extends` keyword
  argument (default `None`).
- Resolution behaviour is unchanged — `extends` carries no runtime semantics.
