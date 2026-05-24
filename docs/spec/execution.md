# AML Execution Model

## Overview

Execution is Phase 5 of the lifecycle. It takes a resolved AST and produces
the final output by running each bound implementation and injecting results.

## Execution Order: Bottom-Up (Post-Order)

The executor walks the AST in **post-order** (children before parents):

```
Document
├── Text("Analyse this code:")
├── Skill[interface="code-review"]          ← executes SECOND
│   ├── Skill[interface="lint"]             ← executes FIRST
│   │   └── Text("fn main() { ... }")
│   └── Text("Focus on security.")
└── Text("Summary:")
```

In this example:
1. `lint` executes first with scope `"fn main() { ... }"`.
2. Its result replaces the inner `<skill>` tag.
3. `code-review` executes with the enriched scope (lint result + "Focus on security.").

## Execution Policies

Each outer skill may declare how its children are executed. The policy is
determined by the **implementation** (not the invocation syntax).

### Policy: `bottom-up` (default)

Children execute before the parent. The parent receives enriched content.
This is the standard composition pattern.

```
Inner skill executes → result injected → outer skill executes with enriched scope
```

### Policy: `wrapper`

The outer skill's implementation **controls** child execution. Children are
NOT automatically executed. Instead, the implementation receives the raw
(unexecuted) children and decides:
- Whether to execute them.
- In what order.
- With what additional context or constraints.

Use case: sandboxing, validation gates, conditional execution.

```xml
<skill interface="sandbox" policy="wrapper">
  <skill interface="dangerous-tool">do something risky</skill>
</skill>
```

In wrapper mode, `sandbox` receives the raw inner `<skill>` node. It may:
- Validate the inner skill's permissions before executing it.
- Execute it in a restricted context.
- Refuse to execute it entirely (returning an error or alternative result).

### Policy: `sequential`

Children execute in document order, but each child receives the result of
the previous child as additional context. Useful for pipelines.

```xml
<skill interface="pipeline" policy="sequential">
  <skill interface="fetch-data">url</skill>
  <skill interface="transform">transform rules</skill>
  <skill interface="validate">schema</skill>
</skill>
```

## Result Injection

When a skill completes execution, its result replaces the original tag in the
parent's scope.

### SkillResult structure

```rust
struct SkillResult {
    text: String,           // Injected into parent scope
    metadata: Metadata,     // Side-channel data (NOT injected)
    status: ResultStatus,   // success | failed | skipped | partial
}

enum ResultStatus {
    Success,
    Failed { error: String },
    Skipped { reason: String },
    Partial { completed: Vec<String>, failed: Vec<String> },
}
```

### Injection rules

1. The entire `<skill>...</skill>` tag (including its content) is replaced
   by `result.text`.
2. `result.text` is **escaped** — it is treated as literal text. Any `<skill>`
   tags that happen to appear in the result text are NOT re-parsed or executed.
3. `result.metadata` is attached to the execution trace but never appears in
   the output text.
4. If `result.status` is `Failed` and the node has no `retries` remaining,
   the failure propagates per the propagation rules below.
5. If `result.status` is `Skipped`, the tag is replaced by empty string
   (the skill contributes nothing to the scope).

### Whitespace handling

- The injected text preserves the result's own whitespace.
- No additional whitespace is inserted around the injection point.
- If the original tag was on its own line, the result occupies that line.

## Failure Propagation

When a skill fails (status = `Failed` after exhausting retries):

### Propagation modes

| Mode | Behaviour | When to use |
|---|---|---|
| `halt` (default) | Stop execution of the entire document. Return error. | Safety-critical operations |
| `skip` | Replace failed node with empty string. Continue execution. | Optional enrichments |
| `partial` | Mark parent as `Partial`. Continue with available results. | Best-effort composition |

### Configuration

Propagation mode is set per-invocation:

```xml
<skill interface="optional-enrichment" on-failure="skip">
  ...
</skill>
```

If not specified, the default is `halt`.

### Parent interaction

When a child fails with mode `skip`:
- The parent receives the scope with the failed child replaced by empty string.
- The parent's execution proceeds normally.

When a child fails with mode `halt`:
- Execution stops immediately.
- The parent does NOT execute.
- The error propagates up to the document level.

## Retry Behaviour

The `retries` attribute controls automatic retry on failure:

```xml
<skill interface="flaky-api" retries="3">query</skill>
```

### Rules

1. On first failure, re-execute with the same inputs.
2. Each retry is independent — no state is preserved between attempts.
3. After all retries exhausted, the final failure propagates per `on-failure` mode.
4. A `timeout` attribute (if present) applies to EACH attempt individually.
5. The execution trace records all attempts (including intermediate failures).

### What counts as retryable

- Any `Failed` status triggers a retry (if retries remain).
- `Skipped` does NOT trigger a retry (it is intentional).
- `Timeout` triggers a retry (the skill may succeed on the next attempt).

