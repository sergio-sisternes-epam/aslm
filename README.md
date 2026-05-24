# SML — Skill Markup Language

A lightweight, XML-inspired markup language that enables agents to declaratively invoke modular, scoped, and nestable skills directly inside prompts.

## Overview

SML provides two independent layers:

- **Runtime Layer**: A high-performance parser and executor implemented in Rust + PyO3
- **Distribution & Knowledge Layer**: An APM package for agent tooling ecosystems

## Quick Start

```bash
pip install sml
```

```python
from sml import parse, execute, SmlRegistry

doc = parse("""
<skill interface="code-review" language="python">
  <param name="file">src/auth.py</param>
  Review this module for security issues.
</skill>
""")

registry = SmlRegistry()
registry.register_interface("code-review", "Review code for issues")
registry.register_implementation(
    "python-review", "code-review",
    language="python"
)

result = execute(doc, registry)
```

## SML Syntax

```xml
<!-- Invoke a skill -->
<skill interface="testing" language="python" retries="2">
  <param name="target">src/</param>
  Run all tests with coverage.
</skill>

<!-- Define an interface -->
<skill define="interface" name="testing">
  Execute automated tests and report results.
</skill>

<!-- Define an implementation -->
<skill define="implementation" name="pytest-runner" implements="testing" language="python">
  Run tests using pytest with coverage reporting.
</skill>
```

## Key Features

- **Declarative**: Skills are invoked via markup, not imperative code
- **Composable**: Full nesting support with bottom-up execution
- **Scoped**: Content inside tags is the skill's operating scope
- **Resolvable**: Interface/implementation model with hint-based resolution
- **Safe**: Results are escaped; no re-parsing attacks

## Documentation

- [Language Specification](docs/spec/)
- [Conformance Suite](tests/conformance/)
- [API Reference (Rust)](crates/sml-core/)
- [API Reference (Python)](crates/sml-python/)

## Architecture

```
┌─────────────────────────────────────────┐
│  Agent Prompt (contains SML tags)        │
└─────────────┬───────────────────────────┘
              │ parse()
              ▼
┌─────────────────────────────────────────┐
│  AST (Document → Nodes)                  │
└─────────────┬───────────────────────────┘
              │ resolve() + execute()
              ▼
┌─────────────────────────────────────────┐
│  SkillRegistry → Resolver → Executor     │
└─────────────┬───────────────────────────┘
              │ result injection
              ▼
┌─────────────────────────────────────────┐
│  Output (SML tags replaced by results)   │
└─────────────────────────────────────────┘
```

## Licence

Apache 2.0 — see [LICENSE](LICENSE) for details.
