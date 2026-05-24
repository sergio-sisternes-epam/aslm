//! Smoke test: full pipeline with directives.
//!
//! Exercises parse → validate → execute for a complex AML document
//! combining definitions, directives (tool, session, agent), nesting,
//! and mixed text content.

use std::collections::HashMap;

use aml_core::ast::{
    AgentMode, DirectiveKind, Node, NodeKind, SessionDirective,
};
use aml_core::executor::{ExecutionContext, SkillResult, SkillStatus};
use aml_core::parser::parse;
use aml_core::registry::SkillRegistry;
use aml_core::validator::{validate, Severity};

// ── Helpers ──────────────────────────────────────────────────────────

fn build_registry() -> SkillRegistry {
    let mut reg = SkillRegistry::new();
    reg.register_interface("code-review".into(), Some("Review code".into()))
        .unwrap();
    reg.register_implementation(
        "python-review".into(),
        "code-review".into(),
        Some("python".into()),
        None,
        Some("Python code reviewer".into()),
        0,
    )
    .unwrap();
    reg.register_interface("testing".into(), Some("Run tests".into()))
        .unwrap();
    reg.register_implementation(
        "pytest-runner".into(),
        "testing".into(),
        Some("python".into()),
        None,
        Some("Pytest executor".into()),
        0,
    )
    .unwrap();
    reg
}

fn build_context() -> ExecutionContext {
    let reg = build_registry();
    let mut ctx = ExecutionContext::new(reg);

    ctx.register_handler(
        "python-review",
        Box::new(|_name, _params, scope| {
            Ok(SkillResult {
                text: format!("[reviewed: {}]", scope.trim()),
                metadata: HashMap::new(),
                status: SkillStatus::Success,
            })
        }),
    );
    ctx.register_handler(
        "pytest-runner",
        Box::new(|_name, _params, scope| {
            Ok(SkillResult {
                text: format!("[tested: {}]", scope.trim()),
                metadata: HashMap::new(),
                status: SkillStatus::Success,
            })
        }),
    );
    ctx
}

// ── The complex document ────────────────────────────────────────────

const COMPLEX_DOC: &str = r#"Here is the analysis pipeline.

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
"#;

// ── Tests ───────────────────────────────────────────────────────────

#[test]
fn smoke_parse_complex_document() {
    let doc = parse(COMPLEX_DOC).expect("parse should succeed");

    // Should contain: Text, 4 definitions, Text, Session directive, Text
    assert!(
        doc.nodes.len() >= 7,
        "expected at least 7 top-level nodes, got {}",
        doc.nodes.len()
    );

    // Count node types
    let definitions = doc
        .nodes
        .iter()
        .filter(|n| n.is_definition())
        .count();
    assert_eq!(definitions, 4, "expected 4 definitions");

    let directives: Vec<_> = doc
        .nodes
        .iter()
        .filter(|n| matches!(n, Node::Directive { .. }))
        .collect();
    assert_eq!(directives.len(), 1, "expected 1 top-level directive (session)");

    // Verify the session directive structure
    if let Node::Directive { kind, children, .. } = &directives[0] {
        match kind {
            DirectiveKind::Session(SessionDirective { name, isolated, .. }) => {
                assert_eq!(name.as_deref(), Some("review-pipeline"));
                assert_eq!(*isolated, Some(true));
            }
            other => panic!("expected Session directive, got {:?}", other),
        }

        // Session should contain 2 agent directives (plus whitespace text)
        let agents: Vec<_> = children
            .iter()
            .filter(|n| matches!(n, Node::Directive { kind: DirectiveKind::Agent(_), .. }))
            .collect();
        assert_eq!(agents.len(), 2, "session should contain 2 agents");

        // First agent: lead-reviewer
        if let Node::Directive { kind: DirectiveKind::Agent(agent), children: agent_children, .. } =
            &agents[0]
        {
            assert_eq!(agent.name, "lead-reviewer");
            assert_eq!(agent.mode, Some(AgentMode::Sync));
            assert!(agent.model.is_none());

            // Should contain a tool directive
            let tools: Vec<_> = agent_children
                .iter()
                .filter(|n| matches!(n, Node::Directive { kind: DirectiveKind::Tool(_), .. }))
                .collect();
            assert_eq!(tools.len(), 1);

            if let Node::Directive { kind: DirectiveKind::Tool(tool), children: tool_children, .. } =
                &tools[0]
            {
                assert_eq!(tool.allow.as_deref(), Some("grep,view,bash"));
                assert!(tool.deny.is_none());

                // Should contain a skill invocation
                let skills: Vec<_> = tool_children
                    .iter()
                    .filter(|n| matches!(n, Node::Skill { kind: NodeKind::Invocation { .. }, .. }))
                    .collect();
                assert_eq!(skills.len(), 1, "tool should wrap one skill");
            }
        }

        // Second agent: test-engineer
        if let Node::Directive { kind: DirectiveKind::Agent(agent), .. } = &agents[1] {
            assert_eq!(agent.name, "test-engineer");
            assert_eq!(agent.model.as_deref(), Some("claude-haiku"));
            assert_eq!(agent.mode, Some(AgentMode::Background));
        }
    }
}

