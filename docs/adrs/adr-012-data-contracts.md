# ADR-012: Data-Shape Contracts

## Status
Accepted

## Context

The APM marketplace L0 loop primitives (specification-contract, implementation-result-contract, quality-gate, context-contract) define **data-shape contracts** at loop seams -- field schemas for data that flows between Design, Implementation, and Quality stages. Today these are markdown field tables: human-readable but not machine-validatable, not referenceable by AML interfaces, and not composable.

AML currently has `define="interface"` for behavioural capabilities with typed `<param>` and `<returns>`, but **no construct for pure data shapes**: named, reusable, versioned field schemas that interfaces can reference as parameter or return types.

This gap blocks:
- **Interface type contracts**: a loop skill cannot declare "I consume specification-contract" as a typed param.
- **Mechanical validation**: no runtime check that an implementation's emitted data satisfies a contract's required fields.
- **Contract reuse**: shared schemas (e.g. tracker metadata, stack component descriptors) are copy-pasted across multiple contracts instead of composed via reference.

Token efficiency is the primary design constraint. Loop contracts live in prompt context permanently. A baseline markdown table row:
```
| work_item_id | string | yes | Stable tracker ID |
```
is **13 tokens**. Field declarations must stay within **1.5x** of that (ideally ~20 tokens with description).

## Decision

Add a `define="contract"` value for `<skill>` tags, with `<field>` children mirroring `<param>` structure:

```xml
<skill define="contract" name="specification-contract" version="1.0">
  <field name="work_item_id" type="string" required>Stable tracker ID</field>
  <field name="tracker" type="object">
    <field name="platform" type="enum" values="github-issues|jira|ado-boards" required />
    <field name="ref" type="string" required />
    <field name="url" type="string" required />
  </field>
  <field name="implementation_id" type="string" required>Unique impl identifier</field>
  <field name="title" type="string" required>Human-readable summary</field>
  <field name="description" type="string" required>Detailed scope statement</field>
  <field name="acceptance_criteria" type="list" required>Verifiable success conditions</field>
  <field name="complexity" type="enum" values="Patch|Feature|Epic" required />
  <field name="scenario_profile" type="enum" values="Greenfield|Brownfield|Modernisation" required />
  <field name="stack_components" type="list" required>
    <field name="component_id" type="string" required />
    <field name="kind" type="enum" values="app|service|library|infra|docs" required />
    <field name="language" type="string">null means detect at runtime</field>
    <field name="path_hints" type="list" />
  </field>
  <field name="applicable_standards" type="list">Standards to apply</field>
  <field name="source_traceability" type="list" required>Origin issue/epic links</field>
  <field name="linked_docs" type="list">Supporting documentation URLs</field>
  <field name="constraints" type="list">Explicit limitations</field>
</skill>
```

**Syntax details:**
- **Contract definition**: `<skill define="contract" name="..." version="...">` reuses the skill tag with a new define value.
- **Field declaration**: `<field name="..." type="..." />` or `<field name="..." type="...">description</field>`.
- **Required flag**: bare `required` attribute (boolean presence flag, no value). **Fields default to optional** (required=false). Valued form `required="true|false"` also accepted for consistency with `<param>`.
- **Types**: `string`, `number`, `boolean`, `enum`, `list`, `object`, `path`, or `contract:<name>` for referencing another contract.
- **Enum values**: `values="a|b|c"` pipe-separated (reuses `<param>` syntax).
- **Default value**: `default="..."` attribute.
- **Description**: text content of the `<field>` tag (not an attribute).
- **Nested objects**: `type="object"` with nested `<field>` children (supports arbitrary depth).
- **List item shapes**: `type="list"` with nested `<field>` children declaring the item shape (object fields). Scalar list types (list of strings, etc.) have no children; item type goes in description.
- **Contract references**: `type="contract:tracker-metadata"` declares this field's shape is defined by another contract.
- **Versioning**: optional `version="1.0"` attribute on the contract (freeform semver-ish; no resolution semantics in v0.3.0).
- **Inheritance**: optional `extends="parent-contract"` attribute to inherit fields from another contract (validation only -- no automatic merging in v0.3.0).

