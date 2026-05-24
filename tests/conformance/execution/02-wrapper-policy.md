# Execution Conformance: Wrapper Policy

## Input

```aml
<skill interface="sandbox" policy="wrapper">
  <skill interface="file-writer">
    Write secret.txt with contents "password123"
  </skill>
</skill>
```

## Registry

- `sandbox` implementation (policy: wrapper): checks if child has `fs:write`
  capability and denies execution if security policy is restrictive.
- `file-writer` implementation: capabilities=["fs:write"]

## Expected behaviour

1. `sandbox` executes FIRST (wrapper policy — it controls children).
2. `sandbox` inspects the child `file-writer` node.
3. `sandbox` determines `fs:write` is not allowed by current policy.
4. `sandbox` returns: "Blocked: file-writer requires fs:write capability."

The inner `file-writer` skill is NEVER executed.

## Expected output

```
Blocked: file-writer requires fs:write capability.
```

## Expected trace

```yaml
- node: sandbox
  policy: wrapper
  children:
    - node: file-writer
      status: skipped
      reason: "denied by wrapper"
  status: success
```