## Execution Context

Each skill execution receives an `ExecutionContext`:

```rust
struct ExecutionContext {
    params: HashMap<String, String>,    // from <param> children
    scope: String,                       // text content (with inner results injected)
    parent_chain: Vec<NodeId>,           // ancestry for audit/logging
    registry: &SkillRegistry,            // for dynamic lookups
    depth: usize,                        // nesting depth (for recursion limits)
    trace: &mut ExecutionTrace,          // accumulates metadata
}
```

## Directive Execution

Directive nodes (`<tool>`, `<session>`, `<agent>`) participate in the execution
tree but behave differently from skill invocations.

### `<tool>` — Scope Directive

The `<tool>` tag constrains which tools are available to descendants. It does
**not** execute anything itself. Children execute normally under the tool
constraint; results flow upward as text.

```xml
<tool name="bash">
  <skill interface="run-tests">test suite</skill>
</tool>
```

The executor processes `<tool>` as pass-through: children execute, results
concatenate. The runtime harness enforces the tool constraint.

### `<session>` — Execution Directive

The `<session>` tag runs its children in a separate execution context. The
result is the concatenated output of the children, injected in place of the
`<session>` tag.

```xml
<session name="backend" isolated="true">
  <skill interface="deploy">backend service</skill>
</session>
```

In isolated mode (default), child execution does not share state with the
parent. In non-isolated mode (`isolated="false"`), state may be shared.

### `<agent>` — Execution Directive

The `<agent>` tag delegates execution to a subagent. The subagent receives
the content as its prompt and returns a result that replaces the tag.

```xml
<agent name="reviewer" model="gpt-4">
  <skill interface="code-review">fn main() {}</skill>
</agent>
```

In `mode="sync"` (default), execution blocks until the agent completes.
In `mode="background"`, the agent runs asynchronously; the tag is replaced
by a placeholder or the result is delivered out-of-band.

### Directive Failure

Directives support the `on-failure` attribute on `<session>` and `<agent>` tags.
This controls how child failures propagate:

- `on-failure="halt"` (default) — any child failure propagates immediately,
  halting the directive and returning an error to the parent.
- `on-failure="skip"` — child failures are silently suppressed; the directive
  produces output only from successful children.
- `on-failure="partial"` — child failures are captured inline as
  `[DIRECTIVE FAILED: <error>]` markers; execution continues.

The `<tool>` tag does **not** support `on-failure` because it is a scope
constraint, not an execution unit — it has nothing to "fail" on its own.

```xml
<session name="resilient" on-failure="skip">
  <skill interface="flaky-service">request</skill>
</session>

<agent name="reviewer" on-failure="partial">
  <skill interface="code-review">code</skill>
</agent>
```

### Tool Constraint Composition

Nested `<tool>` directives compose **monotonically** — inner scopes can only
further restrict, never widen, the available tool set.

**Composition rules:**

| Pattern | Rule | Example |
|---|---|---|
| Allow ∩ Allow | Intersection of allow-lists | `allow="a,b,c"` → `allow="b,c,d"` → effective: `{b,c}` |
| Deny ∪ Deny | Union of deny-lists | `deny="a"` → `deny="b"` → effective deny: `{a,b}` |
| Allow + Deny | Allow first, then deny removes | `allow="a,b,c"` → `deny="b"` → effective: `{a,c}` |
| Deny beats Allow | A tool denied at any ancestor is permanently denied | `deny="bash"` → `allow="bash,grep"` → effective: `{grep}` |

**Validator behaviour:** The validator emits **warnings** (not errors) when an
inner `<tool>` requests tools that are denied or absent at an ancestor level.
These warnings are advisory — the runtime may have legitimate reasons for
seemingly contradictory constraints.

```xml
<!-- Valid but produces a warning: web_search not in ancestor's allow-list -->
<tool allow="grep,view,bash">
  <tool allow="grep,web_search">
    <skill interface="search">query</skill>
  </tool>
</tool>
```

### Depth limit

To prevent infinite recursion (e.g. a skill that emits AML which gets re-parsed),
a hard depth limit is enforced:
- Default: 16 levels of nesting.
- Configurable per-registry.
- Exceeding the limit → `ExecutionError::DepthLimitExceeded`.

## Execution Trace (Observability)

Every execution produces a trace:

```rust
struct ExecutionTrace {
    nodes: Vec<NodeTrace>,
}

struct NodeTrace {
    node_id: NodeId,
    span: SourceSpan,
    resolved_impl: String,
    package: Option<String>,
    version: Option<String>,
    duration: Duration,
    retries: usize,
    status: ResultStatus,
    children: Vec<NodeTrace>,
}
```

This trace enables:
- Debugging nested execution.
- Performance profiling per skill.
- Reproducibility auditing (recording exactly which versions were used).
