# AML Attribute Reference

## Overview

Every attribute in AML has a defined type, allowed values, and validity scope.
This document is the canonical reference for attribute semantics.

## Attribute Table

| Attribute | Type | Allowed values | Default | Valid on | Required |
|---|---|---|---|---|---|
| `define` | enum | `"interface"`, `"implementation"`, `"contract"` | — | InterfaceDef, ImplementationDef, ContractDef | Yes (on definitions) |
| `name` | string | 1-128 chars, `[a-z0-9-/]` | — | All node types | Yes (on definitions and Lookup invocations) |
| `interface` | string | 1-128 chars, `[a-z0-9-/]` | — | Invocation | No |
| `impl` | string | 1-128 chars, `[a-z0-9-/]` | — | Invocation | No |
| `extends` | string | 1-128 chars, `[a-z0-9-/]` | — | InterfaceDef, ContractDef | No |
| `implements` | string | 1-128 chars, `[a-z0-9-/]` | — | ImplementationDef | Yes |
| `language` | string | free text, lowercase recommended | — | Invocation, ImplementationDef | No |
| `framework` | string | free text, lowercase recommended | — | Invocation, ImplementationDef | No |
| `description` | string | free text, <= 1024 chars | — | InterfaceDef, ImplementationDef, ContractDef | No |
| `version` | string | free text, semver-ish recommended | — | ContractDef | No |
| `type` | string | primitive types or `contract:<name>` | `"string"` by convention | ParamDecl, ReturnsDecl, FieldDecl | No |
| `required` | boolean / bare flag | `"true"`, `"false"`, or bare `required` | `false` | ParamDecl, FieldDecl | No |
| `default` | string | free text | — | ParamDecl, FieldDecl | No |
| `values` | string | pipe-separated values | — | ParamDecl, ReturnsDecl, FieldDecl | No |
| `retries` | uint | 0-10 | 0 | Invocation | No |
| `timeout` | duration | e.g. "30s", "5m", "1h" | no timeout | Invocation | No |
| `policy` | enum | `"bottom-up"`, `"wrapper"`, `"sequential"` | `"bottom-up"` | Invocation | No |
| `on-failure` | enum | `"halt"`, `"skip"`, `"partial"` | `"halt"` | Invocation, SessionDirective, AgentDirective | No |
| `allow` | string | comma-separated tool names | — | ToolDirective | No |
| `deny` | string | comma-separated tool names | — | ToolDirective | No |
| `isolated` | boolean | `"true"`, `"false"` | `"true"` | SessionDirective | No |
| `model` | string | free text | — | AgentDirective | No |
| `mode` | enum | `"sync"`, `"background"` | `"sync"` | AgentDirective | No |

## Mutual Exclusivity Rules

The following combinations are **invalid** and MUST be rejected during validation:

| Combination | Reason |
|---|---|
| `define` + `interface` | Definition cannot also be an invocation target |
| `define` + `impl` | Definition cannot also be an invocation target |
| `define` + `retries` | Definitions are not executable |
| `define` + `timeout` | Definitions are not executable |
| `define` + `policy` | Definitions are not executable |
| `define` + `on-failure` | Definitions are not executable |
| `name` + `interface` (on Invocation) | Use one resolution mode: name OR interface |
| `name` + `impl` (on Invocation) | Use one resolution mode: name OR impl |
| `allow` + `deny` (on ToolDirective) | Use one constraint mode: whitelist OR blacklist |
| `extends` + `implements` (on InterfaceDef, different values) | Conflicting parent declarations |
| `extends` on ImplementationDef or Invocation | `extends` is only valid on interface and contract definitions |
| `implements` on InterfaceDef | Deprecated — use `extends` instead (warning now, error in a future release) |

## Co-occurrence Rules

The following combinations are **valid** with specific semantics:

| Combination | Semantics |
|---|---|
| `interface` + `impl` | `impl` is used directly; runtime validates it implements `interface` |
| `interface` + `language` | `language` is a hint for interface resolution |
| `interface` + `framework` | `framework` is a hint for interface resolution |
| `interface` + `language` + `framework` | Both hints applied conjunctively |
| `impl` + `language` | `language` is ignored (impl is resolved directly) |
| `impl` + `framework` | `framework` is ignored (impl is resolved directly) |

## Node Type Determination

