use crate::ast::{
    DirectiveKind, FieldDecl, IoDecl, Node, NodeDecl, NodeKind, NodeType, ParamDecl, ReturnDecl,
    SkillRef, Span, ToolConstraint, ToolDirective,
};
use crate::parser::BARE_ATTR_SENTINEL;

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
            write!(
                f,
                "[{}..{}] {}: {}",
                span.start, span.end, prefix, self.message
            )
        } else {
            write!(f, "{}: {}", prefix, self.message)
        }
    }
}

/// Split a comma-separated attribute into non-empty, trimmed, deduplicated tool names.
fn parse_tool_list(value: &str) -> Vec<String> {
    let mut seen = Vec::new();
    for raw in value.split(',') {
        let s = raw.trim().to_string();
        if !s.is_empty() && !seen.contains(&s) {
            seen.push(s);
        }
    }
    seen
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
    /// Union child deny-list into accumulated denies and clean inherited allows.
    fn apply_deny(&mut self, deny: &str) {
        for tool in parse_tool_list(deny) {
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
        let requested = parse_tool_list(allow);

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

        let effective = if let Some(parent_allowed) = &parent.allowed {
            requested
                .iter()
                .filter(|t| parent_allowed.contains(t) && !self.denied.contains(t))
                .cloned()
                .collect()
        } else {
            requested
                .into_iter()
                .filter(|t| !self.denied.contains(t))
                .collect()
        };
        self.allowed = Some(effective);

        warnings
    }

    /// Apply name shorthand as singleton allow-list, emitting warnings.
    fn apply_name(&mut self, name: &str, parent: &ToolConstraints) -> Vec<String> {
        let mut warnings = Vec::new();
        let name_s = name.to_string();

        if self.denied.contains(&name_s) {
            let source = if parent.denied.contains(&name_s) {
                "an ancestor"
            } else {
                "this"
            };
            warnings.push(format!(
                "tool '{name}' is requested but denied by {source} <tool> directive"
            ));
        }
        if let Some(parent_allowed) = &parent.allowed {
            if !parent_allowed.contains(&name_s) && !self.denied.contains(&name_s) {
                warnings.push(format!(
                    "tool '{name}' is requested but not in ancestor's allow-list"
                ));
            }
        }

        if !self.denied.contains(&name_s) {
            if let Some(parent_allowed) = &parent.allowed {
                if parent_allowed.contains(&name_s) {
                    self.allowed = Some(vec![name_s]);
                } else {
                    self.allowed = Some(vec![]);
                }
            } else {
                self.allowed = Some(vec![name_s]);
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
            warnings.push("<tool> 'name' is redundant when 'allow' is also set".to_string());
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

            // Compute new constraints if this is a valid tool directive.
            // Skip constraint derivation for errored tools (e.g. allow+deny)
            // to avoid spurious downstream warnings.
            let child_constraints = if let DirectiveKind::Tool(tool) = kind {
                let has_tool_error = tool.allow.is_some() && tool.deny.is_some();
                if has_tool_error {
                    constraints.clone()
                } else {
                    let (new_constraints, warnings) = constraints.apply(tool);
                    for warning in warnings {
                        errors.push(ValidationError {
                            message: warning,
                            span: Some(*span),
                            severity: Severity::Warning,
                        });
                    }
                    new_constraints
                }
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
            validate_kind(kind, children, *span, errors);

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

            if !matches!(kind, NodeKind::ContractDefinition { .. }) {
                for child in children {
                    validate_node(child, constraints, errors);
                }
            }
        }
    }
}

fn validate_kind(
    kind: &NodeKind,
    children: &[Node],
    span: Span,
    errors: &mut Vec<ValidationError>,
) {
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
        NodeKind::InterfaceDefinition {
            name,
            extends,
            legacy_implements,
            params,
            returns,
            reads,
            writes,
            skill_refs,
            tool_constraints,
            ..
        } => {
            if name.is_empty() {
                errors.push(ValidationError {
                    message: "interface definition must have a non-empty name".to_string(),
                    span: Some(span),
                    severity: Severity::Error,
                });
            }
            if let Some(ext) = extends {
                if ext.is_empty() {
                    errors.push(ValidationError {
                        message: "extends attribute must be a non-empty interface name".to_string(),
                        span: Some(span),
                        severity: Severity::Error,
                    });
                }
            }
            if let Some(leg) = legacy_implements {
                match extends {
                    Some(ext) if ext != leg => {
                        errors.push(ValidationError {
                            message: format!(
                                "interface definition has both extends=\"{ext}\" and implements=\"{leg}\"; use only extends= on interface nodes"
                            ),
                            span: Some(span),
                            severity: Severity::Error,
                        });
                    }
                    _ => {
                        errors.push(ValidationError {
                            message: format!(
                                "implements=\"{leg}\" on an interface definition is deprecated; use extends=\"{leg}\" instead"
                            ),
                            span: Some(span),
                            severity: Severity::Warning,
                        });
                    }
                }
            }
            validate_interface_declarations(params, returns, reads, writes, span, errors);
            validate_skill_refs(skill_refs, span, errors);
            validate_tool_constraints(tool_constraints, span, errors);
        }
        NodeKind::ImplementationDefinition {
            name,
            implements,
            nodes,
            skill_refs,
            ..
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
            validate_node_declarations(nodes, span, errors);
            validate_skill_refs(skill_refs, span, errors);
        }
        NodeKind::ContractDefinition {
            name,
            extends,
            fields,
            ..
        } => validate_contract_definition(name, extends, fields, children, span, errors),
    }
}

/// Valid scalar types for typed declarations.
const BASIC_DECL_TYPES: &[&str] = &["string", "enum", "number", "boolean", "path", "list"];
const CONTRACT_OBJECT_TYPES: &[&str] = &["object", "list"];

fn is_contract_reference(value: &str) -> bool {
    value
        .strip_prefix("contract:")
        .is_some_and(|name| !name.is_empty())
}

fn is_valid_decl_type(value: &str) -> bool {
    BASIC_DECL_TYPES.contains(&value) || is_contract_reference(value)
}

fn is_valid_field_type(value: &str) -> bool {
    is_valid_decl_type(value) || CONTRACT_OBJECT_TYPES.contains(&value)
}

fn is_scalar_field_type(value: &str) -> bool {
    matches!(value, "string" | "enum" | "number" | "boolean" | "path")
        || is_contract_reference(value)
}

fn bare_attribute_name<'a>(attrs: &'a [(&'a str, Option<&'a str>)]) -> Option<&'a str> {
    attrs
        .iter()
        .find_map(|(name, value)| (*value == Some(BARE_ATTR_SENTINEL)).then_some(*name))
}

