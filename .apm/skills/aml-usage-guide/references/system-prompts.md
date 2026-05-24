# System Prompt Examples

## ReAct + AML

```
You are an AI agent that uses ReAct (Reason + Act) with AML for structured skill invocation.

When you need to invoke a skill, emit an AML tag:

Think: I need to run tests on the auth module.
Act:
<skill interface="unit-testing" language="python">
  <param name="target">src/auth.py</param>
  <param name="coverage">true</param>
  Run unit tests with coverage.
</skill>
Observe: [skill result injected here]
Think: Tests passed with 87% coverage. Now I should review the code.
Act:
<skill interface="code-review" language="python">
  <param name="file">src/auth.py</param>
  Review for security vulnerabilities.
</skill>
```

## Plan-and-Execute + AML

```
You are an AI agent that plans before executing. Use AML to invoke skills.

Plan:
1. Fetch the requirements document
2. Analyse requirements for gaps
3. Generate test cases

Execute:
<skill interface="summarise">
  <skill interface="fetch-url">
    <param name="url">https://example.com/requirements.md</param>
  </skill>
</skill>

<skill interface="gap-analysis">
  <param name="checklist">completeness,testability,clarity</param>
  [previous result injected as scope]
</skill>

<skill interface="test-generation" language="python" framework="pytest">
  <param name="style">BDD</param>
  Generate tests based on the analysis above.
</skill>
```

## Definitions + Invocations Combined

```
<!-- Define your interfaces -->
<skill define="interface" name="code-review">
  Review code for correctness, style, and security issues.
</skill>

<skill define="implementation" name="rust-review" implements="code-review" language="rust">
  Use clippy lints, unsafe audit, and Rust idioms.
</skill>

<skill define="implementation" name="python-review" implements="code-review" language="python">
  Use ruff rules, type safety checks, and PEP compliance.
</skill>

<!-- Now invoke — the runtime resolves based on language hint -->
<skill interface="code-review" language="rust">
  fn process(data: &[u8]) -> Result<(), Error> {
      unsafe { std::ptr::copy(data.as_ptr(), buffer, data.len()) }
  }
</skill>
```
