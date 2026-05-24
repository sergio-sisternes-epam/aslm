# AML Execution Lifecycle

## Overview

AML processing follows a strict 6-phase pipeline. Each phase has well-defined
inputs, outputs, and error conditions. Phases execute sequentially; no phase
may be skipped.

```
Input Prompt → Parse → Validate → Register Definitions → Resolve → Authorise → Execute → Output
```

## Phase 1: Parse

**Input**: Raw text string (the prompt or document containing AML tags).

**Output**: Unvalidated AST (`Document` node containing `SkillNode`, `DirectiveNode`, and `Text` nodes).

**Behaviour**:
- The parser tokenises AML tags within arbitrary prompt text.
- Recognised tags: `<skill>`, `<param>`, `<tool>`, `<session>`, `<agent>`.
- Non-AML text is preserved as `Text` nodes.
- Nested tags produce nested children.
- `<param>` tags within a `<skill>` are attached to the parent node.
- XML comments (`<!-- ... -->`) are stripped.
- Escape sequences (`&lt;`, `&gt;`, `&amp;`, `&quot;`, `&apos;`) are decoded.

**Error conditions**:
- Unclosed `<skill>` tag → `ParseError::UnclosedTag { span }`
- Mismatched closing tag → `ParseError::MismatchedClose { open_span, close_span }`
- Malformed attribute → `ParseError::MalformedAttribute { span, detail }`
- All errors include source spans for precise error reporting.

**Tolerance policy**: The parser is **embedded-tolerant** — it operates within
arbitrary prompt text. Any `<` that does not begin a recognised AML tag is
treated as literal text. The parser does NOT attempt to parse non-AML XML/HTML.

## Phase 2: Validate

**Input**: Unvalidated AST from Phase 1.

**Output**: Validated AST (same structure, all nodes confirmed well-formed).

**Behaviour**:
- Check attribute mutual exclusivity (see grammar.ebnf § Attribute Validation Rules).
- Verify required attributes per node type:
  - Invocation: at least one of `interface`, `impl`, `name`.
  - InterfaceDef: `define="interface"` + `name`.
  - ImplementationDef: `define="implementation"` + `name` + `implements`.
  - ToolDirective: at least one of `name`, `allow`, `deny`. `allow` and `deny` are mutually exclusive.
  - AgentDirective: `name` is required.
- Verify `retries` parses as unsigned integer.
- Verify `timeout` parses as duration string.
- Verify directives do not appear inside definition bodies.
- Flag unknown attributes as warnings (not errors) for forward compatibility.

**Error conditions**:
- `ValidationError::ConflictingAttributes { node_span, attrs }`
- `ValidationError::MissingRequiredAttribute { node_span, attr_name }`
- `ValidationError::InvalidAttributeValue { node_span, attr_name, value, expected }`

## Phase 3: Register Definitions

**Input**: Validated AST.

**Output**: `SkillRegistry` populated with interface and implementation definitions.
The AST is also annotated: definition nodes are marked as `non-executable`.

**Behaviour**:
- Walk the AST, extracting all `InterfaceDef` and `ImplementationDef` nodes.
- For each `InterfaceDef`:
  - Register in the registry under its `name`.
  - Store the body text as the interface description.
- For each `ImplementationDef`:
  - Verify the `implements` interface exists (or defer to runtime if external).
  - Register under its `name` with metadata: `implements`, `language`, `framework`.
- Definition nodes remain in the AST but are marked `executable: false`.

**Error conditions**:
- `RegistrationError::DuplicateInterface { name, first_span, second_span }`
- `RegistrationError::DuplicateImplementation { name, first_span, second_span }`
- `RegistrationError::OrphanImplementation { name, implements, span }` (warning, not error)

## Phase 4: Resolve

**Input**: Validated AST + populated `SkillRegistry`.

**Output**: Resolved AST where each `Invocation` node has a concrete implementation binding.

**Behaviour**:
For each `Invocation` node, resolve to a concrete implementation:

1. If `impl` is present:
   - Look up the named implementation in the registry.
   - If `interface` is also present, validate that `impl` implements `interface`.
   - Bind the node to the found implementation.

