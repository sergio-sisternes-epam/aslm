# Execution Conformance: Ambiguous Resolution Error

## Input

```aml
<skill interface="unit-testing-coverage">
  Test the auth module.
</skill>
```

## Registry

Registered implementations for `unit-testing-coverage`:
- `python-pytest-v2` (language="python")
- `js-jest-v1` (language="javascript")

No default configured. No hints provided.

## Expected behaviour

1. Resolution enters Interface Mode.
2. Candidates: both implementations.
3. No hints to filter.
4. No registry default.
5. Multiple candidates remain → ResolutionError::Ambiguous.

## Expected error

```
ResolutionError::Ambiguous {
    interface: "unit-testing-coverage",
    candidates: ["python-pytest-v2", "js-jest-v1"]
}
```

## Expected output

None (error returned).