#[test]
fn smoke_validate_complex_document() {
    let doc = parse(COMPLEX_DOC).expect("parse should succeed");
    let errors = validate(&doc.nodes);
    assert!(
        errors.is_empty(),
        "expected no validation errors, got: {:?}",
        errors
    );
}

#[test]
fn smoke_execute_complex_document() {
    let ctx = build_context();
    let doc = parse(COMPLEX_DOC).expect("parse should succeed");
    let result = ctx.execute(&doc).expect("execution should succeed");

    // Definitions produce no output; text is preserved; directives are pass-through
    assert!(result.contains("Here is the analysis pipeline."));
    assert!(result.contains("Now executing the secure review pipeline:"));
    assert!(result.contains("Pipeline complete."));

    // Skill invocations should have been handled
    assert!(
        result.contains("[reviewed:"),
        "code-review handler should have run, got: {result}"
    );
    assert!(
        result.contains("[tested:"),
        "testing handler should have run, got: {result}"
    );

    // Definitions should NOT appear in output
    assert!(
        !result.contains("Analyse code for bugs"),
        "definition body should not appear in output"
    );
}

#[test]
fn smoke_execute_directive_passthrough_preserves_order() {
    let ctx = build_context();
    let input = r#"before <session name="s1"><tool allow="bash"><skill interface="testing" language="python">code</skill></tool></session> after"#;
    let doc = parse(input).expect("parse should succeed");
    let result = ctx.execute(&doc).expect("execution should succeed");
    assert_eq!(result, "before [tested: code] after");
}

// ── Negative tests ──────────────────────────────────────────────────

#[test]
fn smoke_validate_allow_deny_conflict() {
    let input = r#"<tool allow="bash" deny="grep">content</tool>"#;
    let doc = parse(input).expect("parse should succeed");
    let errors = validate(&doc.nodes);
    assert!(
        !errors.is_empty(),
        "should report allow/deny conflict"
    );
    assert!(
        errors[0].message.contains("mutually exclusive")
            || errors[0].message.contains("allow")
            || errors[0].message.contains("deny"),
        "error should mention allow/deny, got: {}",
        errors[0].message
    );
}

#[test]
fn smoke_parse_agent_missing_name() {
    // <agent> without name should fail at parse time (name is required)
    let input = r#"<agent mode="sync">content</agent>"#;
    let result = parse(input);
    // Either parse error or validation catches it
    if let Ok(doc) = result {
        let errors = validate(&doc.nodes);
        assert!(!errors.is_empty(), "should report missing agent name");
    }
    // If parse itself fails, that's also acceptable
}

#[test]
fn smoke_validate_directive_in_definition() {
    let input = r#"<skill define="interface" name="review"><tool name="bash">content</tool></skill>"#;
    let doc = parse(input).expect("parse should succeed");
    let errors = validate(&doc.nodes);
    assert!(
        !errors.is_empty(),
        "should report directive inside definition body"
    );
}

// ── Self-closing directives ─────────────────────────────────────────

#[test]
fn smoke_self_closing_directives() {
    let input = r#"before <tool name="bash" /> middle <session name="s1" /> after"#;
    let doc = parse(input).expect("parse should succeed");
    let errors = validate(&doc.nodes);
    assert!(errors.is_empty(), "self-closing directives should be valid");

    let ctx = build_context();
    let result = ctx.execute(&doc).expect("execution should succeed");
    assert_eq!(result, "before  middle  after");
}

// ── Deeply nested directives ────────────────────────────────────────

#[test]
fn smoke_triple_nested_directives() {
    let ctx = build_context();
    let input = r#"<session name="outer"><agent name="worker"><tool allow="grep"><skill interface="testing" language="python">deep</skill></tool></agent></session>"#;
    let doc = parse(input).expect("parse should succeed");
    let errors = validate(&doc.nodes);
    assert!(errors.is_empty(), "triple nesting should be valid: {:?}", errors);

    let result = ctx.execute(&doc).expect("execution should succeed");
    assert_eq!(result, "[tested: deep]");
}

// ── Mixed directives at same level ──────────────────────────────────

#[test]
fn smoke_sibling_directives() {
    let ctx = build_context();
    let input = r#"<tool allow="bash"><skill interface="code-review" language="python">a</skill></tool><agent name="helper"><skill interface="testing" language="python">b</skill></agent>"#;
    let doc = parse(input).expect("parse should succeed");
    let result = ctx.execute(&doc).expect("execution should succeed");
    assert_eq!(result, "[reviewed: a][tested: b]");
}

