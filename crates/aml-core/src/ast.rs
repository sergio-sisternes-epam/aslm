use std::collections::HashMap;
use std::fmt;

/// Source span for error reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    #[must_use]
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Compute line and column (1-based) from byte offset.
    #[must_use]
    pub fn line_col(&self, source: &str) -> (usize, usize) {
        let mut line = 1;
        let mut col = 1;
        for (i, ch) in source.char_indices() {
            if i >= self.start {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }
}

/// The type of a skill node, determined by its attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// An invocation node — resolved and executed at runtime.
    Invocation {
        interface: Option<String>,
        r#impl: Option<String>,
        name: Option<String>,
        language: Option<String>,
        framework: Option<String>,
        retries: Option<u32>,
        timeout: Option<String>,
        policy: Option<ExecutionPolicy>,
        on_failure: Option<FailureMode>,
    },
    /// An interface definition — registered but not executed.
    InterfaceDefinition {
        name: String,
        description: Option<String>,
    },
    /// An implementation definition — registered but not executed.
    ImplementationDefinition {
        name: String,
        implements: String,
        language: Option<String>,
        framework: Option<String>,
        description: Option<String>,
    },
}

/// Execution policy for nested skills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionPolicy {
    BottomUp,
    Wrapper,
    Sequential,
}

impl ExecutionPolicy {
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "bottom-up" => Some(Self::BottomUp),
            "wrapper" => Some(Self::Wrapper),
            "sequential" => Some(Self::Sequential),
            _ => None,
        }
    }
}

impl fmt::Display for ExecutionPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BottomUp => write!(f, "bottom-up"),
            Self::Wrapper => write!(f, "wrapper"),
            Self::Sequential => write!(f, "sequential"),
        }
    }
}

/// Failure propagation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureMode {
    Halt,
    Skip,
    Partial,
}

impl FailureMode {
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "halt" => Some(Self::Halt),
            "skip" => Some(Self::Skip),
            "partial" => Some(Self::Partial),
            _ => None,
        }
    }
}

impl fmt::Display for FailureMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Halt => write!(f, "halt"),
            Self::Skip => write!(f, "skip"),
            Self::Partial => write!(f, "partial"),
        }
    }
}

/// A parameter within a skill invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub value: String,
    pub span: Span,
}

/// A node in the AML AST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// A skill tag (invocation or definition).
    Skill {
        kind: NodeKind,
        params: Vec<Param>,
        children: Vec<Node>,
        span: Span,
    },
    /// Literal text content.
    Text(String),
}

impl Node {
    /// Returns true if this is a definition node (non-executable).
    #[must_use]
    pub fn is_definition(&self) -> bool {
        matches!(
            self,
            Node::Skill {
                kind: NodeKind::InterfaceDefinition { .. }
                    | NodeKind::ImplementationDefinition { .. },
                ..
            }
        )
    }

    /// Returns the span if this is a Skill node.
    #[must_use]
    pub fn span(&self) -> Option<Span> {
        match self {
            Node::Skill { span, .. } => Some(*span),
            Node::Text(_) => None,
        }
    }

    /// Returns parameters if this is a Skill node.
    #[must_use]
    pub fn params(&self) -> Option<&[Param]> {
        match self {
            Node::Skill { params, .. } => Some(params),
            Node::Text(_) => None,
        }
    }

    /// Returns a params map for convenience.
    #[must_use]
    pub fn params_map(&self) -> HashMap<String, String> {
        match self {
            Node::Skill { params, .. } => params
                .iter()
                .map(|p| (p.name.clone(), p.value.clone()))
                .collect(),
            Node::Text(_) => HashMap::new(),
        }
    }
}

/// The root of an AML document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub nodes: Vec<Node>,
}

impl Document {
    #[must_use]
    pub fn new(nodes: Vec<Node>) -> Self {
        Self { nodes }
    }

    /// Iterate over all skill nodes (flat, non-recursive).
    pub fn skill_nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes
            .iter()
            .filter(|n| matches!(n, Node::Skill { .. }))
    }

    /// Iterate over all definition nodes.
    pub fn definitions(&self) -> impl Iterator<Item = &Node> {
        self.nodes.iter().filter(|n| n.is_definition())
    }

    /// Iterate over all invocation nodes.
    pub fn invocations(&self) -> impl Iterator<Item = &Node> {
        self.nodes.iter().filter(|n| {
            matches!(
                n,
                Node::Skill {
                    kind: NodeKind::Invocation { .. },
                    ..
                }
            )
        })
    }
}
