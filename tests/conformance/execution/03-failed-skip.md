# Execution Conformance: Failed Inner Skill with on-failure="skip"

## Input

```aml
<skill interface="report">
  Introduction paragraph.
  <skill interface="fetch-metrics" on-failure="skip" retries="1">
    https://unreachable.example.com/api
  </skill>
  Conclusion paragraph.
</skill>
```

## Registry

- `fetch-metrics` implementation: always fails (simulating unreachable API).
- `report` implementation: returns "Report: " + scope.

## Expected behaviour

1. `fetch-metrics` executes → fails.
2. Retry #1 → fails again.
3. All retries exhausted. `on-failure="skip"` → replace with empty string.
4. `report` scope becomes: "Introduction paragraph.\n  \n  Conclusion paragraph."
5. `report` executes with the gap where fetch-metrics was.

## Expected output

```
Report: Introduction paragraph.
  
  Conclusion paragraph.
```

## Expected trace

```yaml
- node: report
  children:
    - node: fetch-metrics
      retries: 1
      attempts: 2
      status: failed
      on_failure: skip
      final_action: replaced_with_empty
  status: success
```
