# ADR-009: Teach AML via Usage Guide Skill

## Status
Accepted

## Context
Agents need to learn how to use AML correctly. Options:
- Hardcode instructions in system prompts
- Distribute as documentation
- Deliver as an APM skill (self-contained, versionable)

## Decision
Deliver AML usage instructions as an APM skill (`aml-usage-guide`) that agents can load.

## Rationale
- Self-contained: the skill carries everything needed to use AML
- Versionable: updates to instructions are delivered via APM
- Composable: can be loaded alongside other skills
- Testable: trigger and content evals validate effectiveness
- Consistent with "eat your own dog food" — AML knowledge delivered via APM

## Consequences
- Must follow SKILL.md format (≤500 lines, ≤5000 tokens, ≤1024 char description)
- Detailed examples go in `references/` for lazy loading
- Must include few-shot examples for common patterns
- Evals needed to verify the skill triggers correctly
