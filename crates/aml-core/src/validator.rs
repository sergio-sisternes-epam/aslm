use crate::ast::{DirectiveKind, Node, NodeKind, Span};

/// Validation error with source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub message: String,
    pub span: Option<Span>,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(span) = self.span {
            write!(f, "[{}..{}] {}", span.start, span.end, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

/// Validate a parsed AST for semantic correctness.
pub fn validate(nodes: &[Node]) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    for node in nodes {
        validate_node(node, &mut errors);
    }
    errors
}

fn validate_node(node: &Node, errors: &mut Vec<ValidationError>) {
    match node {
        Node::Text(_) => {}
        Node::Directive {
            kind,
            children,
            span,
        } => {
            validate_directive(kind, *span, errors);

            // Recurse into children
            for child in children {
                validate_node(child, errors);
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
                        });
                    }
                    if matches!(child, Node::Directive { .. }) {
                        errors.push(ValidationError {
                            message: "definition nodes must not contain directives".to_string(),
                            span: child.span(),
                        });
                    }
                }
            }

            // Recurse into children
            for child in children {
                validate_node(child, errors);
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
                });
            }
        }
        NodeKind::InterfaceDefinition { name, .. } => {
            if name.is_empty() {
                errors.push(ValidationError {
                    message: "interface definition must have a non-empty name".to_string(),
                    span: Some(span),
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
                });
            }
            if implements.is_empty() {
                errors.push(ValidationError {
                    message: "implementation definition must have a non-empty implements field"
                        .to_string(),
                    span: Some(span),
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
}
