use crate::registry::{ImplementationEntry, SkillRegistry};

/// Resolution error types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    /// No implementations found for the interface.
    NoImplementation { interface: String },
    /// Multiple implementations match and none has higher priority.
    Ambiguous {
        interface: String,
        candidates: Vec<String>,
    },
    /// Explicit `impl` not found in registry.
    ImplNotFound { name: String },
    /// Explicit `impl` does not implement the declared interface.
    ImplMismatch {
        r#impl: String,
        expected_interface: String,
        actual_interface: String,
    },
    /// No resolution target (neither interface nor impl nor name).
    NoTarget,
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoImplementation { interface } => {
                write!(f, "no implementation found for interface '{interface}'")
            }
            Self::Ambiguous {
                interface,
                candidates,
            } => {
                write!(
                    f,
                    "ambiguous resolution for interface '{interface}': candidates are [{}]",
                    candidates.join(", ")
                )
            }
            Self::ImplNotFound { name } => {
                write!(f, "implementation '{name}' not found in registry")
            }
            Self::ImplMismatch {
                r#impl,
                expected_interface,
                actual_interface,
            } => {
                write!(
                    f,
                    "implementation '{impl}' implements '{actual_interface}', not '{expected_interface}'"
                )
            }
            Self::NoTarget => {
                write!(
                    f,
                    "skill invocation has no resolution target (no interface, impl, or name)"
                )
            }
        }
    }
}

impl std::error::Error for ResolveError {}

/// Hints provided by the invocation for resolution filtering.
#[derive(Debug, Clone, Default)]
pub struct ResolutionHints {
    pub language: Option<String>,
    pub framework: Option<String>,
}

/// Resolve an invocation to a concrete implementation.
///
/// Resolution priority:
/// 1. `impl` present → use exact implementation (validate against interface if both present)
/// 2. `interface` present → filter by hints, pick highest priority, fail if ambiguous
/// 3. `name` present → direct lookup as implementation name
pub fn resolve<'a>(
    registry: &'a SkillRegistry,
    interface: Option<&str>,
    r#impl: Option<&str>,
    name: Option<&str>,
    hints: &ResolutionHints,
) -> Result<&'a ImplementationEntry, ResolveError> {
    // Priority 1: explicit impl
    if let Some(impl_name) = r#impl {
        let entry =
            registry
                .get_implementation(impl_name)
                .ok_or_else(|| ResolveError::ImplNotFound {
                    name: impl_name.to_string(),
                })?;

        // If interface is also specified, validate consistency
        if let Some(iface) = interface {
            if entry.implements != iface {
                return Err(ResolveError::ImplMismatch {
                    r#impl: impl_name.to_string(),
                    expected_interface: iface.to_string(),
                    actual_interface: entry.implements.clone(),
                });
            }
        }
        return Ok(entry);
    }

    // Priority 2: interface resolution with hints
    if let Some(iface) = interface {
        let mut candidates = registry.implementations_for(iface);

        if candidates.is_empty() {
            return Err(ResolveError::NoImplementation {
                interface: iface.to_string(),
            });
        }

        // Filter by language hint
        if let Some(ref lang) = hints.language {
            let filtered: Vec<_> = candidates
                .iter()
                .filter(|c| c.language.as_deref() == Some(lang.as_str()))
                .copied()
                .collect();
            if !filtered.is_empty() {
                candidates = filtered;
            }
        }

        // Filter by framework hint
        if let Some(ref fw) = hints.framework {
            let filtered: Vec<_> = candidates
                .iter()
                .filter(|c| c.framework.as_deref() == Some(fw.as_str()))
                .copied()
                .collect();
            if !filtered.is_empty() {
                candidates = filtered;
            }
        }

        // Pick by priority
        candidates.sort_by_key(|c| std::cmp::Reverse(c.priority));

        if candidates.len() > 1 && candidates[0].priority == candidates[1].priority {
            return Err(ResolveError::Ambiguous {
                interface: iface.to_string(),
                candidates: candidates.iter().map(|c| c.name.clone()).collect(),
            });
        }

        return Ok(candidates[0]);
    }

    // Priority 3: direct name lookup
    if let Some(n) = name {
        return registry
            .get_implementation(n)
            .ok_or_else(|| ResolveError::ImplNotFound {
                name: n.to_string(),
            });
    }

    Err(ResolveError::NoTarget)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_registry() -> SkillRegistry {
        let mut reg = SkillRegistry::new();
        reg.register_interface("testing".into(), None).unwrap();
        reg.register_implementation(
            "pytest-impl".into(),
            "testing".into(),
            Some("python".into()),
            Some("pytest".into()),
            None,
            0,
        )
        .unwrap();
        reg.register_implementation(
            "jest-impl".into(),
            "testing".into(),
            Some("javascript".into()),
            Some("jest".into()),
            None,
            0,
        )
        .unwrap();
        reg
    }

    #[test]
    fn test_resolve_by_impl() {
        let reg = make_registry();
        let result = resolve(
            &reg,
            None,
            Some("pytest-impl"),
            None,
            &ResolutionHints::default(),
        );
        assert_eq!(result.unwrap().name, "pytest-impl");
    }

    #[test]
    fn test_resolve_by_interface_with_language_hint() {
        let reg = make_registry();
        let hints = ResolutionHints {
            language: Some("python".into()),
            framework: None,
        };
        let result = resolve(&reg, Some("testing"), None, None, &hints);
        assert_eq!(result.unwrap().name, "pytest-impl");
    }

    #[test]
    fn test_resolve_ambiguous() {
        let reg = make_registry();
        let result = resolve(
            &reg,
            Some("testing"),
            None,
            None,
            &ResolutionHints::default(),
        );
        assert!(matches!(result, Err(ResolveError::Ambiguous { .. })));
    }

    #[test]
    fn test_resolve_impl_mismatch() {
        let reg = make_registry();
        let result = resolve(
            &reg,
            Some("nonexistent-iface"),
            Some("pytest-impl"),
            None,
            &ResolutionHints::default(),
        );
        assert!(matches!(result, Err(ResolveError::ImplMismatch { .. })));
    }

    #[test]
    fn test_resolve_no_target() {
        let reg = make_registry();
        let result = resolve(&reg, None, None, None, &ResolutionHints::default());
        assert_eq!(result.unwrap_err(), ResolveError::NoTarget);
    }
}