2. If `interface` is present (without `impl`):
   - Find all implementations that declare `implements = <interface>`.
   - Filter by hint attributes (`language`, `framework`) if provided.
   - If exactly one remains → bind.
   - If multiple remain → `ResolutionError::Ambiguous`.
   - If none remain → `ResolutionError::NoImplementation`.

3. If only `name` is present:
   - Look up directly by name (implementation first, then interface).
   - If found → bind.
   - If not found → `ResolutionError::NotFound`.

**Resolution is deterministic**: given the same registry state and the same
document, resolution always produces the same binding (or the same error).

**Error conditions**:
- `ResolutionError::NotFound { name, span }`
- `ResolutionError::Ambiguous { interface, candidates, span }`
- `ResolutionError::NoImplementation { interface, span }`
- `ResolutionError::ImplInterfaceMismatch { impl_name, expected_interface, actual_interface, span }`

## Phase 4b: Authorise

**Input**: Resolved AST with directive nodes.

**Output**: Authorised AST (all directive constraints validated against host policy).

**Behaviour**:
- For each `ToolDirective`: verify requested tools are permitted by the host's
  execution policy. Denied tools produce `AuthorisationError`.
- For each `SessionDirective`: verify session creation is permitted.
- For each `AgentDirective`: verify the named agent exists in the host's agent
  registry and the requested model (if any) is available.
- Authorisation is performed by the runtime harness, not the core engine. The
  core engine treats directives as pass-through.

**Error conditions**:
- `AuthorisationError::ToolDenied { tool, span }`
- `AuthorisationError::SessionDenied { name, span }`
- `AuthorisationError::AgentNotFound { name, span }`
- `AuthorisationError::ModelUnavailable { model, span }`

## Phase 5: Execute

**Input**: Resolved AST + `SkillRegistry` + execution context.

**Output**: Final document with all invocation nodes replaced by their results.

**Behaviour**:
- Walk the AST **bottom-up** (post-order traversal).
- For each `Invocation` node (deepest first):
  1. Check the node's execution policy (see `execution.md`).
  2. Execute the bound implementation with:
     - `params`: key-value map from `<param>` children.
     - `scope`: the text content (with any inner results already injected).
     - `context`: execution metadata (parent chain, registry, etc.).
  3. Receive a `SkillResult { text, metadata, status }`.
  4. Replace the `<skill>...</skill>` tag in the parent's content with `result.text`.
  5. Attach `result.metadata` to the execution trace (not injected into content).
- For each `DirectiveNode`:
  1. Execute children normally (results concatenate).
  2. The runtime harness applies directive-specific behaviour (tool constraints,
     session isolation, agent delegation).
  3. The directive tag is replaced by the concatenated child output.
- Definition nodes are skipped (marked non-executable in Phase 3).
- Text nodes are passed through unchanged.

**Retry behaviour**:
- If `retries` attribute is set and result status is `Failed`:
  - Re-execute up to N times.
  - Each retry receives the same inputs.
  - If all retries exhausted → propagate failure per execution policy.

**Error conditions**:
- `ExecutionError::SkillFailed { name, span, error, retries_exhausted }`
- `ExecutionError::Timeout { name, span, duration }`
- `ExecutionError::PolicyViolation { name, span, detail }`

## Phase 6: Output

**Input**: Executed document (all invocations replaced).

**Output**: Final text string + structured execution metadata.

**Behaviour**:
- Concatenate all remaining `Text` nodes (which now include injected results).
- Produce an `ExecutionTrace` containing:
  - For each executed node: resolved implementation, duration, retry count, status.
  - Resolved package/version/hash for reproducibility.
  - Any warnings accumulated during processing.
- Return `(output_text, execution_trace)`.

## Invariants

1. **Definitions never execute.** A `define=` node's body is descriptive text, never invoked.
2. **Definitions do not contain directives.** Directive nodes inside definition bodies are invalid.
3. **Execution is bottom-up.** Inner skills complete before outer skills see their scope.
4. **Results are escaped.** Injected text is NOT re-parsed for new AML tags.
5. **Resolution is deterministic.** Same inputs → same outputs (or same errors).
6. **Errors include spans.** Every error references the source location for tooling.
7. **Directives are pass-through.** The core executor treats directives as transparent wrappers; runtime behaviour is harness-specific.
