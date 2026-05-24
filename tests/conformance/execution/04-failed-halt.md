# Execution Conformance: Failed Inner Skill with on-failure="halt"

## Input

```aml
<skill interface="report">
  <skill interface="critical-data" on-failure="halt">
    fetch critical metrics
  </skill>
  Rest of report.
</skill>
```

## Registry

- `critical-data` implementation: always fails.
- `report` implementation: returns "Report: " + scope.

## Expected behaviour

1. `critical-data` executes → fails.
2. No retries configured (default retries=0).
3. `on-failure="halt"` → stop execution entirely.
4. `report` does NOT execute.
5. Return ExecutionError::SkillFailed.

## Expected output

None (error returned).

## Expected error

```
ExecutionError::SkillFailed {
    name: "critical-data",
    retries_exhausted: true,
    error: "simulated failure"
}
```

## Expected trace

```yaml
- node: report
  status: not_executed
  children:
    - node: critical-data
      status: failed
      on_failure: halt
      propagated: true
```
