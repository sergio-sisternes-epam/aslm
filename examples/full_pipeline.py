"""
End-to-end example: Parse, register, resolve, and execute AML.

This example demonstrates the full AML pipeline using the Python API.
"""


def main():
    # Note: This example requires the aml package to be installed
    # pip install aml (or: maturin develop --release)
    from aml import AmlRegistry, execute, parse

    # 1. Define interfaces and implementations in AML
    definitions = """
<skill define="interface" name="code-review">
  Analyse code for bugs, style issues, and security vulnerabilities.
</skill>

<skill define="implementation" name="python-review" implements="code-review" language="python">
  Use ruff rules, type safety checks, and PEP compliance to review Python code.
</skill>

<skill define="implementation" name="rust-review" implements="code-review" language="rust">
  Use clippy lints, unsafe audit, and Rust idioms to review Rust code.
</skill>
"""

    # 2. Parse and register definitions
    def_doc = parse(definitions)
    registry = AmlRegistry()
    registry.register_from_document(def_doc)

    # 3. Parse an invocation
    prompt = """
Please review this code:

<skill interface="code-review" language="python">
def login(username, password):
    query = f"SELECT * FROM users WHERE name='{username}' AND pass='{password}'"
    return db.execute(query)
</skill>
"""

    doc = parse(prompt)
    print(f"Parsed {doc.node_count} nodes")
    print(f"Invocations: {doc.invocations()}")

    # 4. Execute (pass-through mode — no custom handlers registered)
    result = execute(doc, registry)
    print(f"Result:\n{result}")


if __name__ == "__main__":
    main()