fn push_invalid_bare_attribute(
    subject: &str,
    attr: &str,
    span: Span,
    errors: &mut Vec<ValidationError>,
) {
    errors.push(ValidationError {
        message: format!(
            "{subject} uses bare attribute syntax for '{attr}' (only 'required' may be bare)"
        ),
        span: Some(span),
        severity: Severity::Error,
    });
}

fn validate_contract_definition(
    name: &str,
    extends: &Option<String>,
    fields: &[FieldDecl],
    children: &[Node],
    span: Span,
    errors: &mut Vec<ValidationError>,
) {
    if name.is_empty() {
        errors.push(ValidationError {
            message: "contract definition must have a non-empty name".to_string(),
            span: Some(span),
            severity: Severity::Error,
        });
    }

    if let Some(parent) = extends {
        if parent.is_empty() {
            errors.push(ValidationError {
                message: "extends attribute must be a non-empty contract name".to_string(),
                span: Some(span),
                severity: Severity::Error,
            });
        }
    }

    for child in children {
        if matches!(
            child,
            Node::Skill {
                kind: NodeKind::Invocation { .. },
                ..
            }
        ) {
            errors.push(ValidationError {
                message: "contract definitions must not contain invocations".to_string(),
                span: child.span(),
                severity: Severity::Error,
            });
        }
        if matches!(child, Node::Directive { .. }) {
            errors.push(ValidationError {
                message: "directive tags are not allowed inside contract definitions".to_string(),
                span: child.span(),
                severity: Severity::Error,
            });
        }
    }

    validate_field_declarations(fields, errors);
}

fn validate_field_declarations(fields: &[FieldDecl], errors: &mut Vec<ValidationError>) {
    let mut seen_names = Vec::new();

    for field in fields {
        if field.name.is_empty() {
            errors.push(ValidationError {
                message: "field declaration must have a non-empty name".to_string(),
                span: Some(field.span),
                severity: Severity::Error,
            });
        }

        if seen_names.contains(&field.name) {
            errors.push(ValidationError {
                message: format!(
                    "duplicate field name '{}' in contract definition",
                    field.name
                ),
                span: Some(field.span),
                severity: Severity::Error,
            });
        } else {
            seen_names.push(field.name.clone());
        }

        if let Some(attr) = bare_attribute_name(&[
            ("type", field.field_type.as_deref()),
            ("default", field.default.as_deref()),
            ("values", field.values.as_deref()),
        ]) {
            push_invalid_bare_attribute(
                &format!("field '{}'", field.name),
                attr,
                field.span,
                errors,
            );
        }

        if let Some(field_type) = field.field_type.as_deref() {
            if field_type != BARE_ATTR_SENTINEL && !is_valid_field_type(field_type) {
                errors.push(ValidationError {
                    message: format!(
                        "invalid field type '{}' (expected one of: string, enum, number, boolean, path, list, object, contract:<name>)",
                        field_type
                    ),
                    span: Some(field.span),
                    severity: Severity::Error,
                });
            }

            if is_scalar_field_type(field_type) && !field.children.is_empty() {
                errors.push(ValidationError {
                    message: format!(
                        "field '{}' has children but type '{}' is scalar",
                        field.name, field_type
                    ),
                    span: Some(field.span),
                    severity: Severity::Error,
                });
            }
        }

        let is_enum = field.field_type.as_deref() == Some("enum");
        if is_enum && field.values.is_none() {
            errors.push(ValidationError {
                message: format!(
                    "field '{}' has type 'enum' but no 'values' attribute",
                    field.name
                ),
                span: Some(field.span),
                severity: Severity::Error,
            });
        }

        validate_field_declarations(&field.children, errors);
    }
}

