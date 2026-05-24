---
name: aml-authoring
description: >
  Use this skill when writing or editing AML (Agent Markup Language) content —
  interface definitions with typed params, returns, reads, writes; implementation
  definitions with DDE node declarations; skill refs (self-closing or wrapping);
  tool constraints; or interface/implementation split patterns. Also activate when
  reviewing AML documents for convention compliance, composing DDE-enforced
  workflows, or when the user asks about AML syntax or placeholder conventions.
---

# AML Authoring Guide

This skill guides agents writing well-formed, idiomatic AML. It covers
the typed interface contract, DDE implementation bridge, and five hard
conventions discovered through production usage.

## Hard conventions

Follow these five rules in every AML document. Violations are authoring
errors, not style preferences.

### 1. Placeholders use `{curly braces}`, never `<angle brackets>`

```
✅  {question}   {date}   {category}
❌  <question>   <date>   <category>
```

Angle-bracket placeholders break syntax highlighting and can be
mis-parsed as unknown AML tags.

### 2. Description prefix: `define/` and `implement/`

When a skill is split into interface + implementation, prefix the
frontmatter `description` so models can match pairs by scanning
descriptions alone:

- Interface: `define/brain-query — Exposes the brain-query interface...`
- Implementation: `implement/brain-query — Internal handler that...`

### 3. Interface/implementation split

DDE-enforced skills split into two skill files:

- **External interface** (`brain-query`): `<skill define="interface">`
  with params, returns, reads, writes, tool constraints, and a
  `<skill ref>` pointing to the handler.
- **Internal handler** (`brain-query-handler`):
  `<skill define="implementation" implements="brain-query">` with the
  DDE flowchart and `<node>` declarations.

The interface references the handler:

```xml
<skill ref="brain-query-handler" role="implementation" />
```

### 4. `<skill ref>` — self-closing vs wrapping

Two forms exist:

- **Self-closing** — flat dependency declaration:
  ```xml
  <skill ref="dde" role="enforcement" />
  ```

- **Wrapping** — scoped governance (the ref owns the enclosed content):
  ```xml
  <skill ref="dde" role="enforcement">
    <!-- mermaid flowchart and <node> declarations here -->
  </skill>
  ```

Use the wrapping form when an enforcement engine (e.g. DDE) governs
the entire implementation body. The scope is structurally explicit:
swapping the execution engine means swapping the wrapping ref.

### 5. `<tool allow>` with usage annotations

Include prose annotations alongside tool constraints to explain *why*
each tool is allowed:

```xml
<tool allow="rename_session,send_session_message,view,edit,bash" />

Tool usage:
- `rename_session` — rename session to kebab-case pattern
- `send_session_message` — reply to caller with answer
- `view` — read wiki pages and index
- `edit` — update brain-questions.md
- `bash` — git commit and push to main
```

---

## Interface definition template

Use this template when authoring `<skill define="interface">`:

```xml
<skill define="interface" name="{skill-name}">
  <!-- Typed input parameters -->
  <param name="{param-name}" type="{type}" required="true">{description}</param>
  <param name="{param-name}" type="enum" values="a|b|c" default="a">{description}</param>
  <param name="{param-name}" type="string" required="false">{description}</param>

  <!-- Typed outputs -->
  <returns name="{output-name}" type="string">{description}</returns>
  <returns name="{output-name}" type="enum" values="success|partial|failure" />

  <!-- File I/O declarations -->
  <reads>{glob-pattern}, {glob-pattern}</reads>
  <writes>{glob-pattern}</writes>

  <!-- Tool constraints -->
  <tool allow="{tool1},{tool2},{tool3}" />

  <!-- Skill references -->
  <skill ref="{handler-name}" role="implementation" />
</skill>
```

**Param types:** `string` (default), `enum`, `number`, `boolean`,
`path`, `list`.

**Rules:**
- `type="enum"` requires `values` (pipe-separated)
- `required="true"` and `default` are mutually exclusive
- `<reads>` and `<writes>` appear at most once each
- Param names must be unique; return names must be unique
- `<tool>` uses `allow` or `deny`, never both

Read `references/grammar.md` for the full grammar and validation rules.

---

## Implementation definition template

Use this template when authoring `<skill define="implementation">`:

```xml
<skill define="implementation" name="{handler-name}" implements="{interface-name}">
  <skill ref="dde" role="enforcement">
    ```mermaid
    flowchart LR
        A[{Step1}] --> B[{Step2}]
        B --> C[{Step3}]
        C --> D[{Step4}]
    ```

    <node name="{Step1}" type="tool">
      <tool use="{tool-name}" />
      {What this step does}
    </node>

    <node name="{Step2}" type="prompt">
      {What the LLM reasons about in this step}
    </node>

    <node name="{Step3}" type="tool">
      <tool use="{tool-name}" />
      {What this step does}
    </node>

    <node name="{Step4}" type="tool">
      <tool use="{tool-name}" />
      {What this step does}
    </node>
  </skill>
</skill>
```

**Node rules:**
- `name` must match the mermaid node label exactly
- `type` is `"tool"` (deterministic invocation) or `"prompt"` (LLM reasoning)
- `<tool use="...">` is optional; use only for tool-type nodes
- Nodes inside a wrapping `<skill ref>` are extracted to the parent
  implementation's node list

---

## Worked example: brain-query skill pair

### External interface (brain-query)

```xml
<skill define="interface" name="brain-query">
  <param name="question" type="string" required="true">The question to answer</param>
  <param name="format" type="enum" values="markdown|comparison-table|slide-deck|chart" default="markdown">Output format</param>
  <param name="caller_session_id" type="string" required="false">Set by brain-responder for reply routing</param>

  <returns name="answer" type="string">Citation-backed answer</returns>
  <returns name="quality" type="enum" values="answered|partial|unanswered" />

  <reads>wiki/index.md, wiki/**/*.md</reads>
  <writes>raw/brain-questions.md</writes>

  <tool allow="rename_session,send_session_message,view,edit,bash" />

  <skill ref="brain-query-handler" role="implementation" />
  <skill ref="dde" role="enforcement" />
</skill>
```

### Internal handler (brain-query-handler)

```xml
<skill define="implementation" name="brain-query-handler" implements="brain-query">
  <skill ref="dde" role="enforcement">
    ```mermaid
    flowchart LR
        A[Rename] --> B[ReadIndex]
        B --> C[Synthesise]
        C --> D[Reply]
    ```

    <node name="Rename" type="tool">
      <tool use="rename_session" />
      Rename this session with pattern `query-{brief-description}`
    </node>

    <node name="ReadIndex" type="prompt">
      Read wiki/index.md, identify candidate pages matching {question}
    </node>

    <node name="Synthesise" type="prompt">
      Drill into candidate pages, synthesise citation-backed answer
      in the requested {format}
    </node>

    <node name="Reply" type="tool">
      <tool use="send_session_message" />
      Send answer back to {caller_session_id} if set, else output directly
    </node>
  </skill>
</skill>
```

---

## Common mistakes

| Mistake | Fix |
|---------|-----|
| `<question>` placeholder in AML | Use `{question}` instead |
| `<tool allow="x" deny="y" />` | Use `allow` OR `deny`, not both |
| `type="enum"` without `values` | Add `values="a\|b\|c"` |
| `required="true"` with `default="x"` | Remove one — they are mutually exclusive |
| Node `name` doesn't match mermaid label | Ensure exact string match |
| `<node type="action">` | Use `"tool"` or `"prompt"` only |
| Missing `implements` on implementation | Add `implements="{interface-name}"` |
| Directives inside definition bodies | Definitions are not executable; remove directives |