**Referencing contracts from interfaces:**
```xml
<skill define="interface" name="li/plan">
  <param name="spec" type="contract:specification-contract" required>Work item to plan</param>
  <returns name="result" type="contract:implementation-result-contract">Planned implementation</returns>
</skill>
```

The `type="contract:<name>"` syntax binds a param/return to a named contract. The validator checks that the referenced contract is registered.

**Token efficiency measurement:**
```
Markdown baseline:     | work_item_id | string | yes | Stable tracker ID |  =  13 tokens
Field with desc:       <field name="work_item_id" type="string" required>Stable tracker ID</field>  =  21 tokens  (1.62x)
Field without desc:    <field name="work_item_id" type="string" required />  =  14 tokens  (1.08x)
```

Using bare `required` (boolean flag) instead of `required="true"` saves 6 tokens per required field. Text content for descriptions (instead of `desc="..."` attribute) keeps the syntax consistent with `<param>` and avoids XML escaping issues.

## Rationale

**Why `define="contract"` on `<skill>` instead of a new top-level tag?**
- Reuses existing `<skill>` tag parsing and `define=` dispatch mechanism.
- Avoids expanding the grammar's tag vocabulary (parser only recognises: skill, param, tool, session, agent).
- Contracts are definitions (non-executable) like interfaces/implementations -- keeping them under `<skill>` makes the definition taxonomy consistent.

**Why `<field>` instead of `<param>`?**
- Semantic clarity: params are invocation arguments; fields are data-shape components.
- Contracts describe data structures, not operation signatures.
- Allows future divergence (e.g. field-specific validation rules like regex, range).

**Why nested `<field>` for objects instead of dotted names?**
- Dotted names (`tracker.platform`, `tracker.ref`) flatten the namespace and require parsing logic to reconstruct hierarchy.
- Nested `<field>` mirrors the natural JSON/YAML structure that instances will take.
- Supports arbitrary nesting depth without special delimiter handling.
- Same pattern for list item shapes: `type="list"` with nested children declares object item structure.

**Why bare `required` flag instead of `required="true"`?**
- Token efficiency: bare `required` saves 6 tokens per field vs `required="true"`.
- Default-false semantics: fields are optional by default; bare flag marks the required minority.
- Reuses existing attribute name: `required` already exists on `<param>`, no new vocabulary.
- Precedent: HTML boolean attributes (`disabled`, `checked`) work this way.

**Why `type="contract:<name>"` instead of `ref="<name>"`?**
- Keeps `type` as the single attribute for shape specification.
- `ref="..."` could be confused with skill references (`<skill ref="...">`).
- The `contract:` prefix clearly distinguishes contract references from primitive types.

**Why text content for descriptions instead of `desc="..."`?**
- Consistency with `<param>` and `<returns>` tags (they use text content).
- Avoids XML escaping issues in descriptions (quotes, angle brackets).
- Marginally better token efficiency (no attribute name overhead).

**Why `extends=` for contract inheritance?**
- Mirrors the `extends=` attribute already used for interface inheritance (ADR-011).
- Enables DRY contracts: base contracts with common fields (e.g. `base-work-item`) extended by specialised contracts.
- Validation-only in v0.3.0 (no automatic field merging) to avoid complexity.

## Validation Semantics

**Registry validation (new errors):**
- **ContractReferenceUnknown**: `type="contract:X"` where X is not a registered contract.
- **ContractExtendsUnknown**: `extends="X"` where X is not a registered contract.
- **ContractExtendsCycle**: contract inheritance graph contains a cycle (includes self-extension).
- **DuplicateFieldNames**: contract body contains multiple `<field>` elements with the same name (at the same nesting level).
- **EnumWithoutValues**: `type="enum"` field without a `values="..."` attribute.
- **InvalidFieldType**: `type` attribute value not in the allowed set.
- **DirectiveInContract**: `<tool>`, `<session>`, or `<agent>` directive inside a contract body (contracts are pure data).
- **ChildrenOnScalarField**: `<field>` children on scalar types (string, number, boolean, enum, path).
- **InvalidBareAttribute**: bare attribute other than `required` on `<field>` or `<param>` (e.g. `<field type />` is an error).

**Execution semantics:**
- Contracts are definitions (Rule 5): executor returns empty string, same as interfaces/implementations.

