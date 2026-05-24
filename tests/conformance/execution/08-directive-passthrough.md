# Execution Conformance: Directive Pass-Through

## Input

```aml
<session name="sandbox" isolated="true">
  <agent name="reviewer" mode="sync">
    <tool allow="grep,view">
      <skill interface="testing" language="python">assert True</skill>
    </tool>
  </agent>
</session>
```

## Registry

- `testing` implementation `pytest-impl`: returns "[tested: {scope}]"

## Expected execution order

1. Core executor walks directive tree (session → agent → tool) as pass-through
2. Innermost `<skill>` resolves to `pytest-impl` and executes with scope "assert True"
3. Directives contribute no extra output; only the skill result is returned

## Expected output

```
[tested: assert True]
```

## Notes

- In a real runtime, the harness would enforce tool constraints, create sessions,
  and spawn agents. The core executor treats directives as transparent wrappers.
- Directive attributes are preserved in the AST for harness inspection.
