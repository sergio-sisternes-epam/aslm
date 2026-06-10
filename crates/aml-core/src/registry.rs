use std::collections::HashMap;

use crate::ast::{
    FieldDecl, IoDecl, NodeDecl, NodeKind, ParamDecl, ReturnDecl, SkillRef, ToolConstraint,
};

/// Metadata for a registered interface.
#[derive(Debug, Clone)]
pub struct InterfaceEntry {
    pub name: String,
    /// Parent interface name for interface inheritance (`extends=`).
    /// This is metadata only — resolution does NOT traverse the hierarchy.
    pub extends: Option<String>,
    pub description: Option<String>,
    /// Typed parameter declarations (empty for legacy text-only interfaces).
    pub params: Vec<ParamDecl>,
    /// Return value declarations.
    pub returns: Vec<ReturnDecl>,
    /// File-read declarations.
    pub reads: Option<IoDecl>,
    /// File-write declarations.
    pub writes: Option<IoDecl>,
    /// Skill references (e.g. DDE enforcement).
    pub skill_refs: Vec<SkillRef>,
    /// Tool constraints as part of the interface contract.
    pub tool_constraints: Vec<ToolConstraint>,
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
    /// DDE node declarations.
    pub nodes: Vec<NodeDecl>,
}

/// Metadata for a registered contract.
#[derive(Debug, Clone)]
pub struct ContractEntry {
    pub name: String,
    pub extends: Option<String>,
    pub version: Option<String>,
    pub fields: Vec<FieldDecl>,
}

/// Registry error types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    DuplicateInterface(String),
    DuplicateImplementation(String),
    DuplicateContract(String),
    ImplementsUnknownInterface {
        implementation: String,
        interface: String,
    },
    ExtendsUnknownInterface {
        child: String,
        parent: String,
    },
    ExtendsInterfaceCycle {
        /// The cycle path, e.g. `"A -> B -> C -> A"`.
        cycle: String,
    },
    ContractReferenceUnknown {
        source: String,
        contract: String,
    },
    ContractExtendsUnknown {
        child: String,
        parent: String,
    },
    ContractExtendsCycle {
        cycle: String,
    },
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
            Self::DuplicateContract(name) => {
                write!(f, "duplicate contract definition: '{name}'")
            }
            Self::ImplementsUnknownInterface {
                implementation,
                interface,
            } => {
                write!(
                    f,
                    "implementation '{implementation}' references unknown interface '{interface}'"
                )
            }
            Self::ExtendsUnknownInterface { child, parent } => {
                write!(
                    f,
                    "interface '{child}' extends unknown interface '{parent}'"
                )
            }
            Self::ExtendsInterfaceCycle { cycle } => {
                write!(f, "interface extends cycle detected: {cycle}")
            }
            Self::ContractReferenceUnknown { source, contract } => {
                write!(f, "'{source}' references unknown contract '{contract}'")
            }
            Self::ContractExtendsUnknown { child, parent } => {
                write!(f, "contract '{child}' extends unknown contract '{parent}'")
            }
            Self::ContractExtendsCycle { cycle } => {
                write!(f, "contract extends cycle detected: {cycle}")
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
    contracts: HashMap<String, ContractEntry>,
    /// Index: interface name → list of implementation names.
    impl_by_interface: HashMap<String, Vec<String>>,
}

