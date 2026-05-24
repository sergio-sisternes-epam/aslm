"""
Example: Using directive tags (tool, session, agent) with AML.

Directives control *how* content is executed — tool constraints, session
isolation, and subagent delegation — while skills declare *what* to execute.
"""


def main():
    from aml import AmlRegistry, execute, parse

    # Set up registry with a simple skill
    registry = AmlRegistry()
    registry.register_interface("lint", "Run linting on code")
    registry.register_implementation("python-lint", "lint")

    # Example 1: Tool constraint — restrict available tools
    prompt_tool = """
<tool allow="bash,grep">
  <skill interface="lint">
    def greet(name):
        print(f"Hello {name}")
  </skill>
</tool>
"""
    doc = parse(prompt_tool)
    print(f"Tool directive nodes: {doc.node_count}")
    result = execute(doc, registry)
    print(f"Tool result: {result}")

    # Example 2: Session isolation
    prompt_session = """
<session name="review-session" isolated="true">
  <skill interface="lint">
    import os
    os.system("rm -rf /")
  </skill>
</session>
"""
    doc = parse(prompt_session)
    print(f"\nSession directive nodes: {doc.node_count}")
    result = execute(doc, registry)
    print(f"Session result: {result}")

    # Example 3: Agent delegation
    prompt_agent = """
<agent name="security-reviewer" model="gpt-4" mode="sync">
  <skill interface="lint">
    password = input("Enter password: ")
    db.execute(f"SELECT * FROM users WHERE pass='{password}'")
  </skill>
</agent>
"""
    doc = parse(prompt_agent)
    print(f"\nAgent directive nodes: {doc.node_count}")
    result = execute(doc, registry)
    print(f"Agent result: {result}")

    # Example 4: Nested directives
    prompt_nested = """
<session name="secure-review">
  <agent name="reviewer">
    <tool allow="grep,view">
      <skill interface="lint">
        def handle_payment(card, amount):
            return process(card, amount)
      </skill>
    </tool>
  </agent>
</session>
"""
    doc = parse(prompt_nested)
    print(f"\nNested directive nodes: {doc.node_count}")
    result = execute(doc, registry)
    print(f"Nested result: {result}")


if __name__ == "__main__":
    main()
