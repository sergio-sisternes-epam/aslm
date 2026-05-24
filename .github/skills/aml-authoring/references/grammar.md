# AML Grammar Reference

Comprehensive grammar rules and validation for AML (Agent Markup Language).
Load this reference when you need syntax detail beyond what the main
SKILL.md body covers.

Source of truth: `docs/spec/grammar.ebnf` in the ASLM repository.

---

## Document structure

```ebnf
Document  ::= (Text | SkillNode | DirectiveNode)*
SkillNode ::= Invocation | InterfaceDef | ImplementationDef
```

An AML document is arbitrary text interspersed with skill nodes and
directive nodes. The three skill-node forms are mutually exclusive —
determined by which attributes appear on the `<skill>` tag.

---

## Skill node disambiguation

| Attributes present | Form |
|--------------------|------|
| `define="interface"` + `name` | InterfaceDef |
| `define="implementation"` + `name` + `implements` | ImplementationDef |
| `ref` (no define/interface/impl/name) | SkillRefDecl |
| `interface` and/or `impl` and/or `name` (no define/ref) | Invocation |

---

## Interface definition

```ebnf
InterfaceDef ::= '<skill' 'define="interface"' 'name="..."' '>'
                   InterfaceBody
                 '</skill>'

InterfaceBody ::= (Text | ParamDecl | ReturnsDecl | ReadsDecl
                 | WritesDecl | SkillRefDecl | ToolConstraintDecl)*
```

### ParamDecl (typed input parameter)

```ebnf
ParamDecl ::= '<param' ParamDeclAttrs '>' Text '</param>'
            | '<param' ParamDeclAttrs '/>'

ParamDeclAttrs ::= name type? required? default? values?
```

| Attribute | Required | Values | Notes |
|-----------|----------|--------|-------|
| `name` | yes | any string | Must be unique within the interface |
| `type` | no | `string` `enum` `number` `boolean` `path` `list` | Defaults to `string` |
| `required` | no | `"true"` `"false"` | Defaults to `false` |
| `default` | no | any string | Mutually exclusive with `required="true"` |
| `values` | conditional | pipe-separated, e.g. `"a\|b\|c"` | Required when `type="enum"`; forbidden otherwise |

### ReturnsDecl (typed output)

```ebnf
ReturnsDecl ::= '<returns' ReturnsDeclAttrs '>' Text '</returns>'
              | '<returns' ReturnsDeclAttrs '/>'

ReturnsDeclAttrs ::= name type? values?
```

| Attribute | Required | Values | Notes |
|-----------|----------|--------|-------|
| `name` | yes | any string | Must be unique within the interface |
| `type` | no | same as ParamDecl types | Defaults to `string` |
| `values` | conditional | pipe-separated | Required when `type="enum"` |

### ReadsDecl / WritesDecl (file I/O)

```ebnf
ReadsDecl  ::= '<reads>' Text '</reads>'
WritesDecl ::= '<writes>' Text '</writes>'
```

Content is comma-separated glob patterns. Each may appear at most once
per interface.

### ToolConstraintDecl

```ebnf
ToolConstraintDecl ::= '<tool' (allow | deny) '/>'
```

- `allow` and `deny` are mutually exclusive
- Value is a comma-separated list of tool names
- Self-closing only (inside interface bodies)

### SkillRefDecl

```ebnf
SkillRefDecl ::= '<skill' 'ref="..."' role? '/>'
               | '<skill' 'ref="..."' role? '>' SkillRefBody '</skill>'

SkillRefBody ::= (Text | NodeDecl)*
```

- Self-closing: flat dependency declaration
- Wrapping: scoped governance — nodes inside are extracted to the
  parent implementation's node list

---

## Implementation definition

```ebnf
ImplementationDef ::= '<skill' 'define="implementation"'
                        'name="..."' 'implements="..."' '>'
                        ImplementationBody
                      '</skill>'

ImplementationBody ::= (Text | NodeDecl | SkillRefDecl)*
```

### NodeDecl (DDE step)

```ebnf
NodeDecl ::= '<node' 'name="..."' 'type="..."' '>' NodeBody '</node>'
NodeBody ::= (Text | ToolUseDecl)*

ToolUseDecl ::= '<tool' 'use="..."' '/>'
```

| Attribute | Required | Values | Notes |
|-----------|----------|--------|-------|
| `name` | yes | any string | Must match the mermaid node label |
| `type` | yes | `"tool"` `"prompt"` | Validated at parse time |

---

## Invocation

```ebnf
Invocation ::= '<skill' InvocationAttrs '>' InvocationBody '</skill>'
             | '<skill' InvocationAttrs '/>'

InvocationAttrs ::= (interface | impl | name) language? framework?
                    retries? timeout?

InvocationBody ::= (Text | Param | SkillNode | DirectiveNode)*
```

At least one of `interface`, `impl`, or `name` must be present.
`name` is mutually exclusive with `interface`/`impl`.

---

## Directive nodes

### Tool directive

```ebnf
ToolDirective ::= '<tool' (name | allow | deny)+ '>' Body '</tool>'
                | '<tool' (name | allow | deny)+ '/>'
```

Nested `<tool>` directives compose monotonically:
- Allow-lists intersect: inner ∩ outer
- Deny-lists union: inner ∪ outer
- Deny always beats allow

### Session directive

```ebnf
SessionDirective ::= '<session' name? isolated? '>' Body '</session>'
```

### Agent directive

```ebnf
AgentDirective ::= '<agent' name model? mode? '>' Body '</agent>'
```

---

## Invalid attribute combinations

These must be rejected:

- `define` + `interface` (definition is not invocation)
- `define` + `impl` (definition is not invocation)
- `define` + `retries` or `timeout` (definitions don't execute)
- `name` + `interface` or `impl` (mutually exclusive resolution)
- `define="interface"` without `name`
- `define="implementation"` without `name` or `implements`
- `<tool>` with both `allow` and `deny`
- `<tool>` without any of: `name`, `allow`, `deny`
- `<agent>` without `name`
- Directives inside definition bodies

---

## Lexical rules

- **Quoting:** double (`"..."`) or single (`'...'`) quotes for attribute values
- **Escaping:** `&lt;` `&gt;` `&amp;` `&quot;` `&apos;` — no CDATA
- **Whitespace:** at least one space between attributes; preserved in Text
- **Comments:** `<!-- ... -->` — stripped during parsing
