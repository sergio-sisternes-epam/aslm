use std::collections::HashMap;

use crate::ast::{DirectiveKind, Document, ExecutionPolicy, FailureMode, Node, NodeKind};
use crate::registry::SkillRegistry;
use crate::resolver::{self, ResolutionHints, ResolveError};

/// The result of executing a single skill.
#[derive(Debug, Clone)]
pub struct SkillResult {
    /// The text output to inject in place of the skill tag.
    pub text: String,
    /// Structured metadata (optional side-channel).
    pub metadata: HashMap<String, String>,
    /// Execution status.
    pub status: SkillStatus,
}

/// Status of a skill execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillStatus {
    Success,
    Failed,
    Skipped,
    Partial,
}

/// Execution error types.
#[derive(Debug, Clone)]
pub enum ExecutionError {
    ResolutionFailed(ResolveError),
    HandlerError {
        skill: String,
        message: String,
    },
    RetriesExhausted {
        skill: String,
        attempts: u32,
    },
    ChildFailed {
        skill: String,
        child_error: Box<ExecutionError>,
    },
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ResolutionFailed(e) => write!(f, "resolution failed: {e}"),
            Self::HandlerError { skill, message } => {
                write!(f, "skill '{skill}' handler error: {message}")
            }
            Self::RetriesExhausted { skill, attempts } => {
                write!(f, "skill '{skill}' failed after {attempts} attempts")
            }
            Self::ChildFailed { skill, child_error } => {
                write!(f, "child of '{skill}' failed: {child_error}")
            }
        }
    }
}

impl std::error::Error for ExecutionError {}

/// A skill handler function signature.
/// Receives: resolved implementation name, params, scope content (children as text).
/// Returns: the skill result or an error message.
pub type SkillHandler =
    Box<dyn Fn(&str, &HashMap<String, String>, &str) -> Result<SkillResult, String> + Send + Sync>;

/// Execution context holding the registry and skill handlers.
pub struct ExecutionContext {
    pub registry: SkillRegistry,
    handlers: HashMap<String, SkillHandler>,
}

impl ExecutionContext {
    #[must_use]
    pub fn new(registry: SkillRegistry) -> Self {
        Self {
            registry,
            handlers: HashMap::new(),
        }
    }

    /// Register a handler for a specific implementation name.
    pub fn register_handler(&mut self, impl_name: impl Into<String>, handler: SkillHandler) {
        self.handlers.insert(impl_name.into(), handler);
    }

    /// Execute a full document, returning the final text output.
    pub fn execute(&self, doc: &Document) -> Result<String, ExecutionError> {
        let mut output = String::new();
        for node in &doc.nodes {
            output.push_str(&self.execute_node(node)?);
        }
        Ok(output)
    }