impl SkillRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an interface definition.
    #[allow(clippy::too_many_arguments)]
    pub fn register_interface(
        &mut self,
        name: String,
        extends: Option<String>,
        description: Option<String>,
        params: Vec<ParamDecl>,
        returns: Vec<ReturnDecl>,
        reads: Option<IoDecl>,
        writes: Option<IoDecl>,
        skill_refs: Vec<SkillRef>,
        tool_constraints: Vec<ToolConstraint>,
    ) -> Result<(), RegistryError> {
        if self.interfaces.contains_key(&name) {
            return Err(RegistryError::DuplicateInterface(name));
        }
        self.interfaces.insert(
            name.clone(),
            InterfaceEntry {
                name,
                extends,
                description,
                params,
                returns,
                reads,
                writes,
                skill_refs,
                tool_constraints,
            },
        );
        Ok(())
    }

    /// Register an implementation definition.
    #[allow(clippy::too_many_arguments)]
    pub fn register_implementation(
        &mut self,
        name: String,
        implements: String,
        language: Option<String>,
        framework: Option<String>,
        description: Option<String>,
        priority: i32,
        nodes: Vec<NodeDecl>,
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
                nodes,
            },
        );
        Ok(())
    }

    /// Register a contract definition.
    pub fn register_contract(
        &mut self,
        name: String,
        extends: Option<String>,
        version: Option<String>,
        fields: Vec<FieldDecl>,
    ) -> Result<(), RegistryError> {
        if self.contracts.contains_key(&name) {
            return Err(RegistryError::DuplicateContract(name));
        }
        self.contracts.insert(
            name.clone(),
            ContractEntry {
                name,
                extends,
                version,
                fields,
            },
        );
        Ok(())
    }

    /// Register definitions extracted from AST nodes.
    pub fn register_from_node_kind(&mut self, kind: &NodeKind) -> Result<(), RegistryError> {
        match kind {
            NodeKind::InterfaceDefinition {
                name,
                extends,
                description,
                params,
                returns,
                reads,
                writes,
                skill_refs,
                tool_constraints,
                ..
            } => self.register_interface(
                name.clone(),
                extends.clone(),
                description.clone(),
                params.clone(),
                returns.clone(),
                reads.clone(),
                writes.clone(),
                skill_refs.clone(),
                tool_constraints.clone(),
            ),
            NodeKind::ImplementationDefinition {
                name,
                implements,
                language,
                framework,
                description,
                nodes,
                ..
            } => self.register_implementation(
                name.clone(),
                implements.clone(),
                language.clone(),
                framework.clone(),
                description.clone(),
                0,
                nodes.clone(),
            ),
            NodeKind::ContractDefinition {
                name,
                extends,
                version,
                fields,
                ..
            } => self.register_contract(
                name.clone(),
                extends.clone(),
                version.clone(),
                fields.clone(),
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

    /// Look up a contract by name.
    #[must_use]
    pub fn get_contract(&self, name: &str) -> Option<&ContractEntry> {
        self.contracts.get(name)
    }

    /// Get all contracts.
    #[must_use]
    pub fn contracts(&self) -> &HashMap<String, ContractEntry> {
        &self.contracts
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

    /// Validate cross-definition references and inheritance graphs.
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

        for entry in self.interfaces.values() {
            if let Some(ref parent) = entry.extends {
                if !self.interfaces.contains_key(parent) {
                    errors.push(RegistryError::ExtendsUnknownInterface {
                        child: entry.name.clone(),
                        parent: parent.clone(),
                    });
                }
            }

            collect_contract_refs_from_types(
                &entry.name,
                entry
                    .params
                    .iter()
                    .filter_map(|param| param.param_type.as_deref()),
                &self.contracts,
                &mut errors,
            );
            collect_contract_refs_from_types(
                &entry.name,
                entry
                    .returns
                    .iter()
                    .filter_map(|ret| ret.return_type.as_deref()),
                &self.contracts,
                &mut errors,
            );
        }

        push_cycles(
            self.interfaces.iter().filter_map(|(name, entry)| {
                entry
                    .extends
                    .as_ref()
                    .map(|parent| (name.clone(), parent.clone()))
            }),
            |cycle| RegistryError::ExtendsInterfaceCycle { cycle },
            &mut errors,
        );

        for entry in self.contracts.values() {
            if let Some(ref parent) = entry.extends {
                if !self.contracts.contains_key(parent) {
                    errors.push(RegistryError::ContractExtendsUnknown {
                        child: entry.name.clone(),
                        parent: parent.clone(),
                    });
                }
            }

            collect_contract_refs_from_fields(
                &entry.name,
                &entry.fields,
                &self.contracts,
                &mut errors,
            );
        }

        push_cycles(
            self.contracts.iter().filter_map(|(name, entry)| {
                entry
                    .extends
                    .as_ref()
                    .map(|parent| (name.clone(), parent.clone()))
            }),
            |cycle| RegistryError::ContractExtendsCycle { cycle },
            &mut errors,
        );

        errors
    }
}

fn collect_contract_refs_from_types<'a>(
    source: &str,
    types: impl Iterator<Item = &'a str>,
    contracts: &HashMap<String, ContractEntry>,
    errors: &mut Vec<RegistryError>,
) {
    for ty in types {
        if let Some(contract) = ty.strip_prefix("contract:") {
            if !contract.is_empty() && !contracts.contains_key(contract) {
                errors.push(RegistryError::ContractReferenceUnknown {
                    source: source.to_string(),
                    contract: contract.to_string(),
                });
            }
        }
    }
}

fn collect_contract_refs_from_fields(
    source: &str,
    fields: &[FieldDecl],
    contracts: &HashMap<String, ContractEntry>,
    errors: &mut Vec<RegistryError>,
) {
    for field in fields {
        if let Some(contract) = field
            .field_type
            .as_deref()
            .and_then(|ty| ty.strip_prefix("contract:"))
        {
            if !contract.is_empty() && !contracts.contains_key(contract) {
                errors.push(RegistryError::ContractReferenceUnknown {
                    source: source.to_string(),
                    contract: contract.to_string(),
                });
            }
        }

        collect_contract_refs_from_fields(source, &field.children, contracts, errors);
    }
}

fn push_cycles(
    edges: impl Iterator<Item = (String, String)>,
    build_error: impl Fn(String) -> RegistryError,
    errors: &mut Vec<RegistryError>,
) {
    let extends_map: HashMap<String, String> = edges.collect();
    let mut reported_cycles: Vec<String> = Vec::new();
    let mut names: Vec<String> = extends_map.keys().cloned().collect();
    names.sort();

    for start in &names {
        if reported_cycles.contains(start) {
            continue;
        }

        let mut path: Vec<String> = Vec::new();
        let mut current = start.clone();
        loop {
            if let Some(pos) = path.iter().position(|node| node == &current) {
                let cycle_nodes = &path[pos..];
                let mut cycle = cycle_nodes.join(" -> ");
                cycle.push_str(" -> ");
                cycle.push_str(&current);
                for node in cycle_nodes {
                    reported_cycles.push(node.clone());
                }
                errors.push(build_error(cycle));
                break;
            }

            path.push(current.clone());
            match extends_map.get(&current) {
                Some(parent) => current = parent.clone(),
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_lookup() {
        let mut reg = SkillRegistry::new();
        reg.register_interface(
            "testing".into(),
            None,
            Some("Run tests".into()),
            Vec::new(),
            Vec::new(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        reg.register_implementation(
            "pytest-impl".into(),
            "testing".into(),
            Some("python".into()),
            Some("pytest".into()),
            None,
            0,
            Vec::new(),
        )
        .unwrap();

        assert!(reg.get_interface("testing").is_some());
        assert!(reg.get_implementation("pytest-impl").is_some());
        assert_eq!(reg.implementations_for("testing").len(), 1);
    }

    #[test]
    fn test_duplicate_interface() {
        let mut reg = SkillRegistry::new();
        reg.register_interface(
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
        let err = reg
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
            .unwrap_err();
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
            Vec::new(),
        )
        .unwrap();
        let errors = reg.validate();
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_extends_unknown_parent_is_error() {
        let mut reg = SkillRegistry::new();
        reg.register_interface(
            "child".into(),
            Some("nonexistent-parent".into()),
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        let errors = reg.validate();
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, RegistryError::ExtendsUnknownInterface { child, .. } if child == "child")),
            "expected ExtendsUnknownInterface; got: {errors:?}"
        );
    }

    #[test]
    fn test_extends_self_cycle_is_error() {
        let mut reg = SkillRegistry::new();
        reg.register_interface(
            "self-loop".into(),
            Some("self-loop".into()),
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        let errors = reg.validate();
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, RegistryError::ExtendsInterfaceCycle { .. })),
            "expected cycle error for self-extension; got: {errors:?}"
        );
    }

    #[test]
    fn test_extends_two_node_cycle_is_error() {
        let mut reg = SkillRegistry::new();
        reg.register_interface(
            "a".into(),
            Some("b".into()),
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        reg.register_interface(
            "b".into(),
            Some("a".into()),
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        let errors = reg.validate();
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, RegistryError::ExtendsInterfaceCycle { .. })),
            "expected cycle error for A→B→A; got: {errors:?}"
        );
    }

    #[test]
    fn test_valid_extends_hierarchy_no_errors() {
        let mut reg = SkillRegistry::new();
        // root
        reg.register_interface(
            "root".into(),
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
        // child extends root
        reg.register_interface(
            "child".into(),
            Some("root".into()),
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        // grandchild extends child
        reg.register_interface(
            "grandchild".into(),
            Some("child".into()),
            None,
            Vec::new(),
            Vec::new(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        let errors = reg.validate();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }
}