// ── Nested tool directives ──────────────────────────────────────────

/// Outer tool sets a broad allowlist; inner tool narrows it further.
/// A conforming runtime would intersect the two: only `grep` survives.
/// The validator warns that `web_search` is outside the ancestor's allow-list.
#[test]
fn smoke_nested_tools_narrowing() {
    let ctx = build_context();
    let input = r#"<tool allow="grep,view,bash,glob">
  <tool allow="grep,web_search">
    <skill interface="code-review" language="python">search code</skill>
  </tool>
</tool>"#;
    let doc = parse(input).expect("parse should succeed");
    let errors = validate(&doc.nodes);

    // Should produce a warning (not error) about web_search
    let warnings: Vec<_> = errors.iter()
        .filter(|e| e.severity == Severity::Warning)
        .collect();
    assert_eq!(warnings.len(), 1, "expected 1 warning about web_search: {:?}", errors);
    assert!(warnings[0].message.contains("web_search"), "warning should mention web_search");

    let hard_errors: Vec<_> = errors.iter()
        .filter(|e| e.severity == Severity::Error)
        .collect();
    assert!(hard_errors.is_empty(), "no hard errors expected");

    let result = ctx.execute(&doc).expect("execution should succeed");
    assert!(result.contains("[reviewed:"), "skill should execute through nested tools");

    // Verify AST structure: outer tool > inner tool > skill
    let outer = &doc.nodes.iter()
        .find(|n| matches!(n, Node::Directive { kind: DirectiveKind::Tool(_), .. }))
        .expect("should have outer tool");
    if let Node::Directive { kind: DirectiveKind::Tool(tool), children, .. } = outer {
        assert_eq!(tool.allow.as_deref(), Some("grep,view,bash,glob"));
        let inner = children.iter()
            .find(|n| matches!(n, Node::Directive { kind: DirectiveKind::Tool(_), .. }))
            .expect("should have inner tool");
        if let Node::Directive { kind: DirectiveKind::Tool(inner_tool), children: inner_children, .. } = inner {
            assert_eq!(inner_tool.allow.as_deref(), Some("grep,web_search"));
            let has_skill = inner_children.iter()
                .any(|n| matches!(n, Node::Skill { kind: NodeKind::Invocation { .. }, .. }));
            assert!(has_skill, "inner tool should contain the skill");
        }
    }
}

/// Outer tool uses deny; inner tool uses allow — both constraints apply.
/// Runtime should deny `bash` from outer AND restrict to `grep,view` from inner.
#[test]
fn smoke_nested_tools_deny_then_allow() {
    let ctx = build_context();
    let input = r#"<tool deny="bash,web_fetch">
  <tool allow="grep,view">
    <skill interface="testing" language="python">safe search</skill>
  </tool>
</tool>"#;
    let doc = parse(input).expect("parse should succeed");
    let errors = validate(&doc.nodes);
    assert!(errors.is_empty(), "deny+allow nesting should be valid: {:?}", errors);

    let result = ctx.execute(&doc).expect("execution should succeed");
    assert_eq!(result.trim(), "[tested: safe search]");
}

/// Three levels of tool nesting: progressively restricting available tools.
#[test]
fn smoke_triple_nested_tools() {
    let ctx = build_context();
    let input = r#"<tool allow="grep,glob,view,bash,web_search,web_fetch,sql">
  <tool deny="bash,web_fetch">
    <tool allow="grep,view">
      <skill interface="code-review" language="python">deeply constrained</skill>
    </tool>
  </tool>
</tool>"#;
    let doc = parse(input).expect("parse should succeed");
    let errors = validate(&doc.nodes);
    assert!(errors.is_empty(), "triple tool nesting should be valid: {:?}", errors);

    let result = ctx.execute(&doc).expect("execution should succeed");
    assert!(result.contains("[reviewed: deeply constrained]"));

    // Walk 3 levels deep
    let mut depth = 0;
    let mut current_nodes = &doc.nodes;
    loop {
        let tool_node = current_nodes.iter()
            .find(|n| matches!(n, Node::Directive { kind: DirectiveKind::Tool(_), .. }));
        match tool_node {
            Some(Node::Directive { children, .. }) => {
                depth += 1;
                current_nodes = children;
            }
            _ => break,
        }
    }
    assert_eq!(depth, 3, "should have 3 levels of tool nesting");
}

