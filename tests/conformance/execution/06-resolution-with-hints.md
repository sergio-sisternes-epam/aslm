# Execution Conformance: Interface Resolution with Hints

## Input

```sml
<skill interface="unit-testing-coverage" language="python">
  Test the auth module.
</skill>
```

## Registry

Registered implementations for `unit-testing-coverage`:
- `python-pytest-v2` (language="python", framework="pytest")
- `js-jest-v1` (language="javascript", framework="jest")
- `go-testing-v1` (language="go")

## Expected behaviour

1. Resolution enters Interface Mode.
2. Candidates: all 3 implementations.
3. Filter by `language="python"` → only `python-pytest-v2` remains.
4. Exactly one candidate → resolved.
5. `python-pytest-v2` executes with scope "Test the auth module."

## Expected resolution

```yaml
resolved_impl: python-pytest-v2
resolution_mode: interface
hints_applied:
  - language: python
candidates_before_filter: 3
candidates_after_filter: 1
```