fn validate_interface_declarations(
    params: &[ParamDecl],
    returns: &[ReturnDecl],
    reads: &Option<IoDecl>,
    writes: &Option<IoDecl>,
    span: Span,
    errors: &mut Vec<ValidationError>,
) {
    // Validate param declarations
    let mut seen_param_names = Vec::new();
    for p in params {
        // Unique param names
        if seen_param_names.contains(&p.name) {
            errors.push(ValidationError {
                message: format!("duplicate param name '{}' in interface definition", p.name),
                span: Some(p.span),
                severity: Severity::Error,
            });
        } else {
            seen_param_names.push(p.name.clone());
        }

        // Empty name
        if p.name.is_empty() {
            errors.push(ValidationError {
                message: "param declaration must have a non-empty name".to_string(),
                span: Some(p.span),
                severity: Severity::Error,
            });
        }

        if let Some(attr) = bare_attribute_name(&[
            ("type", p.param_type.as_deref()),
            ("default", p.default.as_deref()),
            ("values", p.values.as_deref()),
        ]) {
            push_invalid_bare_attribute(&format!("param '{}'", p.name), attr, p.span, errors);
        }

        // Valid type
        if let Some(ref t) = p.param_type {
            if t != BARE_ATTR_SENTINEL && !is_valid_decl_type(t) {
                errors.push(ValidationError {
                    message: format!(
                        "invalid param type '{}' (expected one of: string, enum, number, boolean, path, list, contract:<name>)",
                        t
                    ),
                    span: Some(p.span),
                    severity: Severity::Error,
                });
            }
        }

        let is_enum = p.param_type.as_deref() == Some("enum");

        // Enum requires values
        if is_enum && p.values.is_none() {
            errors.push(ValidationError {
                message: format!(
                    "param '{}' has type 'enum' but no 'values' attribute",
                    p.name
                ),
                span: Some(p.span),
                severity: Severity::Error,
            });
        }

        // Non-enum must not have values
        if !is_enum && p.values.is_some() && p.values.as_deref() != Some(BARE_ATTR_SENTINEL) {
            errors.push(ValidationError {
                message: format!(
                    "param '{}' has 'values' attribute but type is not 'enum'",
                    p.name
                ),
                span: Some(p.span),
                severity: Severity::Error,
            });
        }

        // Validate enum values are non-empty
        if let Some(ref values) = p.values {
            if values != BARE_ATTR_SENTINEL {
                let entries: Vec<&str> = values.split('|').map(str::trim).collect();
                if entries.iter().any(|v| v.is_empty()) {
                    errors.push(ValidationError {
                        message: format!("param '{}' has empty entries in 'values'", p.name),
                        span: Some(p.span),
                        severity: Severity::Warning,
                    });
                }
            }
        }

        // required + default is a warning
        if p.required == Some(true)
            && p.default.is_some()
            && p.default.as_deref() != Some(BARE_ATTR_SENTINEL)
        {
            errors.push(ValidationError {
                message: format!("param '{}' is required but has a default value", p.name),
                span: Some(p.span),
                severity: Severity::Warning,
            });
        }

        // Default must be a valid enum value
        if let (Some(ref default), Some(ref values)) = (&p.default, &p.values) {
            if default != BARE_ATTR_SENTINEL && values != BARE_ATTR_SENTINEL {
                let entries: Vec<&str> = values.split('|').map(str::trim).collect();
                if !entries.contains(&default.as_str()) {
                    errors.push(ValidationError {
                        message: format!(
                            "param '{}' default '{}' is not one of the allowed values: {}",
                            p.name, default, values
                        ),
                        span: Some(p.span),
                        severity: Severity::Error,
                    });
                }
            }
        }

        // Default type compatibility for boolean
        if p.param_type.as_deref() == Some("boolean") {
            if let Some(ref default) = p.default {
                if default != BARE_ATTR_SENTINEL && default != "true" && default != "false" {
                    errors.push(ValidationError {
                        message: format!(
                            "param '{}' has type 'boolean' but default '{}' is not 'true' or 'false'",
                            p.name, default
                        ),
                        span: Some(p.span),
                        severity: Severity::Error,
                    });
                }
            }
        }

        // Default type compatibility for number
        if p.param_type.as_deref() == Some("number") {
            if let Some(ref default) = p.default {
                if default != BARE_ATTR_SENTINEL && default.parse::<f64>().is_err() {
                    errors.push(ValidationError {
                        message: format!(
                            "param '{}' has type 'number' but default '{}' is not a valid number",
                            p.name, default
                        ),
                        span: Some(p.span),
                        severity: Severity::Error,
                    });
                }
            }
        }
    }

    // Validate return declarations
    let mut seen_return_names = Vec::new();
    for r in returns {
        if seen_return_names.contains(&r.name) {
            errors.push(ValidationError {
                message: format!(
                    "duplicate returns name '{}' in interface definition",
                    r.name
                ),
                span: Some(r.span),
                severity: Severity::Error,
            });
        } else {
            seen_return_names.push(r.name.clone());
        }

        if r.name.is_empty() {
            errors.push(ValidationError {
                message: "returns declaration must have a non-empty name".to_string(),
                span: Some(r.span),
                severity: Severity::Error,
            });
        }

        if let Some(attr) = bare_attribute_name(&[
            ("type", r.return_type.as_deref()),
            ("values", r.values.as_deref()),
        ]) {
            push_invalid_bare_attribute(&format!("returns '{}'", r.name), attr, r.span, errors);
        }

        if let Some(ref t) = r.return_type {
            if t != BARE_ATTR_SENTINEL && !is_valid_decl_type(t) {
                errors.push(ValidationError {
                    message: format!(
                        "invalid returns type '{}' (expected one of: string, enum, number, boolean, path, list, contract:<name>)",
                        t
                    ),
                    span: Some(r.span),
                    severity: Severity::Error,
                });
            }
        }

        let is_enum = r.return_type.as_deref() == Some("enum");

        if is_enum && r.values.is_none() {
            errors.push(ValidationError {
                message: format!(
                    "returns '{}' has type 'enum' but no 'values' attribute",
                    r.name
                ),
                span: Some(r.span),
                severity: Severity::Error,
            });
        }

        if !is_enum && r.values.is_some() && r.values.as_deref() != Some(BARE_ATTR_SENTINEL) {
            errors.push(ValidationError {
                message: format!(
                    "returns '{}' has 'values' attribute but type is not 'enum'",
                    r.name
                ),
                span: Some(r.span),
                severity: Severity::Error,
            });
        }
    }

    // Validate I/O declarations have non-empty patterns
    if let Some(ref io) = reads {
        if io.patterns.is_empty() {
            errors.push(ValidationError {
                message: "reads declaration has no patterns".to_string(),
                span: Some(io.span),
                severity: Severity::Warning,
            });
        }
    }
    if let Some(ref io) = writes {
        if io.patterns.is_empty() {
            errors.push(ValidationError {
                message: "writes declaration has no patterns".to_string(),
                span: Some(io.span),
                severity: Severity::Warning,
            });
        }
    }

    // Multiple reads/writes are prevented at the parser level (last one wins),
    // but we validate the presence is meaningful.
    let _ = span; // used in the destructuring match arm
}

/// Validate skill ref declarations inside an interface body.
fn validate_skill_refs(skill_refs: &[SkillRef], _span: Span, errors: &mut Vec<ValidationError>) {
    for sr in skill_refs {
        if sr.ref_name.is_empty() {
            errors.push(ValidationError {
                message: "skill ref must have a non-empty name".to_string(),
                span: Some(sr.span),
                severity: Severity::Error,
            });
        }
    }
}

