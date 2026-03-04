// registry.rs — MediatorRegistry: routes URIs to the correct ResourceMediator.
//
// The registry is the single point where all mediators are registered at startup.
// When a tool call arrives, the MCP gateway extracts the URI scheme and asks
// the registry for the right mediator.

use std::collections::HashMap;

use crate::error::MediationError;
use crate::mediator::ResourceMediator;

/// Registry of ResourceMediator implementations, keyed by URI scheme.
pub struct MediatorRegistry {
    mediators: HashMap<String, Box<dyn ResourceMediator>>,
}

impl MediatorRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            mediators: HashMap::new(),
        }
    }

    /// Register a mediator for its scheme.
    ///
    /// If a mediator for the same scheme already exists, it is replaced.
    pub fn register(&mut self, mediator: Box<dyn ResourceMediator>) {
        let scheme = mediator.scheme().to_string();
        self.mediators.insert(scheme, mediator);
    }

    /// Get a mediator by scheme name.
    pub fn get(&self, scheme: &str) -> Option<&dyn ResourceMediator> {
        self.mediators.get(scheme).map(|m| m.as_ref())
    }

    /// Route a URI to the correct mediator by extracting its scheme.
    ///
    /// Parses `scheme://...` from the URI and looks up the mediator.
    pub fn route(&self, uri: &str) -> Result<&dyn ResourceMediator, MediationError> {
        let scheme = extract_scheme(uri).ok_or_else(|| MediationError::InvalidUri {
            uri: uri.to_string(),
        })?;

        self.get(scheme).ok_or_else(|| MediationError::NoMediator {
            scheme: scheme.to_string(),
        })
    }

    /// List all registered scheme names.
    pub fn schemes(&self) -> Vec<&str> {
        self.mediators.keys().map(|s| s.as_str()).collect()
    }

    /// Check if a scheme is registered.
    pub fn has_scheme(&self, scheme: &str) -> bool {
        self.mediators.contains_key(scheme)
    }

    /// Number of registered mediators.
    pub fn len(&self) -> usize {
        self.mediators.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.mediators.is_empty()
    }
}

impl Default for MediatorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract the scheme from a URI (e.g., "fs" from "fs://workspace/file.txt").
fn extract_scheme(uri: &str) -> Option<&str> {
    uri.find("://").map(|pos| &uri[..pos])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mediator::{
        ActionClassification, ApplyResult, MutationPreview, ProposedAction, StagedMutation,
    };
    use chrono::Utc;
    use uuid::Uuid;

    /// A minimal test mediator for registry tests.
    struct MockMediator {
        scheme_name: String,
    }

    impl MockMediator {
        fn new(scheme: &str) -> Self {
            Self {
                scheme_name: scheme.to_string(),
            }
        }
    }

    impl ResourceMediator for MockMediator {
        fn scheme(&self) -> &str {
            &self.scheme_name
        }

        fn stage(&self, action: ProposedAction) -> Result<StagedMutation, MediationError> {
            Ok(StagedMutation {
                mutation_id: Uuid::new_v4(),
                action,
                staged_at: Utc::now(),
                preview: None,
                staging_ref: "mock-ref".to_string(),
            })
        }

        fn preview(&self, _staged: &StagedMutation) -> Result<MutationPreview, MediationError> {
            Ok(MutationPreview {
                summary: "mock preview".to_string(),
                diff: None,
                risk_flags: vec![],
                classification: ActionClassification::ReadOnly,
            })
        }

        fn apply(&self, staged: &StagedMutation) -> Result<ApplyResult, MediationError> {
            Ok(ApplyResult {
                mutation_id: staged.mutation_id,
                success: true,
                message: "mock apply".to_string(),
                applied_at: Utc::now(),
            })
        }

        fn rollback(&self, _staged: &StagedMutation) -> Result<(), MediationError> {
            Ok(())
        }

        fn classify(&self, _action: &ProposedAction) -> ActionClassification {
            ActionClassification::ReadOnly
        }
    }

    #[test]
    fn register_and_get() {
        let mut registry = MediatorRegistry::new();
        registry.register(Box::new(MockMediator::new("fs")));

        assert!(registry.get("fs").is_some());
        assert!(registry.get("email").is_none());
    }

    #[test]
    fn route_by_uri() {
        let mut registry = MediatorRegistry::new();
        registry.register(Box::new(MockMediator::new("fs")));
        registry.register(Box::new(MockMediator::new("email")));

        let mediator = registry.route("fs://workspace/file.txt").unwrap();
        assert_eq!(mediator.scheme(), "fs");

        let mediator = registry.route("email://draft/123").unwrap();
        assert_eq!(mediator.scheme(), "email");
    }

    #[test]
    fn route_unknown_scheme_errors() {
        let registry = MediatorRegistry::new();
        let result = registry.route("db://table/users");
        assert!(result.is_err());
    }

    #[test]
    fn route_invalid_uri_errors() {
        let registry = MediatorRegistry::new();
        let result = registry.route("no-scheme-here");
        assert!(result.is_err());
    }

    #[test]
    fn schemes_list() {
        let mut registry = MediatorRegistry::new();
        registry.register(Box::new(MockMediator::new("fs")));
        registry.register(Box::new(MockMediator::new("email")));

        let schemes = registry.schemes();
        assert_eq!(schemes.len(), 2);
        assert!(schemes.contains(&"fs"));
        assert!(schemes.contains(&"email"));
    }

    #[test]
    fn replace_existing_mediator() {
        let mut registry = MediatorRegistry::new();
        registry.register(Box::new(MockMediator::new("fs")));
        registry.register(Box::new(MockMediator::new("fs")));

        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn empty_registry() {
        let registry = MediatorRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn extract_scheme_works() {
        assert_eq!(extract_scheme("fs://workspace/file"), Some("fs"));
        assert_eq!(extract_scheme("email://draft/123"), Some("email"));
        assert_eq!(extract_scheme("db://table/users"), Some("db"));
        assert_eq!(extract_scheme("no-scheme"), None);
    }
}
