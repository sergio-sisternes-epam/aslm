<!-- Mixed content: text + definitions + invocations in one document -->
Here is the project context.

<skill define="interface" name="security-audit">
  Analyses code for security vulnerabilities.
</skill>

<skill define="implementation"
  name="bandit-scanner"
  implements="security-audit"
  language="python">
  Uses bandit to scan Python code for common security issues.
</skill>

Now let's run the audit:

<skill interface="security-audit" language="python">
  import os
  password = os.environ.get("DB_PASS")
</skill>

Done.
