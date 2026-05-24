use crate::ast::{DirectiveKind, Node, NodeKind, Span, ToolDirective};

/// Validation error with source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub message: String,
    pub span: Option<Span>,
    pub severity: Severity,
}

/// Error severity — errors prevent execution, warnings are advisory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let prefix = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        if let Some(span) = self.span {
            write!(f, "[{}..{}] {}: {}", span.start, span.end, prefix, self.message)
        } else {
            write!(f, "{}: {}", prefix, self.message)
        }
    }
}

/// Active tool constraints inherited from ancestor `<tool>` directives.
#[derive(Debug, Clone, Default)]
struct ToolConstraints {
    /// Tools explicitly allowed (intersection of all ancestor allow-lists).
    allowed: Option<Vec<String>>,
    /// Tools explicitly denied (union of all ancestor deny-lists).
    denied: Vec<String>,
}

impl ToolConstraints {
    /// Split a comma-separated attribute into non-empty trimmed tool names.
    fn parse_tool_list(value: &str) -> Vec<String> {
        value.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Union child deny-list into accumulated denies and clean inherited allows.
    fn apply_deny(&mut self, deny: &str) {
        for tool in Self::parse_tool_list(deny) {
            if !self.denied.contains(&tool) {
                self.denied.push(tool.clone());
                if let Some(ref mut allowed) = self.allowed {
                    allowed.retain(|t| t != &tool);
                }
            }
        }
    }

    /// Intersect child allow-list with accumulated constraints, emitting warnings.
    fn apply_allow(&mut self, allow: &str, parent: &ToolConstraints) -> Vec<String> {
        let mut warnings = Vec::new();
        let requested = Self::parse_tool_list(allow);

        for tool in &requested {
            if self.denied.contains(tool) {
                warnings.push(format!(
                    "tool '{tool}' is allowed here but denied by an ancestor <tool> directive"
                ));
            }
        }

        if let Some(parent_allowed) = &parent.allowed {
            for tool in &requested {
                if !parent_allowed.contains(tool) && !self.denied.contains(tool) {
                    warnings.push(format!(
                        "tool '{tool}' is allowed here but not in ancestor's allow-list"
                    ));
                }
            }
        }

        // Use self.denied (includes this node's denies) for effective computation
        let effective = if let Some(parent_allowed) = &parent.allowed {
            requested.iter()
                .filter(|t| parent_allowed.contains(t) && !self.denied.contains(t))
                .cloned()
                .collect()
        } else {
            requested.into_iter()
                .filter(|t| !self.denied.contains(t))
                .collect()
        };
        self.allowed = Some(effective);

        warnings
    }

    /// Apply name shorthand as singleton allow-list, emitting warnings.
    fn apply_name(&mut self, name: &str, parent: &ToolConstraints) -> Vec<String> {
        let mut warnings = Vec::new();

        if self.denied.contains(&name.to_string()) {
            warnings.push(format!(
                "tool '{name}' is requested but denied by an ancestor <tool> directive"
            ));
        }
        if let Some(parent_allowed) = &parent.allowed {
            if !parent_allowed.contains(&name.to_string()) {
                warnings.push(format!(
                    "tool '{name}' is requested but not in ancestor's allow-list"
                ));
            }
        }

        // Narrow to singleton (or empty if denied/outside parent scope)
        if !self.denied.contains(&name.to_string()) {
            if let Some(parent_allowed) = &parent.allowed {
                if parent_allowed.contains(&name.to_string()) {
                    self.allowed = Some(vec![name.to_string()]);
                } else {
                    self.allowed = Some(vec![]);
                }
            } else {
                self.allowed = Some(vec![name.to_string()]);
            }
        } else {
            self.allowed = Some(vec![]);
        }

        warnings
    }

    fn apply(&self, child: &ToolDirective) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let mut new = self.clone();

        // Phase 1: union denies (runs first so deny state is current for allow/name)
        if let Some(deny) = &child.deny {
            new.apply_deny(deny);
        }

        // Phase 2: intersect allows (uses new.denied, not self.denied)
        if let Some(allow) = &child.allow {
            warnings.extend(new.apply_allow(allow, self));
        }

        // Phase 3: name shorthand — runs if allow is absent (deny may be present)
        if let Some(name) = &child.name {
            if child.allow.is_none() {
                warnings.extend(new.apply_name(name, self));
            }
        }

        // Phase 4: redundancy warnings
        if child.name.is_some() && child.allow.is_some() {
            warnings.push(
                "<tool> 'name' is redundant when 'allow' is also set".to_string()
            );
        }

        (new, warnings)
    }
}

