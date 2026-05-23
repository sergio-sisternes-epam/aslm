# SML Security Model

## Overview

SML invokes executable skills, which may have side effects on real systems.
A security model is essential to prevent unintended or malicious behaviour.
SML adopts a **deny-by-default** execution policy with explicit capability grants.

## Trust Boundaries

### Input trust levels

| Level | Source | Treatment |
|---|---|---|
| **Trusted** | System prompt, pre-verified templates | Execute all skills without additional checks |
| **Semi-trusted** | User-provided prompts in controlled environments | Execute skills within declared capability set |
| **Untrusted** | External input, user-generated content, RAG results | Parse only; do NOT execute without explicit approval |

The trust level is set by the **caller** (the system integrating SML), not by
the document itself. A document cannot escalate its own trust level.

## Capability Declarations

Each implementation declares its capabilities — what it is allowed to do:

```xml
<skill define="implementation"
  name="file-reader"
  implements="data-source"
  capabilities="fs:read">
  Reads files from the local filesystem.
</skill>
```

### Capability taxonomy

| Capability | Scope | Description |
|---|---|---|
| `pure` | No side effects | Computation only; no I/O |
| `fs:read` | Filesystem read | Read files, list directories |
| `fs:write` | Filesystem write | Create, modify, delete files |
| `net:read` | Network read | HTTP GET, DNS lookups |
| `net:write` | Network write | HTTP POST/PUT/DELETE, send data |
| `exec` | Process execution | Spawn subprocesses, run commands |
| `env:read` | Environment read | Read environment variables, system info |
| `env:write` | Environment write | Modify environment, system state |

### Capability composition

Capabilities are **additive** — an implementation with `fs:read,net:read` can
read files and make network requests but cannot write to either.

## Execution Policy

### Deny-by-default

Skills are NOT executable unless:
1. The document's trust level permits execution, AND
2. The skill's capabilities are within the allowed set.

### Allowed capability set

The caller configures which capabilities are permitted:

```rust
let policy = ExecutionPolicy::new()
    .allow(Capability::Pure)
    .allow(Capability::FsRead)
    .deny(Capability::Exec);

execute(document, registry, policy)?;
```

### Capability checking

Before executing any skill, the executor checks:
```
if skill.capabilities.is_subset_of(policy.allowed):
    execute(skill)
else:
    denied_caps = skill.capabilities - policy.allowed
    return Error::CapabilityDenied(skill, denied_caps)
```

## User Confirmation Gates

For semi-trusted input, certain capabilities may require user confirmation:

```rust
let policy = ExecutionPolicy::new()
    .allow(Capability::Pure)
    .allow(Capability::FsRead)
    .confirm(Capability::FsWrite)      // ask user before executing
    .confirm(Capability::Exec)
    .deny(Capability::NetWrite);
```

When a skill requires a `confirm`-level capability:
1. Execution pauses.
2. The runtime presents the skill name, description, and requested capabilities.
3. The user approves or denies.
4. Execution continues (or halts with `CapabilityDenied`).

## Scope Isolation

Each skill execution is **scope-isolated**:
- A skill can only access data passed to it via `params` and `scope`.
- Skills cannot read other skills' internal state.
- Skills cannot modify the AST or registry.
- The execution context is read-only (except the trace accumulator).

## Injection Prevention

### Re-parsing attacks

Injected results are **never re-parsed**. If a skill returns text containing
`<skill>` tags, those tags are treated as literal text. This prevents:
- A compromised skill from escalating privileges via injection.
- Untrusted data (e.g. from an API response) from triggering execution.

### Attribute injection

Attribute values are validated against their declared type during parsing.
A value like `name="foo" impl="evil"` in a single attribute is caught by the
parser (the attribute value is the entire quoted string, not parsed for spaces).

## Registry Trust

### Package provenance

The `SkillRegistry` tracks where each implementation was loaded from:
- Package name and version.
- Installation source (APM registry, local path, etc.).
- Hash of the implementation content.

### Trusted registries

The caller may configure a list of trusted package sources:
```rust
registry.trust_source("apm:microsoft/*");
registry.trust_source("apm:my-org/*");
registry.deny_source("apm:*");  // deny everything else
```

Skills from untrusted sources are treated as if they have no capabilities
(effectively `pure` only).

## Audit Trail

Every execution produces an audit record:
- Which skills were executed.
- What capabilities were exercised.
- Whether user confirmation was requested (and the response).
- The trust level of the input document.
- The allowed capability set of the policy.

This enables post-hoc security review and compliance auditing.

## Threat Model Summary

| Threat | Mitigation |
|---|---|
| Malicious SML in user input | Trust levels + deny-by-default |
| Skill escalates privileges | Capability declarations + pre-execution check |
| Injected results trigger execution | Results are never re-parsed |
| Compromised package | Registry trust + package provenance |
| Infinite recursion DoS | Depth limit (default 16) |
| Resource exhaustion | Timeout attribute + global execution budget |