## Out of Scope for v0.3.0

State explicitly as future work:
- **Instance validation**: checking a JSON/YAML document against a contract schema at runtime.
- **Code generation**: producing TypeScript interfaces, Pydantic models, or JSON Schema from contracts.
- **Contract merging**: automatic field inheritance when `extends=` is present (registry stores the attribute but does not merge).
- **Transitive contract references**: validating that nested contracts' required fields are satisfied.
- **Scalar list types**: `of="string"` attribute for list-of-primitives. In v0.3.0, scalar list item types go in description text.

## Trade-offs

**Bare boolean attributes (HTML-style, not XML well-formed):**
The bare `required` attribute syntax (`<field ... required />`) is HTML-style, not standard XML. Standard XML requires all attributes to have quoted values. AML is XML-LIKE (ADR-001) with a hand-rolled parser, so this is acceptable and buys significant token savings. However, this choice forecloses any future "feed AML to a strict XML parser" path. We accept this trade-off because:
- Token efficiency is the primary design goal for loop contracts.
- AML is a domain-specific language, not generic XML.
- The hand-rolled parser gives us full control over lexical forms.
- `required` is the only bare attribute in v0.3.0; the validator rejects bare forms of other attributes.

## Migration Path

No breaking changes. This is a pure addition:
- Existing `<skill define="interface">` and `<skill define="implementation">` nodes are unchanged.
- The `define="contract"` value is new; older parsers that do not recognise it will emit an "unknown define value" error, which is correct behaviour.

## Consequences

**AST changes (`crates/aml-core/src/ast.rs`):**
- Add `ContractDefinition` variant to `NodeKind`:
  ```rust
  ContractDefinition {
      name: String,
      extends: Option<String>,
      version: Option<String>,
      description: Option<String>,
      fields: Vec<FieldDecl>,
  }
  ```
- Add `FieldDecl` struct (mirrors `ParamDecl`):
  ```rust
  pub struct FieldDecl {
      pub name: String,
      pub field_type: Option<String>,
      pub required: bool,  // presence of `req` attribute
      pub default: Option<String>,
      pub values: Option<String>,  // for enum types
      pub children: Vec<FieldDecl>,  // for object types
      pub description: Option<String>,
      pub span: Span,
  }
  ```
- Update `Node::is_definition()` to include `ContractDefinition`.

**Parser changes (`crates/aml-core/src/parser.rs`):**
- Extend `parse_skill_tag` to dispatch on `define="contract"`.
- Add `parse_contract_body` function handling `<field>` children (mirrors `parse_interface_body`).
- Parse `<field>` tags: name, type, req flag, default, values, nested children, text content.

**Registry changes (`crates/aml-core/src/registry.rs`):**
- Add `contracts: HashMap<String, ContractEntry>` to `SkillRegistry`.
- Add `ContractEntry { name, extends, version, fields }`.
- Extend `register_from_node_kind` to handle `ContractDefinition`.
- Extend `validate()` to check contract references and extends cycles.

**Validator changes (`crates/aml-core/src/validator.rs`):**
- Add `validate_contract_definition` function checking duplicate field names, enum fields without values, unknown types.
- Extend `validate_kind` to dispatch `ContractDefinition` nodes.

**Executor changes (`crates/aml-core/src/executor.rs`):**
- Extend definition match arm to include `ContractDefinition` (returns empty string).

**Python bindings (`crates/aml-python/src/lib.rs`):**
- Extend `definitions()` to include contracts.
- Add `contracts()` accessor returning `{ name: { extends, version, fields: [...] } }`.
- Add `get_contract(name)` accessor.

**Documentation updates:**
- `docs/spec/grammar.ebnf`: add contract and field production rules.
- `.apm/skills/aml-usage-guide/SKILL.md`: add compact contracts section (keep it tight -- this ships to agents).
- `CHANGELOG.md`: document the feature.

**Conformance fixtures:**
- Positive: full specification-contract (all 13 fields incl. nested tracker object and stack_components list-of-objects), bare `required` in self-closing and text-content forms, contract references, extends, version.
- Negative: duplicate field names, enum without values, unknown contract reference, extends cycle, directive in contract body, invalid field type, children on scalar field, bare attribute other than `required`.