A `<skill>` tag's node type is determined by its attributes:

```
if "define" is present:
    if define == "interface" → InterfaceDef
    if define == "implementation" → ImplementationDef
    if define == "contract" → ContractDef
    else → ValidationError (unknown define value)
else:
    → Invocation (must have at least one of: interface, impl, name)
```

## Directive Node Types

Directive tags are determined by tag name, not by attributes:

| Tag | Node type | Required attributes |
|---|---|---|
| `<tool>` | ToolDirective | At least one of: `name`, `allow`, `deny` |
| `<session>` | SessionDirective | None (all optional) |
| `<agent>` | AgentDirective | `name` |

Directives are **not** skill nodes. They instruct the runtime about execution
environment — tool constraints, session isolation, or subagent delegation.

## `extends=` — Interface and Contract Inheritance

The `extends` attribute expresses **interface or contract inheritance** (specialisation):

```xml
<!-- Parent interface — top-level contract -->
<skill define="interface" name="diagram-driven-execution">...</skill>

<!-- Child interface — narrows the parent contract -->
<skill define="interface" name="dde-simple"
       extends="diagram-driven-execution">...</skill>

<!-- Implementation realises the child interface — unchanged -->
<skill define="implementation" name="dde-simple-impl"
       implements="dde-simple">...</skill>
```

**Semantics:**

- An interface with `extends` is still abstract and **cannot be invoked directly**.
- The hierarchy is **metadata and validation only** — implementations registered for
  a child interface are NOT automatically candidates for the parent interface at
  resolution time. Use explicit `implements=` to link an implementation to each
  interface it satisfies.
- The `extends` graph MUST be acyclic. Cycles (including self-extension) are hard
  errors detected by `SkillRegistry::validate()`.
- The named parent MUST be a registered interface, or `SkillRegistry::validate()`
  reports `ExtendsUnknownInterface`.

**Difference from `implements=`:**

| Attribute | Semantics | Valid on |
|---|---|---|
| `extends` | Interface ← Interface or Contract ← Contract (specialisation) | `define="interface"`, `define="contract"` |
| `implements` | Implementation ← Interface (realisation) | `define="implementation"` |

Using `implements=` on an interface definition is **deprecated** (validation
warning). Migrate to `extends=` to express interface inheritance. This will
become a hard error in a future release.



Names follow this pattern: `[a-z0-9]([a-z0-9-]*[a-z0-9])?(/[a-z0-9]([a-z0-9-]*[a-z0-9])?)?`

Rules:
- Lowercase alphanumeric and hyphens only.
- No leading, trailing, or consecutive hyphens.
- Optional package prefix separated by `/` (e.g. `my-package/my-skill`).
- Minimum 1 character, maximum 128 characters.

## Duration Format

The `timeout` attribute accepts duration strings:

| Format | Meaning | Example |
|---|---|---|
| `Ns` | N seconds | `30s` |
| `Nm` | N minutes | `5m` |
| `Nh` | N hours | `1h` |
| `NsMs` | N seconds + M seconds (invalid) | — |

Only one unit is allowed per value. Fractional values are not supported.
Maximum timeout: `1h`. Values exceeding this are clamped with a warning.

## Retries Semantics

- `retries="0"` — no retries (execute once, same as omitting the attribute).
- `retries="3"` — execute once, then retry up to 3 times on failure (4 total attempts).
- Maximum allowed value: 10. Values above 10 are clamped with a warning.

## Forward Compatibility

Unknown attributes are treated as **warnings**, not errors. This allows AML
documents written for newer versions to be parsed by older runtimes with
graceful degradation. The unknown attributes are:
- Preserved in the AST (for round-tripping).
- Reported as warnings during validation.
- Ignored during resolution and execution.

## Field Declarations

Contracts use nested `<field>` declarations to describe object shapes:

```xml
<skill define="contract" name="tracker-metadata" version="1.0">
  <field name="platform" type="enum" values="github|jira" required />
  <field name="url" type="string">Optional deep link</field>
</skill>
```

Rules:
- `required` may be written as a bare attribute on `<field>` and `<param>`.
- `type="object"` and `type="list"` may contain nested `<field>` children.
- Scalar field types (`string`, `number`, `boolean`, `enum`, `path`, `contract:<name>`) must not contain child fields.
- `type="contract:<name>"` references another registered contract.