/// Nested tools inside agents inside a session — realistic multi-agent pipeline
/// where each agent gets progressively different tool access.
#[test]
fn smoke_realistic_pipeline_with_nested_tools() {
    let ctx = build_context();
    let input = r#"<session name="ci-pipeline" isolated="true">
  <agent name="explorer" mode="sync">
    <tool allow="grep,glob,view">
      <skill interface="code-review" language="python">
        # Read-only exploration phase
        import ast
        tree = ast.parse(source)
      </skill>
    </tool>
  </agent>

  <agent name="implementer" mode="sync">
    <tool allow="grep,glob,view,bash,web_search">
      <tool deny="web_fetch">
        <skill interface="code-review" language="python">
          # Implementation phase - can run commands but not fetch URLs
          subprocess.run(["cargo", "build"])
        </skill>
      </tool>
    </tool>
  </agent>

  <agent name="reviewer" mode="sync">
    <tool allow="grep,view">
      <skill interface="code-review" language="python">
        # Review phase - minimal tools, read-only
        diff = get_diff()
      </skill>
    </tool>
  </agent>

  <agent name="deployer" mode="background">
    <tool allow="bash">
      <tool allow="bash">
        <skill interface="testing" language="python">
          # Deploy phase - only bash, double-wrapped for emphasis
          deploy_to_staging()
        </skill>
      </tool>
    </tool>
  </agent>
</session>"#;

    let doc = parse(input).expect("parse should succeed");
    let errors = validate(&doc.nodes);
    assert!(errors.is_empty(), "realistic pipeline should be valid: {:?}", errors);

    let result = ctx.execute(&doc).expect("execution should succeed");

    // All 3 code-review skills + 1 testing skill should execute
    let review_count = result.matches("[reviewed:").count();
    let test_count = result.matches("[tested:").count();
    assert_eq!(review_count, 3, "3 code-review skills should run");
    assert_eq!(test_count, 1, "1 testing skill should run");

    // Verify agent structure
    let session = doc.nodes.iter()
        .find(|n| matches!(n, Node::Directive { kind: DirectiveKind::Session(_), .. }))
        .expect("should have session");
    if let Node::Directive { children, .. } = session {
        let agents: Vec<_> = children.iter()
            .filter(|n| matches!(n, Node::Directive { kind: DirectiveKind::Agent(_), .. }))
            .collect();
        assert_eq!(agents.len(), 4, "session should have 4 agents");

        // Verify implementer has nested tools (allow > deny)
        if let Node::Directive { kind: DirectiveKind::Agent(agent), children: ac, .. } = &agents[1] {
            assert_eq!(agent.name, "implementer");
            let outer_tool = ac.iter()
                .find(|n| matches!(n, Node::Directive { kind: DirectiveKind::Tool(_), .. }))
                .expect("implementer should have outer tool");
            if let Node::Directive { kind: DirectiveKind::Tool(t), children: tc, .. } = outer_tool {
                assert_eq!(t.allow.as_deref(), Some("grep,glob,view,bash,web_search"));
                let inner_tool = tc.iter()
                    .find(|n| matches!(n, Node::Directive { kind: DirectiveKind::Tool(_), .. }))
                    .expect("implementer should have inner tool");
                if let Node::Directive { kind: DirectiveKind::Tool(it), .. } = inner_tool {
                    assert_eq!(it.deny.as_deref(), Some("web_fetch"));
                }
            }
        }

        // Verify deployer runs in background
        if let Node::Directive { kind: DirectiveKind::Agent(agent), .. } = &agents[3] {
            assert_eq!(agent.name, "deployer");
            assert_eq!(agent.mode, Some(AgentMode::Background));
        }
    }
}

/// Tool directive with `name` shorthand (equivalent to `allow="<name>"`).
#[test]
fn smoke_tool_name_shorthand_nested() {
    let ctx = build_context();
    let input = r#"<tool name="bash">
  <tool name="grep">
    <skill interface="testing" language="python">single tool each level</skill>
  </tool>
</tool>"#;
    let doc = parse(input).expect("parse should succeed");
    let errors = validate(&doc.nodes);
    assert!(errors.is_empty(), "name shorthand nesting should be valid: {:?}", errors);

    let result = ctx.execute(&doc).expect("execution should succeed");
    assert!(result.contains("[tested: single tool each level]"));
}

/// Sibling tool directives at the same level inside an agent.
#[test]
fn smoke_sibling_tools_in_agent() {
    let ctx = build_context();
    let input = r#"<agent name="multi-phase">
  <tool allow="grep,view">
    <skill interface="code-review" language="python">phase 1: search</skill>
  </tool>
  <tool allow="bash,glob">
    <skill interface="testing" language="python">phase 2: execute</skill>
  </tool>
</agent>"#;
    let doc = parse(input).expect("parse should succeed");
    let errors = validate(&doc.nodes);
    assert!(errors.is_empty(), "sibling tools should be valid: {:?}", errors);

    let result = ctx.execute(&doc).expect("execution should succeed");
    assert!(result.contains("[reviewed: phase 1: search]"));
    assert!(result.contains("[tested: phase 2: execute]"));
}
