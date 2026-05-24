"""
Example: Nested skill execution with bottom-up evaluation.

Inner skills execute first, their results become the scope for outer skills.
"""


def main():
    from aml import AmlRegistry, execute, parse

    # Set up registry
    registry = AmlRegistry()
    registry.register_interface("fetch", "Fetch content from a URL")
    registry.register_interface("summarise", "Summarise text content")
    registry.register_implementation("http-fetch", "fetch")
    registry.register_implementation("gpt-summarise", "summarise")

    # Nested AML: fetch runs first, then summarise operates on the result
    prompt = """
<skill interface="summarise">
  <skill interface="fetch">
    <param name="url">https://example.com/article</param>
  </skill>
</skill>
"""

    doc = parse(prompt)
    print(f"Nodes: {doc.node_count}")
    print(f"Invocations: {doc.invocations()}")

    # In pass-through mode, the inner content flows to the outer skill
    result = execute(doc, registry)
    print(f"Result: {result}")


if __name__ == "__main__":
    main()