    /// Execute a single node, returning its text contribution.
    fn execute_node(&self, node: &Node) -> Result<String, ExecutionError> {
        match node {
            Node::Text(text) => Ok(text.clone()),
            Node::Directive { kind, children, .. } => {
                // Directives pass through: execute children and concatenate results.
                // Runtime-specific behaviour (tool constraints, session isolation,
                // agent delegation) is handled by the harness, not the core executor.
                let on_failure = match kind {
                    DirectiveKind::Session(s) => s.on_failure.unwrap_or(FailureMode::Halt),
                    DirectiveKind::Agent(a) => a.on_failure.unwrap_or(FailureMode::Halt),
                    DirectiveKind::Tool(_) => FailureMode::Halt,
                };
                let mut output = String::new();
                for child in children {
                    match self.execute_node(child) {
                        Ok(text) => output.push_str(&text),
                        Err(e) => match on_failure {
                            FailureMode::Halt => return Err(e),
                            FailureMode::Skip => {}
                            FailureMode::Partial => {
                                output.push_str(&format!("[DIRECTIVE FAILED: {e}]"));
                            }
                        },
                    }
                }
                Ok(output)
            }
            Node::Skill {
                kind,
                params,
                children,
                ..
            } => {
                match kind {
                    // Definition nodes produce no output
                    NodeKind::InterfaceDefinition { .. }
                    | NodeKind::ImplementationDefinition { .. }
                    | NodeKind::ContractDefinition { .. } => Ok(String::new()),

                    NodeKind::Invocation {
                        interface,
                        r#impl,
                        name,
                        language,
                        framework,
                        retries,
                        on_failure,
                        policy,
                        ..
                    } => {
                        let policy = policy.unwrap_or(ExecutionPolicy::BottomUp);
                        let on_failure = on_failure.unwrap_or(FailureMode::Halt);
                        let max_retries = retries.unwrap_or(0);

                        self.execute_invocation(
                            interface.as_deref(),
                            r#impl.as_deref(),
                            name.as_deref(),
                            language.as_deref(),
                            framework.as_deref(),
                            params,
                            children,
                            policy,
                            on_failure,
                            max_retries,
                        )
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_invocation(
        &self,
        interface: Option<&str>,
        r#impl: Option<&str>,
        name: Option<&str>,
        language: Option<&str>,
        framework: Option<&str>,
        params: &[crate::ast::Param],
        children: &[Node],
        policy: ExecutionPolicy,
        on_failure: FailureMode,
        max_retries: u32,
    ) -> Result<String, ExecutionError> {
        // Resolve the implementation
        let hints = ResolutionHints {
            language: language.map(String::from),
            framework: framework.map(String::from),
        };
        let resolved = resolver::resolve(&self.registry, interface, r#impl, name, &hints)
            .map_err(ExecutionError::ResolutionFailed)?;

        let impl_name = &resolved.name;

        // Execute children based on policy
        let scope_content = match policy {
            ExecutionPolicy::BottomUp => {
                // Children execute first; their results become the scope
                self.execute_children(children, impl_name, on_failure)?
            }
            ExecutionPolicy::Wrapper => {
                // Wrapper: pass raw children text to handler (handler controls execution)
                self.children_as_text(children)
            }
            ExecutionPolicy::Sequential => {
                // Sequential: execute children in order, each seeing previous results
                self.execute_children(children, impl_name, on_failure)?
            }
        };

        // Build params map
        let params_map: HashMap<String, String> = params
            .iter()
            .map(|p| (p.name.clone(), p.value.clone()))
            .collect();

        // Execute with retry
        let mut last_error = None;
        for attempt in 0..=max_retries {
            match self.invoke_handler(impl_name, &params_map, &scope_content) {
                Ok(result) => match result.status {
                    SkillStatus::Success | SkillStatus::Partial => return Ok(result.text),
                    SkillStatus::Skipped => return Ok(String::new()),
                    SkillStatus::Failed => {
                        last_error = Some(result.text);
                        if attempt == max_retries {
                            break;
                        }
                    }
                },
                Err(msg) => {
                    last_error = Some(msg);
                    if attempt == max_retries {
                        break;
                    }
                }
            }
        }

        // Retries exhausted — apply failure mode
        let error_msg = last_error.unwrap_or_else(|| "unknown error".to_string());
        match on_failure {
            FailureMode::Halt => Err(ExecutionError::RetriesExhausted {
                skill: impl_name.clone(),
                attempts: max_retries + 1,
            }),
            FailureMode::Skip => Ok(String::new()),
            FailureMode::Partial => Ok(format!("[FAILED: {impl_name}: {error_msg}]")),
        }
    }

    fn execute_children(
        &self,
        children: &[Node],
        parent_skill: &str,
        on_failure: FailureMode,
    ) -> Result<String, ExecutionError> {
        let mut output = String::new();
        for child in children {
            match self.execute_node(child) {
                Ok(text) => output.push_str(&text),
                Err(e) => match on_failure {
                    FailureMode::Halt => {
                        return Err(ExecutionError::ChildFailed {
                            skill: parent_skill.to_string(),
                            child_error: Box::new(e),
                        });
                    }
                    FailureMode::Skip => {}
                    FailureMode::Partial => {
                        output.push_str(&format!("[CHILD FAILED: {e}]"));
                    }
                },
            }
        }
        Ok(output)
    }

    fn children_as_text(&self, children: &[Node]) -> String {
        let mut output = String::new();
        for child in children {
            match child {
                Node::Text(t) => output.push_str(t),
                Node::Skill { .. } => {
                    // In wrapper mode, skill tags are passed as-is (text representation)
                    output.push_str("[nested-skill]");
                }
                Node::Directive { .. } => {
                    output.push_str("[nested-directive]");
                }
            }
        }
        output
    }

    fn invoke_handler(
        &self,
        impl_name: &str,
        params: &HashMap<String, String>,
        scope: &str,
    ) -> Result<SkillResult, String> {
        if let Some(handler) = self.handlers.get(impl_name) {
            handler(impl_name, params, scope)
        } else {
            // No handler registered — return scope content as pass-through
            Ok(SkillResult {
                text: scope.to_string(),
                metadata: HashMap::new(),
                status: SkillStatus::Success,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn setup_context() -> ExecutionContext {
        let mut registry = SkillRegistry::new();
        registry
            .register_interface(
                "testing".into(),
                None,
                None,
                Vec::new(),
                Vec::new(),
                None,
                None,
                Vec::new(),
                Vec::new(),
            )
            .unwrap();
        registry
            .register_implementation(
                "pytest-impl".into(),
                "testing".into(),
                Some("python".into()),
                None,
                None,
                0,
                Vec::new(),
            )
            .unwrap();

        let mut ctx = ExecutionContext::new(registry);
        ctx.register_handler(
            "pytest-impl",
            Box::new(|_name, _params, scope| {
                Ok(SkillResult {
                    text: format!("[tested: {scope}]"),
                    metadata: HashMap::new(),
                    status: SkillStatus::Success,
                })
            }),
        );
        ctx
    }

    #[test]
    fn test_simple_execution() {
        let ctx = setup_context();
        let doc = parse(r#"<skill interface="testing" language="python">my code</skill>"#).unwrap();
        let result = ctx.execute(&doc).unwrap();
        assert_eq!(result, "[tested: my code]");
    }

    #[test]
    fn test_text_passthrough() {
        let ctx = setup_context();
        let doc = parse("just plain text").unwrap();
        let result = ctx.execute(&doc).unwrap();
        assert_eq!(result, "just plain text");
    }

    #[test]
    fn test_definition_produces_no_output() {
        let ctx = setup_context();
        let doc = parse(r#"<skill define="interface" name="foo">description</skill>"#).unwrap();
        let result = ctx.execute(&doc).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_skip_on_failure() {
        let mut registry = SkillRegistry::new();
        registry
            .register_interface(
                "failing".into(),
                None,
                None,
                Vec::new(),
                Vec::new(),
                None,
                None,
                Vec::new(),
                Vec::new(),
            )
            .unwrap();
        registry
            .register_implementation(
                "fail-impl".into(),
                "failing".into(),
                None,
                None,
                None,
                0,
                Vec::new(),
            )
            .unwrap();

        let mut ctx = ExecutionContext::new(registry);
        ctx.register_handler(
            "fail-impl",
            Box::new(|_name, _params, _scope| Err("always fails".to_string())),
        );

        let doc =
            parse(r#"before <skill interface="failing" on-failure="skip">content</skill> after"#)
                .unwrap();
        let result = ctx.execute(&doc).unwrap();
        assert_eq!(result, "before  after");
    }

    #[test]
    fn test_halt_on_failure() {
        let mut registry = SkillRegistry::new();
        registry
            .register_interface(
                "failing".into(),
                None,
                None,
                Vec::new(),
                Vec::new(),
                None,
                None,
                Vec::new(),
                Vec::new(),
            )
            .unwrap();
        registry
            .register_implementation(
                "fail-impl".into(),
                "failing".into(),
                None,
                None,
                None,
                0,
                Vec::new(),
            )
            .unwrap();

        let mut ctx = ExecutionContext::new(registry);
        ctx.register_handler(
            "fail-impl",
            Box::new(|_name, _params, _scope| Err("always fails".to_string())),
        );

        let doc = parse(r#"<skill interface="failing">content</skill>"#).unwrap();
        let result = ctx.execute(&doc);
        assert!(result.is_err());
    }

    #[test]
    fn test_directive_passthrough() {
        let ctx = setup_context();
        let doc = parse(r#"<tool name="bash">plain text</tool>"#).unwrap();
        let result = ctx.execute(&doc).unwrap();
        assert_eq!(result, "plain text");
    }

    #[test]
    fn test_directive_with_nested_skill() {
        let ctx = setup_context();
        let doc = parse(
            r#"<agent name="dev"><skill interface="testing" language="python">my code</skill></agent>"#,
        )
        .unwrap();
        let result = ctx.execute(&doc).unwrap();
        assert_eq!(result, "[tested: my code]");
    }

    #[test]
    fn test_nested_directives() {
        let ctx = setup_context();
        let doc = parse(r#"<session name="s1"><tool name="bash">hello</tool></session>"#).unwrap();
        let result = ctx.execute(&doc).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_agent_on_failure_skip() {
        let mut registry = SkillRegistry::new();
        registry
            .register_interface(
                "failing".into(),
                None,
                None,
                Vec::new(),
                Vec::new(),
                None,
                None,
                Vec::new(),
                Vec::new(),
            )
            .unwrap();
        registry
            .register_implementation(
                "fail-impl".into(),
                "failing".into(),
                None,
                None,
                None,
                0,
                Vec::new(),
            )
            .unwrap();

        let mut ctx = ExecutionContext::new(registry);
        ctx.register_handler(
            "fail-impl",
            Box::new(|_name, _params, _scope| Err("always fails".to_string())),
        );

        let doc = parse(
            r#"before <agent name="worker" on-failure="skip"><skill interface="failing">x</skill></agent> after"#,
        )
        .unwrap();
        let result = ctx.execute(&doc).unwrap();
        assert_eq!(result, "before  after");
    }

    #[test]
    fn test_session_on_failure_partial() {
        let mut registry = SkillRegistry::new();
        registry
            .register_interface(
                "failing".into(),
                None,
                None,
                Vec::new(),
                Vec::new(),
                None,
                None,
                Vec::new(),
                Vec::new(),
            )
            .unwrap();
        registry
            .register_implementation(
                "fail-impl".into(),
                "failing".into(),
                None,
                None,
                None,
                0,
                Vec::new(),
            )
            .unwrap();

        let mut ctx = ExecutionContext::new(registry);
        ctx.register_handler(
            "fail-impl",
            Box::new(|_name, _params, _scope| Err("broken".to_string())),
        );

        let doc = parse(
            r#"<session name="s1" on-failure="partial"><skill interface="failing">x</skill></session>"#,
        )
        .unwrap();
        let result = ctx.execute(&doc).unwrap();
        assert!(
            result.contains("DIRECTIVE FAILED"),
            "partial mode should include failure text: {result}"
        );
    }

    #[test]
    fn test_agent_on_failure_halt() {
        let mut registry = SkillRegistry::new();
        registry
            .register_interface(
                "failing".into(),
                None,
                None,
                Vec::new(),
                Vec::new(),
                None,
                None,
                Vec::new(),
                Vec::new(),
            )
            .unwrap();
        registry
            .register_implementation(
                "fail-impl".into(),
                "failing".into(),
                None,
                None,
                None,
                0,
                Vec::new(),
            )
            .unwrap();

        let mut ctx = ExecutionContext::new(registry);
        ctx.register_handler(
            "fail-impl",
            Box::new(|_name, _params, _scope| Err("always fails".to_string())),
        );

        let doc = parse(
            r#"<agent name="worker" on-failure="halt"><skill interface="failing">x</skill></agent>"#,
        )
        .unwrap();
        let result = ctx.execute(&doc);
        assert!(result.is_err(), "halt mode should propagate error");
    }
}
