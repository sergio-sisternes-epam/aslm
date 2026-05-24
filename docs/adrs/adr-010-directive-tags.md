# ADR-010: Add Directive Tags (`<tool>`, `<session>`, `<agent>`)

## Status
Accepted

## Context
AML currently only supports `<skill>` (invocation/definition) and `<param>` (structured input) tags. Agents need to express *how* content should be executed — not just *what* skill to invoke. Three recurring patterns have no AML representation:

1. **Tool constraints** — restricting which tools an agent may use within a scope.
2. **Session isolation** — running part of a prompt in a separate session with independent state.
3. **Subagent delegation** — handing a section to a specialised subagent.

These are *execution directives* — they instruct the runtime about execution environment, not about skill selection.

## Decision
Add three new top-level XML-like tags as **directive nodes**:

- `<tool>` — scope directive that constrains tool access for descendants.
- `<session>` — execution directive that runs children in a separate session.
- `<agent>` — execution directive that delegates children to a subagent.

These are peers of `<skill>` in the grammar. They support full mutual nesting with each other and with `<skill>`.

In the AST they are represented as `Node::Directive { kind: DirectiveKind, ... }` with a typed `DirectiveKind` enum, keeping traversal and execution generic while preserving strong typing per directive.

## Rationale
- **Separate tags** (not attributes on `<skill>`) keep concerns cleanly separated: skills declare *what*, directives declare *how*.
- **Full nesting** enables natural composition: `<agent><tool><skill>...</skill></tool></agent>`.
- **Typed enum** avoids both variant duplication (three separate AST nodes) and untyped generality (string-keyed directive bags).
- **Parser generalisation** around a `TagName` set makes future tag additions cheaper and avoids the growing prefix-check problem.

## Consequences
- The parser must recognise five tag names (`skill`, `param`, `tool`, `session`, `agent`) and handle matching close tags for each.
- The validator must enforce directive-specific attribute rules (e.g. `allow`/`deny` mutual exclusivity).
- The executor must handle `Node::Directive` — initially as pass-through (children execute, results flow up), with runtime-specific behaviour deferred to the harness.
- The lifecycle gains a conceptual Authorise phase for directive constraints, documented in the spec but implemented by the runtime.
- Directives inside definition bodies (`define="interface"` or `define="implementation"`) are invalid, extending the existing rule that forbids invocations in definitions.
