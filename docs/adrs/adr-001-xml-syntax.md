# ADR-001: Use XML-like Syntax

## Status
Accepted

## Context
SML needs a syntax for structured skill invocation within agent prompts. Options considered:
- JSON blocks
- YAML blocks
- Custom DSL
- XML-like tags

## Decision
Use XML-like `<skill>` tags with attributes and content.

## Rationale
- Clear boundaries (open/close tags) make scope explicit
- Native attribute support for metadata
- Proven compatibility with LLMs (XML is well-represented in training data)
- Nesting is natural and unambiguous
- Existing tool support for syntax highlighting and validation

## Consequences
- Must handle entity escaping (`&lt;`, `&amp;`, etc.)
- Parser must be tolerant of non-SML `<` characters in prompt text
- Cannot use self-closing tags for skills with content
