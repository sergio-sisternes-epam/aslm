<!-- Complex nesting: session > agent > tool > skill, with mixed text -->
Here is the analysis pipeline.

<skill define="interface" name="code-review">
  Analyse code for bugs, style, and security vulnerabilities.
</skill>

<skill define="implementation" name="python-review" implements="code-review" language="python">
  Use ruff, bandit, and type-checking to review Python code.
</skill>

<skill define="interface" name="testing">
  Run automated tests on code.
</skill>

<skill define="implementation" name="pytest-runner" implements="testing" language="python">
  Execute pytest with coverage enabled.
</skill>

Now executing the secure review pipeline:

<session name="review-pipeline" isolated="true">
  <agent name="lead-reviewer" mode="sync">
    <tool allow="grep,view,bash">
      <skill interface="code-review" language="python">
        def login(username, password):
            query = f"SELECT * FROM users WHERE name='{username}'"
            return db.execute(query)
      </skill>
    </tool>
  </agent>

  <agent name="test-engineer" model="claude-haiku" mode="background">
    <tool deny="bash">
      <skill interface="testing" language="python">
        def test_login():
            assert login("admin", "pass") is not None
      </skill>
    </tool>
  </agent>
</session>

Pipeline complete.
