// PyO3-generated code triggers useless_conversion when map_err produces PyErr
// and ? converts it again via From<PyErr> for PyErr.
#![allow(clippy::useless_conversion)]

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use aml_core::ast::{DirectiveKind, Node, NodeKind};
use aml_core::executor::ExecutionContext;
use aml_core::registry::SkillRegistry as RustRegistry;

/// Python wrapper for a parsed AML document.
#[pyclass]
#[derive(Clone)]
struct Document {
    inner: aml_core::Document,
}

#[pymethods]
impl Document {
    /// Number of top-level nodes.
    #[getter]
    fn node_count(&self) -> usize {
        self.inner.nodes.len()
    }

    /// Return a debug representation.
    fn __repr__(&self) -> String {
        format!("{:?}", self.inner)
    }

    /// Get all definition names.
    fn definitions(&self) -> Vec<String> {
        self.inner
            .definitions()
            .filter_map(|n| match n {
                Node::Skill {
                    kind: NodeKind::InterfaceDefinition { name, .. },
                    ..
                } => Some(format!("interface:{name}")),
                Node::Skill {
                    kind: NodeKind::ImplementationDefinition { name, .. },
                    ..
                } => Some(format!("impl:{name}")),
                _ => None,
            })
            .collect()
    }

    /// Get all invocation targets.
    fn invocations(&self) -> Vec<String> {
        self.inner
            .invocations()
            .filter_map(|n| match n {
                Node::Skill {
                    kind:
                        NodeKind::Invocation {
                            interface,
                            r#impl,
                            name,
                            ..
                        },
                    ..
                } => {
                    let target = interface
                        .as_deref()
                        .or(r#impl.as_deref())
                        .or(name.as_deref())
                        .unwrap_or("unknown");
                    Some(target.to_string())
                }
                _ => None,
            })
            .collect()
    }

    /// Get all directive descriptions.
    fn directives(&self) -> Vec<String> {
        self.inner
            .directives()
            .filter_map(|n| match n {
                Node::Directive { kind, .. } => {
                    let desc = match kind {
                        DirectiveKind::Tool(t) => {
                            let target = t
                                .name
                                .as_deref()
                                .or(t.allow.as_deref())
                                .or(t.deny.as_deref())
                                .unwrap_or("unknown");
                            format!("tool:{target}")
                        }
                        DirectiveKind::Session(s) => {
                            let name = s.name.as_deref().unwrap_or("anonymous");
                            format!("session:{name}")
                        }
                        DirectiveKind::Agent(a) => {
                            format!("agent:{}", a.name)
                        }
                    };
                    Some(desc)
                }
                _ => None,
            })
            .collect()
    }
}

/// Python wrapper for the skill registry.
#[pyclass]
#[derive(Clone)]
struct AmlRegistry {
    inner: RustRegistry,
}

#[pymethods]
impl AmlRegistry {
    #[new]
    fn new() -> Self {
        Self {
            inner: RustRegistry::new(),
        }
    }

    /// Register an interface.
    #[pyo3(signature = (name, description=None))]
    fn register_interface(&mut self, name: String, description: Option<String>) -> PyResult<()> {
        self.inner
            .register_interface(name, description)
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }

    /// Register an implementation.
    #[pyo3(signature = (name, implements, language=None, framework=None, description=None, priority=0))]
    fn register_implementation(
        &mut self,
        name: String,
        implements: String,
        language: Option<String>,
        framework: Option<String>,
        description: Option<String>,
        priority: i32,
    ) -> PyResult<()> {
        self.inner
            .register_implementation(name, implements, language, framework, description, priority)
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }

    /// Register all definitions from a parsed document.
    fn register_from_document(&mut self, doc: &Document) -> PyResult<()> {
        for node in &doc.inner.nodes {
            if let Node::Skill { kind, .. } = node {
                self.inner
                    .register_from_node_kind(kind)
                    .map_err(|e| PyValueError::new_err(e.to_string()))?;
            }
        }
        Ok(())
    }

    /// Validate the registry (check orphan implementations).
    fn validate(&self) -> Vec<String> {
        self.inner
            .validate()
            .iter()
            .map(|e| e.to_string())
            .collect()
    }
}

/// Parse an AML document from a string.
#[pyfunction]
fn parse(input: &str) -> PyResult<Document> {
    match aml_core::parse(input) {
        Ok(doc) => Ok(Document { inner: doc }),
        Err(e) => Err(PyValueError::new_err(e.to_string())),
    }
}

/// Execute a document with a registry (pass-through mode — no custom handlers).
#[pyfunction]
fn execute(doc: &Document, registry: &AmlRegistry) -> PyResult<String> {
    let ctx = ExecutionContext::new(registry.inner.clone());
    ctx.execute(&doc.inner)
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

/// The AML Python module.
#[pymodule]
fn aml(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(execute, m)?)?;
    m.add_class::<Document>()?;
    m.add_class::<AmlRegistry>()?;
    Ok(())
}
