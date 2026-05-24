---
name: aml-usage-guide
description: Teach an agent how to emit valid AML (Agent Markup Language) tags for declarative skill invocation inside prompts, including interface definitions, implementation bindings, scoping, nesting, params, and execution policies.
---

# AML Usage Guide

Use this guide when you need to invoke skills declaratively inside a prompt using AML syntax.

## Core Syntax

AML uses XML-like `<skill>` tags embedded in natural text:

```xml
<skill interface="unit-testing" language="python">
  <param name="target">src/auth.py</param>
  Run coverage analysis on the authentication module.
</skill>
```

## Node Types

There are three mutually exclusive node types:

### 1. Invocation (execute a skill)

```xml
<skill interface="code-review" language="rust">
  Review this function for correctness.
</skill>
```

Required: at least one of `interface`, `impl`, or `name`.

### 2. Interface Definition (declare a capability)

```xml
<skill define="interface" name="code-review">
  Analyse code for bugs, style issues, and security vulnerabilities.
</skill>
```

Required: `define="interface"` and `name`.

### 3. Implementation Definition (bind a concrete skill)

```xml
<skill define="implementation" name="rust-clippy-review" implements="code-review" language="rust">
  Use clippy lints and Rust idioms to review code.
</skill>
```

Required: `define="implementation"`, `name`, and `implements`.

## Attributes Reference

| Attribute | Valid on | Description |
|-----------|----------|-------------|
| `interface` | Invocation | Abstract capability to resolve |
| `impl` | Invocation | Force a specific implementation |
| `name` | All | Identifier for definitions; direct lookup for invocations |
| `define` | Definition | `"interface"` or `"implementation"` |
| `implements` | ImplDef | Which interface this implements |
| `language` | Invocation, ImplDef | Language hint for resolution |
| `framework` | Invocation, ImplDef | Framework hint for resolution |
| `retries` | Invocation | Max retry attempts (default: 0) |
| `on-failure` | Invocation, SessionDirective, AgentDirective | `"halt"` (default), `"skip"`, `"partial"` |
| `policy` | Invocation | `"bottom-up"` (default), `"wrapper"`, `"sequential"` |

## Parameters

Use `<param>` tags inside a skill for structured inputs:

```xml
<skill interface="file-search">
  <param name="pattern">*.rs</param>
  <param name="directory">src/</param>
  Find all Rust source files.
</skill>
```

## Scoping and Nesting

Content inside a `<skill>` tag is its **scope** — what the skill operates on.

Skills nest. Inner skills execute first (bottom-up), and their results replace the tags in the outer scope:

```xml
<skill interface="summarise">
  <skill interface="fetch-url">
    <param name="url">https://example.com/article</param>
  </skill>
</skill>
```

Here `fetch-url` runs first, its output becomes the scope for `summarise`.

## Execution Policies

- **bottom-up** (default): children execute first, results flow up
- **wrapper**: outer skill receives raw children (controls their execution)
- **sequential**: children execute in order, each seeing prior results

```xml
<skill interface="sandbox" policy="wrapper">
  <skill interface="dangerous-tool">do something risky</skill>
</skill>
```

## Document Root (Optional)

Wrap content in an `<aml>` tag to declare the version:

```xml
<aml version="0.1">
  <skill interface="analyse">scan the codebase</skill>
</aml>
```

The `<aml>` wrapper is **optional**. When omitted, tags are extracted from
arbitrary text (fragment mode). When present:
- `version` is required
- Only whitespace/comments allowed outside the wrapper
- Cannot be nested

## Directives

AML has three directive tags that control *how* content is executed:

### `<tool>` — Tool Constraints

Restrict which tools are available within a scope:

```xml
<tool allow="bash,grep">
  <skill interface="search">find files</skill>
</tool>
```

Attributes: `name` (single tool), `allow` (whitelist), `deny` (blacklist).
`allow` and `deny` are mutually exclusive. At least one must be present.

### `<session>` — Session Isolation

Execute content in a separate session:

```xml
<session name="backend" isolated="true">
  <skill interface="deploy">deploy service</skill>
</session>
```

Attributes: `name` (optional), `isolated` (optional, default "true"),
`on-failure` (optional: "halt"/"skip"/"partial").

### `<agent>` — Subagent Delegation

Delegate execution to a subagent:

```xml
<agent name="reviewer" model="gpt-4" mode="sync">
  <skill interface="code-review">fn main() {}</skill>
</agent>
```

Attributes: `name` (required), `model` (optional), `mode` (optional: "sync"/"background"),
`on-failure` (optional: "halt"/"skip"/"partial").

### Directive Nesting

Directives nest freely with each other and with `<skill>`:

```xml
<agent name="dev">
  <tool name="bash">
    <skill interface="test">run tests</skill>
  </tool>
</agent>
```

## Failure Handling

```xml
<skill interface="flaky-service" retries="3" on-failure="skip">
  Call the service.
</skill>

<!-- on-failure also works on session and agent directives -->
<session name="best-effort" on-failure="skip">
  <skill interface="optional">may fail</skill>
</session>

<agent name="reviewer" on-failure="partial">
  <skill interface="review">code</skill>
</agent>
```

- `halt`: stop execution on failure (default)
- `skip`: silently skip failed skill/directive
- `partial`: inject error marker and continue

## Tool Constraint Composition

Nested `<tool>` directives compose monotonically (inner can only restrict):

```xml
<tool allow="grep,view,bash">
  <tool allow="grep,view">
    <!-- Only grep and view here — bash was narrowed out -->
  </tool>
</tool>
```

- Allow ∩ Allow: intersection
- Deny ∪ Deny: union
- Deny beats Allow: a tool denied at any ancestor is permanently denied

> **Warning:** `bash` is a superuser tool — it can bypass file-level
> tool constraints via shell commands. Prefer `allow="grep,view"` for
> read-only contexts.

## Rules

1. **Never nest definitions inside invocations** — definitions are top-level only
2. **One resolution target per invocation** — use `interface` OR `impl` OR `name`
3. **Results are escaped** — skill output is never re-parsed as AML
4. **Content is the scope** — everything between open/close tags is what the skill sees
5. **Definitions don't execute** — they only register capabilities
6. **Directives don't contain definitions** — definitions inside directives are invalid
7. **Definitions don't contain directives** — directive tags inside definition bodies are invalid
8. **Tool constraints only narrow** — `<tool>` cannot expand access beyond host policy
9. **Tool composition is monotonic** — nested `<tool>` tags intersect allows and union denies

## When NOT to use AML

- Simple function calls with no scoping → use tool_call syntax instead
- One-shot queries with no composition → plain text is fine
- Highly dynamic dispatch → use agentic reasoning, then emit AML for the chosen skill