/// Validate a parsed AST for semantic correctness.
pub fn validate(nodes: &[Node]) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let constraints = ToolConstraints::default();
    for node in nodes {
        validate_node(node, &constraints, &mut errors);
    }
    errors
}

fn validate_node(node: &Node, constraints: &ToolConstraints, errors: &mut Vec<ValidationError>) {
    match node {
        Node::Text(_) => {}
        Node::Directive {
            kind,
            children,
            span,
        } => {
            validate_directive(kind, *span, errors);

            // Compute new constraints if this is a tool directive
            let child_constraints = if let DirectiveKind::Tool(tool) = kind {
                let (new_constraints, warnings) = constraints.apply(tool);
                for warning in warnings {
                    errors.push(ValidationError {
                        message: warning,
                        span: Some(*span),
                        severity: Severity::Warning,
                    });
                }
                new_constraints
            } else {
                constraints.clone()
            };

            for child in children {
                validate_node(child, &child_constraints, errors);
            }
        }
        Node::Skill {
            kind,
            children,
            span,
            ..
        } => {
            validate_kind(kind, *span, errors);

            // Definitions must not contain nested skill invocations or directives
            if matches!(
                kind,
                NodeKind::InterfaceDefinition { .. } | NodeKind::ImplementationDefinition { .. }
            ) {
                for child in children {
                    if matches!(
                        child,
                        Node::Skill {
                            kind: NodeKind::Invocation { .. },
                            ..
                        }
                    ) {
                        errors.push(ValidationError {
                            message: "definition nodes must not contain invocations".to_string(),
                            span: child.span(),
                            severity: Severity::Error,
                        });
                    }
                    if matches!(child, Node::Directive { .. }) {
                        errors.push(ValidationError {
                            message: "definition nodes must not contain directives".to_string(),
                            span: child.span(),
                            severity: Severity::Error,
                        });
                    }
                }
            }

            for child in children {
                validate_node(child, constraints, errors);
            }
        }
    }
}

fn validate_kind(kind: &NodeKind, span: Span, errors: &mut Vec<ValidationError>) {
    match kind {
        NodeKind::Invocation {
            interface,
            r#impl,
            name,
            ..
        } => {
            if interface.is_none() && r#impl.is_none() && name.is_none() {
                errors.push(ValidationError {
                    message: "invocation must have at least one of: interface, impl, or name"
                        .to_string(),
                    span: Some(span),
                    severity: Severity::Error,
                });
            }
        }
        NodeKind::InterfaceDefinition { name, .. } => {
            if name.is_empty() {
                errors.push(ValidationError {
                    message: "interface definition must have a non-empty name".to_string(),
                    span: Some(span),
                    severity: Severity::Error,
                });
            }
        }
        NodeKind::ImplementationDefinition {
            name, implements, ..
        } => {
            if name.is_empty() {
                errors.push(ValidationError {
                    message: "implementation definition must have a non-empty name".to_string(),
                    span: Some(span),
                    severity: Severity::Error,
                });
            }
            if implements.is_empty() {
                errors.push(ValidationError {
                    message: "implementation definition must have a non-empty implements field"
                        .to_string(),
                    span: Some(span),
                    severity: Severity::Error,
                });
            }
        }
    }
}

