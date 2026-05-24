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
    fn apply(&self, child: &ToolDirective) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let mut new = self.clone();

        // Union denies
        if let Some(deny) = &child.deny {
            for tool in deny.split(',').map(|s| s.trim().to_string()) {
                if !new.denied.contains(&tool) {
                    new.denied.push(tool);
                }
            }
        }

        // Intersect allows / check contradictions
        if let Some(allow) = &child.allow {
            let requested: Vec<String> = allow.split(',').map(|s| s.trim().to_string()).collect();

            // Warn if any requested tool is denied by an ancestor
            for tool in &requested {
                if self.denied.contains(tool) {
                    warnings.push(format!(
                        "tool '{tool}' is allowed here but denied by an ancestor <tool> directive"
                    ));
                }
            }

            // Warn if any requested tool is outside the ancestor's allow-list
            if let Some(parent_allowed) = &self.allowed {
                for tool in &requested {
                    if !parent_allowed.contains(tool) && !self.denied.contains(tool) {
                        warnings.push(format!(
                            "tool '{tool}' is allowed here but not in ancestor's allow-list"
                        ));
                    }
                }
            }

            // Compute effective allow: intersection with parent
            let effective = if let Some(parent_allowed) = &self.allowed {
                requested.iter()
                    .filter(|t| parent_allowed.contains(t))
                    .cloned()
                    .collect()
            } else {
                requested
            };
            new.allowed = Some(effective);
        }

        // Handle name shorthand (equivalent to allow="<name>")
        if let Some(name) = &child.name {
            if child.allow.is_none() && child.deny.is_none() {
                if self.denied.contains(name) {
                    warnings.push(format!(
                        "tool '{name}' is requested but denied by an ancestor <tool> directive"
                    ));
                }
                if let Some(parent_allowed) = &self.allowed {
                    if !parent_allowed.contains(name) {
                        warnings.push(format!(
                            "tool '{name}' is requested but not in ancestor's allow-list"
                        ));
                    }
                }
            }
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
}
