use std::collections::HashMap;

use crate::ast::NodeKind;

/// Metadata for a registered interface.
#[derive(Debug, Clone)]
pub struct InterfaceEntry {
    pub name: String,
    pub description: Option<String>,
}

/// Metadata for a registered implementation.
#[derive(Debug, Clone)]
pub struct ImplementationEntry {
    pub name: String,
    pub implements: String,
    pub language: Option<String>,
    pub framework: Option<String>,
    pub description: Option<String>,
    /// Higher priority wins during resolution (default: 0).
    pub priority: i32,
}

/// Registry error types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    DuplicateInterface(String),
    DuplicateImplementation(String),
    ImplementsUnknownInterface { implementation: String, interface: String },
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateInterface(name) => {
                write!(f, "duplicate interface definition: '{name}'")
            }
            Self::DuplicateImplementation(name) => {
                write!(f, "duplicate implementation definition: '{name}'")
            }
            Self::ImplementsUnknownInterface { implementation, interface } => {
                write!(
                    f,
                    "implementation '{implementation}' references unknown interface '{interface}'"
                )
            }
        }
    }
}

impl std::error::Error for RegistryError {}

/// The skill registry holds all known interfaces and implementations.
#[derive(Debug, Clone, Default)]
pub struct SkillRegistry {
    interfaces: HashMap<String, InterfaceEntry>,
    implementations: HashMap<String, ImplementationEntry>,
    /// Index: interface name → list of implementation names.
    impl_by_interface: HashMap<String, Vec<String>>,
}

impl SkillRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an interface definition.
    pub fn register_interface(
        &mut self,
        name: String,
        description: Option<String>,
    ) -> Result<(), RegistryError> {
        if self.interfaces.contains_key(&name) {
            return Err(RegistryError::DuplicateInterface(name));
        }
        self.interfaces.insert(name.clone(), InterfaceEntry { name, description });
        Ok(())
    }

    /// Register an implementation definition.
    pub fn register_implementation(
        &mut self,
        name: String,
        implements: String,
        language: Option<String>,
        framework: Option<String>,
        description: Option<String>,
        priority: i32,
    ) -> Result<(), RegistryError> {
        if self.implementations.contains_key(&name) {
            return Err(RegistryError::DuplicateImplementation(name));
        }
        self.impl_by_interface
            .entry(implements.clone())
            .or_default()
            .push(name.clone());
        self.implementations.insert(
            name.clone(),
            ImplementationEntry {
                name,
                implements,
                language,
                framework,
                description,
                priority,
            },
        );
        Ok(())
    }

    /// Register definitions extracted from AST nodes.
    pub fn register_from_node_kind(&mut self, kind: &NodeKind) -> Result<(), RegistryError> {
        match kind {
            NodeKind::InterfaceDefinition { name, description } => {
                self.register_interface(name.clone(), description.clone())
            }
            NodeKind::ImplementationDefinition {
                name,
                implements,
                language,
                framework,
                description,
            } => self.register_implementation(
                name.clone(),
                implements.clone(),
                language.clone(),
                framework.clone(),
                description.clone(),
                0,
            ),
            NodeKind::Invocation { .. } => Ok(()), // Invocations are not registered
        }
    }

    /// Look up an implementation by exact name.
    #[must_use]
    pub fn get_implementation(&self, name: &str) -> Option<&ImplementationEntry> {
        self.implementations.get(name)
    }

    /// Look up an interface by name.
    #[must_use]
    pub fn get_interface(&self, name: &str) -> Option<&InterfaceEntry> {
        self.interfaces.get(name)
    }

    /// Get all implementations for a given interface.
    #[must_use]
    pub fn implementations_for(&self, interface: &str) -> Vec<&ImplementationEntry> {
        self.impl_by_interface
            .get(interface)
            .map(|names| {
                names
                    .iter()
                    .filter_map(|n| self.implementations.get(n))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Validate that all implementations reference known interfaces.
    pub fn validate(&self) -> Vec<RegistryError> {
        let mut errors = Vec::new();
        for entry in self.implementations.values() {
            if !self.interfaces.contains_key(&entry.implements) {
                errors.push(RegistryError::ImplementsUnknownInterface {
                    implementation: entry.name.clone(),
                    interface: entry.implements.clone(),
                });
            }
        }
        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_lookup() {
        let mut reg = SkillRegistry::new();
        reg.register_interface("testing".into(), Some("Run tests".into()))
            .unwrap();
        reg.register_implementation(
            "pytest-impl".into(),
            "testing".into(),
            Some("python".into()),
            Some("pytest".into()),
            None,
            0,
        )
        .unwrap();

        assert!(reg.get_interface("testing").is_some());
        assert!(reg.get_implementation("pytest-impl").is_some());
        assert_eq!(reg.implementations_for("testing").len(), 1);
    }

    #[test]
    fn test_duplicate_interface() {
        let mut reg = SkillRegistry::new();
        reg.register_interface("testing".into(), None).unwrap();
        let err = reg.register_interface("testing".into(), None).unwrap_err();
        assert_eq!(err, RegistryError::DuplicateInterface("testing".into()));
    }

    #[test]
    fn test_validate_unknown_interface() {
        let mut reg = SkillRegistry::new();
        reg.register_implementation(
            "orphan-impl".into(),
            "nonexistent".into(),
            None,
            None,
            None,
            0,
        )
        .unwrap();
        let errors = reg.validate();
        assert_eq!(errors.len(), 1);
    }
}
