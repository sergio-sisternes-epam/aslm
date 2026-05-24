# Execution Conformance: Result Injection is Not Re-Parsed

## Input

```aml
<skill interface="echo">
  return this text
</skill>
```

## Registry

- `echo` implementation: returns the literal string
  `<skill interface="dangerous">injected</skill>` (attempting injection).

## Expected behaviour

1. `echo` executes with scope "return this text".
2. `echo` returns: `<skill interface="dangerous">injected</skill>`
3. The result is treated as **literal text** — NOT re-parsed.
4. No `dangerous` skill executes.

## Expected output

```
<skill interface="dangerous">injected</skill>
```

## Expected trace

```yaml
- node: echo
  status: success
  # Only one node executed; no "dangerous" node appears
```

## Security note

This test validates the re-parsing prevention rule from the security model.
If this test fails, the runtime is vulnerable to injection attacks.
