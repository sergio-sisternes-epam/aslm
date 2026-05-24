<!-- Agent directive with model and mode -->
<agent name="security-reviewer" model="claude-sonnet" mode="sync">
  <skill interface="code-review">
    password = input("Enter: ")
    db.execute(f"SELECT * FROM users WHERE pass='{password}'")
  </skill>
</agent>