/// Validate tool constraint declarations inside an interface body.
fn validate_tool_constraints(
    constraints: &[ToolConstraint],
    _span: Span,
    errors: &mut Vec<ValidationError>,
) {
    for tc in constraints {
        if !tc.allow.is_empty() && !tc.deny.is_empty() {
            errors.push(ValidationError {
                message: "<tool> in interface cannot have both 'allow' and 'deny'".to_string(),
                span: Some(tc.span),
                severity: Severity::Error,
            });
        }
        if tc.allow.is_empty() && tc.deny.is_empty() {
            errors.push(ValidationError {
                message: "<tool> in interface must have 'allow' or 'deny'".to_string(),
                span: Some(tc.span),
                severity: Severity::Error,
            });
        }
    }
}

/// Validate node declarations inside an implementation body.
fn validate_node_declarations(nodes: &[NodeDecl], _span: Span, errors: &mut Vec<ValidationError>) {
    let mut seen_names = Vec::new();
    for node in nodes {
        if node.name.is_empty() {
            errors.push(ValidationError {
                message: "node declaration must have a non-empty name".to_string(),
                span: Some(node.span),
                severity: Severity::Error,
            });
        }
        if seen_names.contains(&node.name) {
            errors.push(ValidationError {
                message: format!("duplicate node name '{}' in implementation", node.name),
                span: Some(node.span),
                severity: Severity::Error,
            });
        } else {
            seen_names.push(node.name.clone());
        }

        // tool-type nodes should declare <tool use="...">
        if node.node_type == NodeType::Tool && node.tool_use.is_none() {
            errors.push(ValidationError {
                message: format!(
                    "node '{}' has type 'tool' but no <tool use=\"...\"/> declaration",
                    node.name
                ),
                span: Some(node.span),
                severity: Severity::Warning,
            });
        }

        // prompt-type nodes should not have <tool use="...">
        if node.node_type == NodeType::Prompt && node.tool_use.is_some() {
            errors.push(ValidationError {
                message: format!(
                    "node '{}' has type 'prompt' but declares <tool use=\"...\"/>",
                    node.name
                ),
                span: Some(node.span),
                severity: Severity::Warning,
            });
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
        let doc =
            parse(r#"<skill define="interface" name="outer"><tool name="bash">x</tool></skill>"#)
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
        assert!(
            result.is_err(),
            "invalid on-failure value should cause parse error"
        );
    }

    // ── Tool narrowing tests ────────────────────────────────────────

    #[test]
    fn test_tool_narrowing_warn_denied_tool() {
        let doc = parse(r#"<tool deny="bash"><tool allow="bash,grep">x</tool></tool>"#).unwrap();
        let errors = validate(&doc.nodes);
        let warnings: Vec<_> = errors
            .iter()
            .filter(|e| e.severity == Severity::Warning)
            .collect();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("bash"));
        assert!(warnings[0].message.contains("denied"));
    }

    #[test]
    fn test_tool_narrowing_warn_outside_allow() {
        let doc =
            parse(r#"<tool allow="grep,view"><tool allow="grep,bash">x</tool></tool>"#).unwrap();
        let errors = validate(&doc.nodes);
        let warnings: Vec<_> = errors
            .iter()
            .filter(|e| e.severity == Severity::Warning)
            .collect();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("bash"));
        assert!(warnings[0].message.contains("allow-list"));
    }

    #[test]
    fn test_tool_narrowing_no_warning_when_valid() {
        let doc = parse(r#"<tool allow="grep,view,bash"><tool allow="grep,view">x</tool></tool>"#)
            .unwrap();
        let errors = validate(&doc.nodes);
        assert!(
            errors.is_empty(),
            "valid narrowing should produce no warnings: {:?}",
            errors
        );
    }

    #[test]
    fn test_tool_narrowing_name_shorthand_denied() {
        let doc = parse(r#"<tool deny="bash"><tool name="bash">x</tool></tool>"#).unwrap();
        let errors = validate(&doc.nodes);
        let warnings: Vec<_> = errors
            .iter()
            .filter(|e| e.severity == Severity::Warning)
            .collect();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("bash"));
    }

    #[test]
    fn test_tool_narrowing_triple_nesting() {
        let doc = parse(
            r#"<tool allow="grep,view,bash"><tool deny="bash"><tool allow="grep,bash">x</tool></tool></tool>"#
        ).unwrap();
        let errors = validate(&doc.nodes);
        let warnings: Vec<_> = errors
            .iter()
            .filter(|e| e.severity == Severity::Warning)
            .collect();
        // Inner allow="grep,bash" requests bash which was denied at level 2
        assert_eq!(
            warnings.len(),
            1,
            "expected warning about bash: {:?}",
            warnings
        );
        assert!(warnings[0].message.contains("bash"));
    }

    // ── Constraint state tests (verify effective allow-set, not just warnings) ──

    #[test]
    fn test_constraint_deny_removes_from_allowed() {
        // After deny="bash", a grandchild allow="bash" should NOT have bash in effective
        let tool_outer = ToolDirective {
            name: None,
            allow: Some("grep,bash,view".into()),
            deny: None,
        };
        let tool_deny = ToolDirective {
            name: None,
            allow: None,
            deny: Some("bash".into()),
        };
        let tool_inner = ToolDirective {
            name: None,
            allow: Some("grep,bash".into()),
            deny: None,
        };

        let root = ToolConstraints::default();
        let (after_outer, w1) = root.apply(&tool_outer);
        assert!(w1.is_empty());
        assert_eq!(
            after_outer.allowed.as_ref().unwrap(),
            &vec!["grep", "bash", "view"]
        );

        let (after_deny, w2) = after_outer.apply(&tool_deny);
        assert!(w2.is_empty());
        // bash should be removed from allowed after deny
        assert!(
            !after_deny
                .allowed
                .as_ref()
                .unwrap()
                .contains(&"bash".to_string()),
            "bash should be removed from allowed: {:?}",
            after_deny.allowed
        );

        let (after_inner, w3) = after_deny.apply(&tool_inner);
        // bash is denied — should warn and NOT be in effective
        assert_eq!(w3.len(), 1, "expected 1 warning: {:?}", w3);
        assert!(
            !after_inner
                .allowed
                .as_ref()
                .unwrap()
                .contains(&"bash".to_string()),
            "bash should NOT be in effective allow-list: {:?}",
            after_inner.allowed
        );
        assert_eq!(after_inner.allowed.as_ref().unwrap(), &vec!["grep"]);
    }

    #[test]
    fn test_constraint_name_shorthand_narrows() {
        // <tool name="bash"> should set allowed = ["bash"]
        let tool_name = ToolDirective {
            name: Some("bash".into()),
            allow: None,
            deny: None,
        };
        let root = ToolConstraints::default();
        let (after, warnings) = root.apply(&tool_name);
        assert!(warnings.is_empty());
        assert_eq!(after.allowed.as_ref().unwrap(), &vec!["bash"]);
    }

    #[test]
    fn test_constraint_name_shorthand_nested_warns() {
        // <tool name="bash"> → <tool allow="grep"> should warn (grep not in bash-only scope)
        let tool_name = ToolDirective {
            name: Some("bash".into()),
            allow: None,
            deny: None,
        };
        let tool_inner = ToolDirective {
            name: None,
            allow: Some("grep".into()),
            deny: None,
        };

        let root = ToolConstraints::default();
        let (after_name, _) = root.apply(&tool_name);
        let (after_inner, warnings) = after_name.apply(&tool_inner);

        assert_eq!(
            warnings.len(),
            1,
            "expected warning about grep: {:?}",
            warnings
        );
        assert!(warnings[0].contains("grep"));
        assert!(
            after_inner.allowed.as_ref().unwrap().is_empty(),
            "grep should not be in effective: {:?}",
            after_inner.allowed
        );
    }

    #[test]
    fn test_constraint_empty_allow_filtered() {
        // allow="" should result in empty effective list, not [""]
        let tool = ToolDirective {
            name: None,
            allow: Some("".into()),
            deny: None,
        };
        let root = ToolConstraints::default();
        let (after, _) = root.apply(&tool);
        assert!(
            after.allowed.as_ref().unwrap().is_empty(),
            "empty allow should not create ghost entries: {:?}",
            after.allowed
        );
    }

    #[test]
    fn test_constraint_empty_deny_filtered() {
        // deny="" should not add empty string to denied list
        let tool = ToolDirective {
            name: None,
            allow: None,
            deny: Some("".into()),
        };
        let root = ToolConstraints::default();
        let (after, _) = root.apply(&tool);
        assert!(
            after.denied.is_empty(),
            "empty deny should not create ghost entries: {:?}",
            after.denied
        );
    }

    #[test]
    fn test_constraint_name_plus_allow_warns() {
        // <tool name="bash" allow="grep"> should warn about redundant name
        let doc = parse(r#"<tool name="bash" allow="grep">x</tool>"#).unwrap();
        let errors = validate(&doc.nodes);
        let warnings: Vec<_> = errors
            .iter()
            .filter(|e| e.severity == Severity::Warning)
            .collect();
        assert!(
            warnings.iter().any(|w| w.message.contains("redundant")),
            "expected warning about redundant name: {:?}",
            warnings
        );
    }

    #[test]
    fn test_tool_on_failure_rejected() {
        let result = parse(r#"<tool name="bash" on-failure="skip">x</tool>"#);
        assert!(
            result.is_err(),
            "on-failure on <tool> should be rejected at parse level"
        );
        let err = result.unwrap_err();
        assert!(
            err.message.contains("on-failure"),
            "error should mention on-failure: {}",
            err.message
        );
    }

    #[test]
    fn test_constraint_deny_beats_allow_effective() {
        // deny="bash" at root → inner allow="bash,grep" → effective should be grep only
        let tool_deny = ToolDirective {
            name: None,
            allow: None,
            deny: Some("bash".into()),
        };
        let tool_allow = ToolDirective {
            name: None,
            allow: Some("bash,grep".into()),
            deny: None,
        };

        let root = ToolConstraints::default();
        let (after_deny, _) = root.apply(&tool_deny);
        let (after_allow, warnings) = after_deny.apply(&tool_allow);

        assert_eq!(warnings.len(), 1);
        assert_eq!(
            after_allow.allowed.as_ref().unwrap(),
            &vec!["grep"],
            "deny should prevent bash from entering effective: {:?}",
            after_allow.allowed
        );
    }

    // ── Iteration 1 bug fixes ──

    #[test]
    fn test_same_node_deny_and_allow_filters_correctly() {
        // Bug fix: allow block must use new.denied (after deny runs), not self.denied
        // This tests programmatic AST — parser rejects allow+deny on same node,
        // but apply() must be correct as a defence-in-depth invariant.
        let tool = ToolDirective {
            name: None,
            allow: Some("bash,grep".into()),
            deny: Some("bash".into()),
        };
        let root = ToolConstraints::default();
        let (after, _) = root.apply(&tool);
        assert!(
            !after
                .allowed
                .as_ref()
                .unwrap()
                .contains(&"bash".to_string()),
            "bash should be excluded by same-node deny: {:?}",
            after.allowed
        );
        assert_eq!(after.allowed.as_ref().unwrap(), &vec!["grep"]);
    }

    #[test]
    fn test_name_plus_deny_narrows_and_denies() {
        // Bug fix: name+deny should not skip name narrowing
        // <tool name="grep" deny="bash"> should narrow to ["grep"] and deny bash
        let tool = ToolDirective {
            name: Some("grep".into()),
            allow: None,
            deny: Some("bash".into()),
        };
        let root = ToolConstraints::default();
        let (after, _) = root.apply(&tool);
        assert_eq!(
            after.allowed.as_ref().unwrap(),
            &vec!["grep"],
            "name should narrow even when deny is present: {:?}",
            after.allowed
        );
        assert!(after.denied.contains(&"bash".to_string()));
    }

    #[test]
    fn test_name_deny_self_contradictory() {
        // <tool name="bash" deny="bash"> — name requests bash but deny blocks it
        let tool = ToolDirective {
            name: Some("bash".into()),
            allow: None,
            deny: Some("bash".into()),
        };
        let root = ToolConstraints::default();
        let (after, warnings) = root.apply(&tool);
        assert!(
            after.allowed.as_ref().unwrap().is_empty(),
            "self-contradictory name+deny should produce empty effective: {:?}",
            after.allowed
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("bash") && w.contains("denied")),
            "should warn about bash being denied: {:?}",
            warnings
        );
    }

    // ── Iteration 1 edge case tests ──

    #[test]
    fn test_empty_parent_allow_intersect_child() {
        // Empty parent allow ∩ child allow = empty (security boundary)
        let tool_empty = ToolDirective {
            name: None,
            allow: Some("".into()),
            deny: None,
        };
        let tool_child = ToolDirective {
            name: None,
            allow: Some("bash".into()),
            deny: None,
        };
        let root = ToolConstraints::default();
        let (after_empty, _) = root.apply(&tool_empty);
        assert!(after_empty.allowed.as_ref().unwrap().is_empty());
        let (after_child, warnings) = after_empty.apply(&tool_child);
        assert!(
            after_child.allowed.as_ref().unwrap().is_empty(),
            "empty ∩ anything should be empty: {:?}",
            after_child.allowed
        );
        assert_eq!(
            warnings.len(),
            1,
            "bash should warn as outside allow-list: {:?}",
            warnings
        );
    }

    #[test]
    fn test_allow_intersection_state_not_just_warnings() {
        // Verify effective state for allow∩allow (no deny involved)
        let outer = ToolDirective {
            name: None,
            allow: Some("bash,view".into()),
            deny: None,
        };
        let inner = ToolDirective {
            name: None,
            allow: Some("bash,grep".into()),
            deny: None,
        };
        let root = ToolConstraints::default();
        let (after_outer, _) = root.apply(&outer);
        let (after_inner, warnings) = after_outer.apply(&inner);
        assert_eq!(
            after_inner.allowed.as_ref().unwrap(),
            &vec!["bash"],
            "intersection should be bash only: {:?}",
            after_inner.allowed
        );
        assert_eq!(warnings.len(), 1, "grep should warn: {:?}", warnings);
    }

    #[test]
    fn test_stacked_deny_union_three_levels() {
        // deny="bash" → deny="grep" → allow="bash,grep,view" → effective = ["view"]
        let d1 = ToolDirective {
            name: None,
            allow: None,
            deny: Some("bash".into()),
        };
        let d2 = ToolDirective {
            name: None,
            allow: None,
            deny: Some("grep".into()),
        };
        let a3 = ToolDirective {
            name: None,
            allow: Some("bash,grep,view".into()),
            deny: None,
        };
        let root = ToolConstraints::default();
        let (s1, _) = root.apply(&d1);
        let (s2, _) = s1.apply(&d2);
        let (s3, warnings) = s2.apply(&a3);
        assert_eq!(
            s3.allowed.as_ref().unwrap(),
            &vec!["view"],
            "only view should survive stacked denies: {:?}",
            s3.allowed
        );
        assert_eq!(
            warnings.len(),
            2,
            "bash and grep should warn: {:?}",
            warnings
        );
    }

    #[test]
    fn test_whitespace_comma_edge_cases() {
        // "bash, , grep" should parse as ["bash", "grep"], filtering empty entries
        let tool = ToolDirective {
            name: None,
            allow: Some("bash, , grep".into()),
            deny: None,
        };
        let root = ToolConstraints::default();
        let (after, _) = root.apply(&tool);
        assert_eq!(
            after.allowed.as_ref().unwrap(),
            &vec!["bash", "grep"],
            "empty entries should be filtered: {:?}",
            after.allowed
        );
    }

    // ── Iteration 2 tests ──

    #[test]
    fn test_parse_tool_list_deduplicates() {
        let result = parse_tool_list("bash,grep,bash,view,grep");
        assert_eq!(
            result,
            vec!["bash", "grep", "view"],
            "parse_tool_list should deduplicate: {:?}",
            result
        );
    }

    #[test]
    fn test_duplicate_allow_no_duplicate_warnings() {
        // deny="bash" → allow="bash,bash" should produce exactly 1 warning, not 2
        let parent = ToolConstraints {
            allowed: None,
            denied: vec!["bash".into()],
        };
        let tool = ToolDirective {
            name: None,
            allow: Some("bash,bash".into()),
            deny: None,
        };
        let (result, warnings) = parent.apply(&tool);
        assert_eq!(
            warnings.len(),
            1,
            "duplicate allow should not produce duplicate warnings: {:?}",
            warnings
        );
        assert!(
            result.allowed.as_ref().unwrap().is_empty(),
            "bash is denied, effective should be empty"
        );
    }

    #[test]
    fn test_four_level_nesting() {
        // allow="bash,grep,view,sql,web_search" → deny="web_search" → allow="bash,grep" → name="grep"
        let l1 = ToolDirective {
            name: None,
            allow: Some("bash,grep,view,sql,web_search".into()),
            deny: None,
        };
        let l2 = ToolDirective {
            name: None,
            allow: None,
            deny: Some("web_search".into()),
        };
        let l3 = ToolDirective {
            name: None,
            allow: Some("bash,grep".into()),
            deny: None,
        };
        let l4 = ToolDirective {
            name: Some("grep".into()),
            allow: None,
            deny: None,
        };

        let root = ToolConstraints::default();
        let (s1, w1) = root.apply(&l1);
        assert!(w1.is_empty());
        let (s2, w2) = s1.apply(&l2);
        assert!(w2.is_empty());
        let (s3, w3) = s2.apply(&l3);
        assert!(w3.is_empty());
        let (s4, w4) = s3.apply(&l4);
        assert!(w4.is_empty());
        assert_eq!(
            s4.allowed.as_ref().unwrap(),
            &vec!["grep"],
            "4-level narrowing should end with grep only: {:?}",
            s4.allowed
        );
    }

    #[test]
    fn test_name_then_deny_conflict_at_depth() {
        // allow="a,b,c" → name="a" → deny="a" → effective should be empty
        let l1 = ToolDirective {
            name: None,
            allow: Some("a,b,c".into()),
            deny: None,
        };
        let l2 = ToolDirective {
            name: Some("a".into()),
            allow: None,
            deny: None,
        };
        let l3 = ToolDirective {
            name: None,
            allow: None,
            deny: Some("a".into()),
        };

        let root = ToolConstraints::default();
        let (s1, _) = root.apply(&l1);
        let (s2, _) = s1.apply(&l2);
        assert_eq!(s2.allowed.as_ref().unwrap(), &vec!["a"]);
        let (s3, _) = s2.apply(&l3);
        assert!(
            s3.allowed.as_ref().unwrap().is_empty(),
            "deny at level 3 should remove a from allowed: {:?}",
            s3.allowed
        );
        assert!(s3.denied.contains(&"a".to_string()));
    }

    #[test]
    fn test_parent_allow_with_name_narrowing() {
        // allow="bash,grep" → name="bash" → effective should be ["bash"]
        let l1 = ToolDirective {
            name: None,
            allow: Some("bash,grep".into()),
            deny: None,
        };
        let l2 = ToolDirective {
            name: Some("bash".into()),
            allow: None,
            deny: None,
        };

        let root = ToolConstraints::default();
        let (s1, _) = root.apply(&l1);
        let (s2, warnings) = s1.apply(&l2);
        assert!(
            warnings.is_empty(),
            "bash is in parent allow-list, no warning: {:?}",
            warnings
        );
        assert_eq!(
            s2.allowed.as_ref().unwrap(),
            &vec!["bash"],
            "name should narrow to bash only: {:?}",
            s2.allowed
        );
    }

    #[test]
    fn test_errored_tool_does_not_propagate_constraints() {
        // <tool allow="bash" deny="bash"> is an error — children should not get derived constraints
        let doc =
            parse(r#"<tool allow="bash" deny="bash"><tool allow="grep">x</tool></tool>"#).unwrap();
        let errors = validate(&doc.nodes);
        let hard_errors: Vec<_> = errors
            .iter()
            .filter(|e| e.severity == Severity::Error)
            .collect();
        let warnings: Vec<_> = errors
            .iter()
            .filter(|e| e.severity == Severity::Warning)
            .collect();
        assert_eq!(
            hard_errors.len(),
            1,
            "should have mutual exclusivity error: {:?}",
            errors
        );
        // The inner <tool allow="grep"> should NOT produce a spurious "not in ancestor's allow-list" warning
        assert!(
            warnings.is_empty(),
            "errored parent should not produce downstream warnings: {:?}",
            warnings
        );
    }

    #[test]
    fn test_apply_name_no_double_warning() {
        // When name is denied AND not in parent allow, should produce only 1 warning (denied)
        let parent = ToolConstraints {
            allowed: Some(vec!["bash".into()]),
            denied: vec!["grep".into()],
        };
        let tool = ToolDirective {
            name: Some("grep".into()),
            allow: None,
            deny: None,
        };
        let (_, warnings) = parent.apply(&tool);
        assert_eq!(
            warnings.len(),
            1,
            "denied name should produce exactly 1 warning, not 2: {:?}",
            warnings
        );
        assert!(warnings[0].contains("denied"));
    }

    #[test]
    fn test_same_node_name_deny_says_this_directive() {
        // <tool name="bash" deny="bash"> — warning should say "this" not "ancestor"
        let root = ToolConstraints::default();
        let tool = ToolDirective {
            name: Some("bash".into()),
            allow: None,
            deny: Some("bash".into()),
        };
        let (result, warnings) = root.apply(&tool);
        assert_eq!(warnings.len(), 1, "expected 1 warning: {:?}", warnings);
        assert!(
            warnings[0].contains("this <tool> directive"),
            "should say 'this' not 'ancestor': {}",
            warnings[0]
        );
        assert!(result.allowed.as_ref().unwrap().is_empty());
    }

    #[test]
    fn test_ancestor_deny_says_ancestor_directive() {
        // parent deny="bash" → child name="bash" — warning should say "ancestor"
        let parent = ToolConstraints {
            allowed: None,
            denied: vec!["bash".into()],
        };
        let tool = ToolDirective {
            name: Some("bash".into()),
            allow: None,
            deny: None,
        };
        let (_, warnings) = parent.apply(&tool);
        assert_eq!(warnings.len(), 1, "expected 1 warning: {:?}", warnings);
        assert!(
            warnings[0].contains("an ancestor"),
            "should say 'ancestor': {}",
            warnings[0]
        );
    }

    // ── Typed interface declaration validation tests ───────────────────────

    #[test]
    fn test_valid_typed_interface() {
        let doc = parse(
            r#"<skill define="interface" name="test">
  <param name="q" type="string" required="true">Question</param>
  <returns name="a" type="string">Answer</returns>
  <reads>data/*.md</reads>
  <writes>output/*.md</writes>
</skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        let real_errors: Vec<_> = errors
            .iter()
            .filter(|e| e.severity == Severity::Error)
            .collect();
        assert!(
            real_errors.is_empty(),
            "unexpected errors: {:?}",
            real_errors
        );
    }

    #[test]
    fn test_duplicate_param_names() {
        let doc = parse(
            r#"<skill define="interface" name="test">
  <param name="x" type="string">First</param>
  <param name="x" type="number">Duplicate</param>
</skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("duplicate param name")));
    }

    #[test]
    fn test_invalid_param_type() {
        let doc = parse(
            r#"<skill define="interface" name="test">
  <param name="x" type="float">Bad type</param>
</skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("invalid param type")));
    }

    #[test]
    fn test_enum_without_values() {
        let doc = parse(
            r#"<skill define="interface" name="test">
  <param name="x" type="enum">Missing values</param>
</skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors.iter().any(|e| e.message.contains("no 'values'")));
    }

    #[test]
    fn test_non_enum_with_values() {
        let doc = parse(
            r#"<skill define="interface" name="test">
  <param name="x" type="string" values="a|b">Not an enum</param>
</skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("type is not 'enum'")));
    }

    #[test]
    fn test_required_with_default_warning() {
        let doc = parse(
            r#"<skill define="interface" name="test">
  <param name="x" type="string" required="true" default="hello">Both</param>
</skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        let warnings: Vec<_> = errors
            .iter()
            .filter(|e| e.severity == Severity::Warning)
            .collect();
        assert!(warnings
            .iter()
            .any(|w| w.message.contains("required but has a default")));
    }

    #[test]
    fn test_enum_default_not_in_values() {
        let doc = parse(
            r#"<skill define="interface" name="test">
  <param name="x" type="enum" values="a|b|c" default="d">Bad default</param>
</skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("not one of the allowed values")));
    }

    #[test]
    fn test_duplicate_returns_names() {
        let doc = parse(
            r#"<skill define="interface" name="test">
  <returns name="r" type="string" />
  <returns name="r" type="number" />
</skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("duplicate returns name")));
    }

    #[test]
    fn test_boolean_default_invalid() {
        let doc = parse(
            r#"<skill define="interface" name="test">
  <param name="flag" type="boolean" default="maybe" />
</skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("not 'true' or 'false'")));
    }

    #[test]
    fn test_number_default_invalid() {
        let doc = parse(
            r#"<skill define="interface" name="test">
  <param name="count" type="number" default="abc" />
</skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("not a valid number")));
    }

    #[test]
    fn test_returns_invalid_type() {
        let doc = parse(
            r#"<skill define="interface" name="test">
  <returns name="r" type="object" />
</skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("invalid returns type")));
    }

    #[test]
    fn test_validate_duplicate_node_names() {
        let doc = parse(
            r#"<skill define="implementation" name="test-impl" implements="test">
  <node name="Step1" type="tool">
    <tool use="view" />
    Do something
  </node>
  <node name="Step1" type="prompt">
    Duplicate
  </node>
</skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("duplicate node name")));
    }

    #[test]
    fn test_validate_tool_node_without_tool_use() {
        let doc = parse(
            r#"<skill define="implementation" name="test-impl" implements="test">
  <node name="Step1" type="tool">
    No tool use declared
  </node>
</skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("no <tool use=") && e.severity == Severity::Warning));
    }

    #[test]
    fn test_validate_prompt_node_with_tool_use() {
        let doc = parse(
            r#"<skill define="implementation" name="test-impl" implements="test">
  <node name="Think" type="prompt">
    <tool use="view" />
    Should not have tool use
  </node>
</skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors.iter().any(
            |e| e.message.contains("type 'prompt' but declares <tool use=")
                && e.severity == Severity::Warning
        ));
    }

    #[test]
    fn test_validate_tool_constraint_allow_deny_conflict() {
        let doc = parse(
            r#"<skill define="interface" name="test">
  <tool allow="view" deny="bash" />
</skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("cannot have both 'allow' and 'deny'")));
    }

    #[test]
    fn test_validate_valid_nodes_no_errors() {
        let doc = parse(
            r#"<skill define="implementation" name="test-impl" implements="test">
  <node name="Read" type="tool">
    <tool use="view" />
    Read a file
  </node>
  <node name="Think" type="prompt">
    Reason about the data
  </node>
</skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        // No errors or warnings expected for well-formed nodes
        let node_errors: Vec<_> = errors
            .iter()
            .filter(|e| e.message.contains("node"))
            .collect();
        assert!(
            node_errors.is_empty(),
            "unexpected errors: {:?}",
            node_errors
        );
    }

    #[test]
    fn test_validate_skill_ref_empty_name() {
        let doc = parse(
            r#"<skill define="interface" name="test">
  <skill ref="" />
</skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("skill ref must have a non-empty name")));
    }

    // ── extends= tests ───────────────────────────────────────────────────────

    #[test]
    fn test_extends_parsed_and_stored() {
        let doc = parse(
            r#"<skill define="interface" name="dde-simple" extends="diagram-driven-execution">
</skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        assert!(
            errors.is_empty(),
            "unexpected errors on valid extends: {errors:?}"
        );
        if let Node::Skill {
            kind: NodeKind::InterfaceDefinition { name, extends, .. },
            ..
        } = &doc.nodes[0]
        {
            assert_eq!(name, "dde-simple");
            assert_eq!(extends.as_deref(), Some("diagram-driven-execution"));
        } else {
            panic!("expected InterfaceDefinition");
        }
    }

    #[test]
    fn test_extends_empty_value_is_error() {
        let doc = parse(r#"<skill define="interface" name="child" extends=""></skill>"#).unwrap();
        let errors = validate(&doc.nodes);
        assert!(
            errors.iter().any(
                |e| e.message.contains("extends attribute must be a non-empty")
                    && e.severity == Severity::Error
            ),
            "expected error for empty extends; got: {errors:?}"
        );
    }

    #[test]
    fn test_legacy_implements_on_interface_emits_warning() {
        let doc = parse(r#"<skill define="interface" name="child" implements="parent"></skill>"#)
            .unwrap();
        let errors = validate(&doc.nodes);
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("deprecated") && e.severity == Severity::Warning),
            "expected deprecation warning for implements= on interface; got: {errors:?}"
        );
    }

    #[test]
    fn test_extends_and_implements_conflict_is_error() {
        let doc = parse(
            r#"<skill define="interface" name="child" extends="parentA" implements="parentB"></skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("extends=") && e.severity == Severity::Error),
            "expected error for extends/implements conflict; got: {errors:?}"
        );
    }

    #[test]
    fn test_extends_and_implements_same_value_emits_warning_only() {
        let doc = parse(
            r#"<skill define="interface" name="child" extends="parent" implements="parent"></skill>"#,
        )
        .unwrap();
        let errors = validate(&doc.nodes);
        // Should warn (deprecated) but not error
        assert!(
            errors.iter().any(|e| e.severity == Severity::Warning),
            "expected deprecation warning; got: {errors:?}"
        );
        assert!(
            !errors.iter().any(|e| e.severity == Severity::Error),
            "unexpected error when extends and implements agree; got: {errors:?}"
        );
    }
}
