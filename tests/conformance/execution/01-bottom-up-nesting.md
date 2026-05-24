# Execution Conformance: Bottom-Up Nesting

## Input

```aml
<skill interface="summarise">
  <skill interface="uppercase">hello world</skill>
  needs summarising.
</skill>
```

## Registry

- `uppercase` implementation: returns input text uppercased.
- `summarise` implementation: returns "Summary: " + scope.

## Expected execution order

1. `uppercase` executes first with scope "hello world" → returns "HELLO WORLD"
2. Result injected: scope becomes "HELLO WORLD\n  needs summarising."
3. `summarise` executes with enriched scope → returns "Summary: HELLO WORLD\n  needs summarising."

## Expected output

```
Summary: HELLO WORLD
  needs summarising.
```

## Expected trace

```yaml
- node: summarise
  children:
    - node: uppercase
      status: success
      duration_ms: 1
  status: success
  duration_ms: 2
```