fn validate_directive(kind: &DirectiveKind, span: Span, errors: &mut Vec<ValidationError>) {
    match kind {
        DirectiveKind::Tool(t) => {
            if t.allow.is_some() && t.deny.is_some() {
                errors.push(ValidationError {
                    message: "<tool> 'allow' and 'deny' are mutually exclusive".to_string(),
                    span: Some(span),
                    severity: Severity::Error,
                });
            }
        }
        // Note: <tool on-failure="..."> is caught at parse level — the parser
        // does not extract on-failure for ToolDirective, so it is silently
        // ignored as an unknown attribute. We check for it here via the AST
        // shape, but since ToolDirective has no on_failure field, we rely on
        // the smoke tests and parser-level validation for this constraint.
        DirectiveKind::Session(_) => {
            // No additional validation beyond parsing
        }
        DirectiveKind::Agent(a) => {
            if a.name.is_empty() {
                errors.push(ValidationError {
                    message: "<agent> must have a non-empty name".to_string(),
                    span: Some(span),
                    severity: Severity::Error,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn test_valid_invocation() {
        let doc = parse(r#"<skill interface="testing">content</skill>"#).unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_valid_definition() {
        let doc = parse(r#"<skill define="interface" name="testing">desc</skill>"#).unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_definition_with_nested_invocation() {
        let doc = parse(
            r#"<skill define="interface" name="outer"><skill interface="inner">x</skill></skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("must not contain invocations"));
    }

    #[test]
    fn test_definition_with_nested_directive() {
        let doc = parse(
            r#"<skill define="interface" name="outer"><tool name="bash">x</tool></skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("must not contain directives"));
    }

    #[test]
    fn test_tool_allow_deny_mutual_exclusivity() {
        let doc = parse(r#"<tool allow="bash" deny="exec">x</tool>"#).unwrap();
        let errors = validate(&doc.nodes);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("mutually exclusive"));
    }

    #[test]
    fn test_valid_tool_directive() {
        let doc = parse(r#"<tool name="bash">x</tool>"#).unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_valid_session_directive() {
        let doc = parse(r#"<session name="s1">x</session>"#).unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_valid_agent_directive() {
        let doc = parse(r#"<agent name="dev">x</agent>"#).unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors.is_empty());
    }

    // ── on-failure tests ────────────────────────────────────────────

    #[test]
    fn test_session_on_failure_parses() {
        let doc = parse(r#"<session name="s1" on-failure="skip">x</session>"#).unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_agent_on_failure_parses() {
        let doc = parse(r#"<agent name="dev" on-failure="partial">x</agent>"#).unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_invalid_on_failure_value() {
        let result = crate::parser::parse(r#"<session name="s1" on-failure="explode">x</session>"#);
        assert!(result.is_err(), "invalid on-failure value should cause parse error");
    }

    // ── Tool narrowing tests ────────────────────────────────────────

    #[test]
    fn test_tool_narrowing_warn_denied_tool() {
        let doc = parse(r#"<tool deny="bash"><tool allow="bash,grep">x</tool></tool>"#).unwrap();
        let errors = validate(&doc.nodes);
        let warnings: Vec<_> = errors.iter().filter(|e| e.severity == Severity::Warning).collect();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("bash"));
        assert!(warnings[0].message.contains("denied"));
    }

    #[test]
    fn test_tool_narrowing_warn_outside_allow() {
        let doc = parse(r#"<tool allow="grep,view"><tool allow="grep,bash">x</tool></tool>"#).unwrap();
        let errors = validate(&doc.nodes);
        let warnings: Vec<_> = errors.iter().filter(|e| e.severity == Severity::Warning).collect();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("bash"));
        assert!(warnings[0].message.contains("allow-list"));
    }

    #[test]
    fn test_tool_narrowing_no_warning_when_valid() {
        let doc = parse(r#"<tool allow="grep,view,bash"><tool allow="grep,view">x</tool></tool>"#).unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors.is_empty(), "valid narrowing should produce no warnings: {:?}", errors);
    }

    #[test]
    fn test_tool_narrowing_name_shorthand_denied() {
        let doc = parse(r#"<tool deny="bash"><tool name="bash">x</tool></tool>"#).unwrap();
        let errors = validate(&doc.nodes);
        let warnings: Vec<_> = errors.iter().filter(|e| e.severity == Severity::Warning).collect();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("bash"));
    }

    #[test]
    fn test_tool_narrowing_triple_nesting() {
        let doc = parse(
            r#"<tool allow="grep,view,bash"><tool deny="bash"><tool allow="grep,bash">x</tool></tool></tool>"#
        ).unwrap();
        let errors = validate(&doc.nodes);
        let warnings: Vec<_> = errors.iter().filter(|e| e.severity == Severity::Warning).collect();
        // Inner allow="grep,bash" requests bash which was denied at level 2
        assert_eq!(warnings.len(), 1, "expected warning about bash: {:?}", warnings);
        assert!(warnings[0].message.contains("bash"));
    }

    // ── Constraint state tests (verify effective allow-set, not just warnings) ──

    #[test]
    fn test_constraint_deny_removes_from_allowed() {
        // After deny="bash", a grandchild allow="bash" should NOT have bash in effective
        let tool_outer = ToolDirective { name: None, allow: Some("grep,bash,view".into()), deny: None };
        let tool_deny = ToolDirective { name: None, allow: None, deny: Some("bash".into()) };
        let tool_inner = ToolDirective { name: None, allow: Some("grep,bash".into()), deny: None };

        let root = ToolConstraints::default();
        let (after_outer, w1) = root.apply(&tool_outer);
        assert!(w1.is_empty());
        assert_eq!(after_outer.allowed.as_ref().unwrap(), &vec!["grep", "bash", "view"]);

        let (after_deny, w2) = after_outer.apply(&tool_deny);
        assert!(w2.is_empty());
        // bash should be removed from allowed after deny
        assert!(!after_deny.allowed.as_ref().unwrap().contains(&"bash".to_string()),
            "bash should be removed from allowed: {:?}", after_deny.allowed);

        let (after_inner, w3) = after_deny.apply(&tool_inner);
        // bash is denied — should warn and NOT be in effective
        assert_eq!(w3.len(), 1, "expected 1 warning: {:?}", w3);
        assert!(!after_inner.allowed.as_ref().unwrap().contains(&"bash".to_string()),
            "bash should NOT be in effective allow-list: {:?}", after_inner.allowed);
        assert_eq!(after_inner.allowed.as_ref().unwrap(), &vec!["grep"]);
    }

    #[test]
    fn test_constraint_name_shorthand_narrows() {
        // <tool name="bash"> should set allowed = ["bash"]
        let tool_name = ToolDirective { name: Some("bash".into()), allow: None, deny: None };
        let root = ToolConstraints::default();
        let (after, warnings) = root.apply(&tool_name);
        assert!(warnings.is_empty());
        assert_eq!(after.allowed.as_ref().unwrap(), &vec!["bash"]);
    }

    #[test]
    fn test_constraint_name_shorthand_nested_warns() {
        // <tool name="bash"> → <tool allow="grep"> should warn (grep not in bash-only scope)
        let tool_name = ToolDirective { name: Some("bash".into()), allow: None, deny: None };
        let tool_inner = ToolDirective { name: None, allow: Some("grep".into()), deny: None };

        let root = ToolConstraints::default();
        let (after_name, _) = root.apply(&tool_name);
        let (after_inner, warnings) = after_name.apply(&tool_inner);

        assert_eq!(warnings.len(), 1, "expected warning about grep: {:?}", warnings);
        assert!(warnings[0].contains("grep"));
        assert!(after_inner.allowed.as_ref().unwrap().is_empty(),
            "grep should not be in effective: {:?}", after_inner.allowed);
    }

    #[test]
    fn test_constraint_empty_allow_filtered() {
        // allow="" should result in empty effective list, not [""]
        let tool = ToolDirective { name: None, allow: Some("".into()), deny: None };
        let root = ToolConstraints::default();
        let (after, _) = root.apply(&tool);
        assert!(after.allowed.as_ref().unwrap().is_empty(),
            "empty allow should not create ghost entries: {:?}", after.allowed);
    }

    #[test]
    fn test_constraint_empty_deny_filtered() {
        // deny="" should not add empty string to denied list
        let tool = ToolDirective { name: None, allow: None, deny: Some("".into()) };
        let root = ToolConstraints::default();
        let (after, _) = root.apply(&tool);
        assert!(after.denied.is_empty(),
            "empty deny should not create ghost entries: {:?}", after.denied);
    }

    #[test]
    fn test_constraint_name_plus_allow_warns() {
        // <tool name="bash" allow="grep"> should warn about redundant name
        let doc = parse(r#"<tool name="bash" allow="grep">x</tool>"#).unwrap();
        let errors = validate(&doc.nodes);
        let warnings: Vec<_> = errors.iter().filter(|e| e.severity == Severity::Warning).collect();
        assert!(warnings.iter().any(|w| w.message.contains("redundant")),
            "expected warning about redundant name: {:?}", warnings);
    }

    #[test]
    fn test_tool_on_failure_rejected() {
        let result = parse(r#"<tool name="bash" on-failure="skip">x</tool>"#);
        assert!(result.is_err(), "on-failure on <tool> should be rejected at parse level");
        let err = result.unwrap_err();
        assert!(err.message.contains("on-failure"), "error should mention on-failure: {}", err.message);
    }

    #[test]
    fn test_constraint_deny_beats_allow_effective() {
        // deny="bash" at root → inner allow="bash,grep" → effective should be grep only
        let tool_deny = ToolDirective { name: None, allow: None, deny: Some("bash".into()) };
        let tool_allow = ToolDirective { name: None, allow: Some("bash,grep".into()), deny: None };

        let root = ToolConstraints::default();
        let (after_deny, _) = root.apply(&tool_deny);
        let (after_allow, warnings) = after_deny.apply(&tool_allow);

        assert_eq!(warnings.len(), 1);
        assert_eq!(after_allow.allowed.as_ref().unwrap(), &vec!["grep"],
            "deny should prevent bash from entering effective: {:?}", after_allow.allowed);
    }

    // ── Iteration 1 bug fixes ──

    #[test]
    fn test_same_node_deny_and_allow_filters_correctly() {
        // Bug fix: allow block must use new.denied (after deny runs), not self.denied
        // This tests programmatic AST — parser rejects allow+deny on same node,
        // but apply() must be correct as a defence-in-depth invariant.
        let tool = ToolDirective { name: None, allow: Some("bash,grep".into()), deny: Some("bash".into()) };
        let root = ToolConstraints::default();
        let (after, _) = root.apply(&tool);
        assert!(!after.allowed.as_ref().unwrap().contains(&"bash".to_string()),
            "bash should be excluded by same-node deny: {:?}", after.allowed);
        assert_eq!(after.allowed.as_ref().unwrap(), &vec!["grep"]);
    }

    #[test]
    fn test_name_plus_deny_narrows_and_denies() {
        // Bug fix: name+deny should not skip name narrowing
        // <tool name="grep" deny="bash"> should narrow to ["grep"] and deny bash
        let tool = ToolDirective { name: Some("grep".into()), allow: None, deny: Some("bash".into()) };
        let root = ToolConstraints::default();
        let (after, _) = root.apply(&tool);
        assert_eq!(after.allowed.as_ref().unwrap(), &vec!["grep"],
            "name should narrow even when deny is present: {:?}", after.allowed);
        assert!(after.denied.contains(&"bash".to_string()));
    }

    #[test]
    fn test_name_deny_self_contradictory() {
        // <tool name="bash" deny="bash"> — name requests bash but deny blocks it
        let tool = ToolDirective { name: Some("bash".into()), allow: None, deny: Some("bash".into()) };
        let root = ToolConstraints::default();
        let (after, warnings) = root.apply(&tool);
        assert!(after.allowed.as_ref().unwrap().is_empty(),
            "self-contradictory name+deny should produce empty effective: {:?}", after.allowed);
        assert!(warnings.iter().any(|w| w.contains("bash") && w.contains("denied")),
            "should warn about bash being denied: {:?}", warnings);
    }

    // ── Iteration 1 edge case tests ──

    #[test]
    fn test_empty_parent_allow_intersect_child() {
        // Empty parent allow ∩ child allow = empty (security boundary)
        let tool_empty = ToolDirective { name: None, allow: Some("".into()), deny: None };
        let tool_child = ToolDirective { name: None, allow: Some("bash".into()), deny: None };
        let root = ToolConstraints::default();
        let (after_empty, _) = root.apply(&tool_empty);
        assert!(after_empty.allowed.as_ref().unwrap().is_empty());
        let (after_child, warnings) = after_empty.apply(&tool_child);
        assert!(after_child.allowed.as_ref().unwrap().is_empty(),
            "empty ∩ anything should be empty: {:?}", after_child.allowed);
        assert_eq!(warnings.len(), 1, "bash should warn as outside allow-list: {:?}", warnings);
    }

    #[test]
    fn test_allow_intersection_state_not_just_warnings() {
        // Verify effective state for allow∩allow (no deny involved)
        let outer = ToolDirective { name: None, allow: Some("bash,view".into()), deny: None };
        let inner = ToolDirective { name: None, allow: Some("bash,grep".into()), deny: None };
        let root = ToolConstraints::default();
        let (after_outer, _) = root.apply(&outer);
        let (after_inner, warnings) = after_outer.apply(&inner);
        assert_eq!(after_inner.allowed.as_ref().unwrap(), &vec!["bash"],
            "intersection should be bash only: {:?}", after_inner.allowed);
        assert_eq!(warnings.len(), 1, "grep should warn: {:?}", warnings);
    }

    #[test]
    fn test_stacked_deny_union_three_levels() {
        // deny="bash" → deny="grep" → allow="bash,grep,view" → effective = ["view"]
        let d1 = ToolDirective { name: None, allow: None, deny: Some("bash".into()) };
        let d2 = ToolDirective { name: None, allow: None, deny: Some("grep".into()) };
        let a3 = ToolDirective { name: None, allow: Some("bash,grep,view".into()), deny: None };
        let root = ToolConstraints::default();
        let (s1, _) = root.apply(&d1);
        let (s2, _) = s1.apply(&d2);
        let (s3, warnings) = s2.apply(&a3);
        assert_eq!(s3.allowed.as_ref().unwrap(), &vec!["view"],
            "only view should survive stacked denies: {:?}", s3.allowed);
        assert_eq!(warnings.len(), 2, "bash and grep should warn: {:?}", warnings);
    }

    #[test]
    fn test_whitespace_comma_edge_cases() {
        // "bash, , grep" should parse as ["bash", "grep"], filtering empty entries
        let tool = ToolDirective { name: None, allow: Some("bash, , grep".into()), deny: None };
        let root = ToolConstraints::default();
        let (after, _) = root.apply(&tool);
        assert_eq!(after.allowed.as_ref().unwrap(), &vec!["bash", "grep"],
            "empty entries should be filtered: {:?}", after.allowed);
    }
}
