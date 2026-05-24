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
| `on-failure` | Invocation | `"halt"` (default), `"skip"`, `"partial"` |
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

## Failure Handling

```xml
<skill interface="flaky-service" retries="3" on-failure="skip">
  Call the service.
</skill>
```

- `halt`: stop execution on failure (default)
- `skip`: silently skip failed skill
- `partial`: inject error marker and continue

## Rules

1. **Never nest definitions inside invocations** — definitions are top-level only
2. **One resolution target per invocation** — use `interface` OR `impl` OR `name`
3. **Results are escaped** — skill output is never re-parsed as AML
4. **Content is the scope** — everything between open/close tags is what the skill sees
5. **Definitions don't execute** — they only register capabilities

## When NOT to use AML

- Simple function calls with no scoping → use tool_call syntax instead
- One-shot queries with no composition → plain text is fine
- Highly dynamic dispatch → use agentic reasoning, then emit AML for the chosen skill
