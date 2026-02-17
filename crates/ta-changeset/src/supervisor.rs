// supervisor.rs — Supervisor agent for dependency graph analysis and validation.
//
// The supervisor validates artifact dispositions against their dependency graph,
// warning about coupled rejections and broken dependencies before apply.

use std::collections::{HashMap, HashSet};

use crate::draft_package::{Artifact, ArtifactDisposition, DependencyKind};

#[cfg(test)]
use crate::draft_package::ChangeDependency;

/// Result of supervisor validation with warnings and errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationResult {
    /// Whether the configuration is valid (no hard errors).
    pub valid: bool,
    /// Non-blocking warnings (e.g., rejecting an artifact others depend on).
    pub warnings: Vec<ValidationWarning>,
    /// Blocking errors (e.g., cycles in dependency graph).
    pub errors: Vec<ValidationError>,
}

impl ValidationResult {
    /// Create a valid result with no issues.
    pub fn valid() -> Self {
        Self {
            valid: true,
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Check if there are any warnings.
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    /// Check if there are any errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Add a warning to the result.
    pub fn add_warning(&mut self, warning: ValidationWarning) {
        self.warnings.push(warning);
    }

    /// Add an error to the result (sets valid = false).
    pub fn add_error(&mut self, error: ValidationError) {
        self.valid = false;
        self.errors.push(error);
    }
}

/// Warning about potentially problematic dispositions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationWarning {
    /// Rejecting an artifact that others depend on.
    CoupledRejection {
        artifact: String,
        required_by: Vec<String>,
    },
    /// Approving an artifact that depends on rejected ones.
    BrokenDependency {
        artifact: String,
        depends_on_rejected: Vec<String>,
    },
    /// An artifact marked "discuss" is blocking others.
    DiscussBlockingApproval {
        artifact: String,
        blocking: Vec<String>,
    },
}

/// Hard errors in the dependency graph or configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// Circular dependency detected.
    CyclicDependency { cycle: Vec<String> },
    /// Self-dependency (artifact depends on itself).
    SelfDependency { artifact: String },
}

/// Dependency graph built from artifact dependencies.
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    /// Adjacency list: artifact URI -> set of artifacts it depends on.
    pub depends_on: HashMap<String, HashSet<String>>,
    /// Reverse adjacency list: artifact URI -> set of artifacts that depend on it.
    pub depended_by: HashMap<String, HashSet<String>>,
}

impl DependencyGraph {
    /// Build a dependency graph from a list of artifacts.
    pub fn from_artifacts(artifacts: &[Artifact]) -> Self {
        let mut depends_on: HashMap<String, HashSet<String>> = HashMap::new();
        let mut depended_by: HashMap<String, HashSet<String>> = HashMap::new();

        for artifact in artifacts {
            let uri = artifact.resource_uri.clone();

            // Initialize entries for this artifact
            depends_on.entry(uri.clone()).or_default();
            depended_by.entry(uri.clone()).or_default();

            // Process dependencies
            for dep in &artifact.dependencies {
                match dep.kind {
                    DependencyKind::DependsOn => {
                        depends_on
                            .entry(uri.clone())
                            .or_default()
                            .insert(dep.target_uri.clone());
                        depended_by
                            .entry(dep.target_uri.clone())
                            .or_default()
                            .insert(uri.clone());
                    }
                    DependencyKind::DependedBy => {
                        depended_by
                            .entry(uri.clone())
                            .or_default()
                            .insert(dep.target_uri.clone());
                        depends_on
                            .entry(dep.target_uri.clone())
                            .or_default()
                            .insert(uri.clone());
                    }
                }
            }
        }

        Self {
            depends_on,
            depended_by,
        }
    }

    /// Get all artifacts that directly depend on the given artifact.
    pub fn get_dependents(&self, uri: &str) -> Vec<String> {
        self.depended_by
            .get(uri)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get all artifacts that the given artifact directly depends on.
    pub fn get_dependencies(&self, uri: &str) -> Vec<String> {
        self.depends_on
            .get(uri)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Detect cycles in the dependency graph using DFS.
    pub fn detect_cycles(&self) -> Vec<Vec<String>> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut cycles = Vec::new();

        for node in self.depends_on.keys() {
            if !visited.contains(node) {
                self.dfs_cycle_detect(
                    node,
                    &mut visited,
                    &mut rec_stack,
                    &mut Vec::new(),
                    &mut cycles,
                );
            }
        }

        cycles
    }

    fn dfs_cycle_detect(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());
        path.push(node.to_string());

        if let Some(neighbors) = self.depends_on.get(node) {
            for neighbor in neighbors {
                if !visited.contains(neighbor) {
                    self.dfs_cycle_detect(neighbor, visited, rec_stack, path, cycles);
                } else if rec_stack.contains(neighbor) {
                    // Found a cycle - extract it from path
                    if let Some(start_idx) = path.iter().position(|n| n == neighbor) {
                        let cycle = path[start_idx..].to_vec();
                        cycles.push(cycle);
                    }
                }
            }
        }

        path.pop();
        rec_stack.remove(node);
    }

    /// Check for self-dependencies (artifact depends on itself).
    pub fn detect_self_dependencies(&self) -> Vec<String> {
        let mut self_deps = Vec::new();

        for (uri, deps) in &self.depends_on {
            if deps.contains(uri) {
                self_deps.push(uri.clone());
            }
        }

        self_deps
    }
}

/// Supervisor agent that validates artifact dispositions against dependencies.
pub struct SupervisorAgent {
    graph: DependencyGraph,
}

impl SupervisorAgent {
    /// Create a new supervisor from a list of artifacts.
    pub fn new(artifacts: &[Artifact]) -> Self {
        Self {
            graph: DependencyGraph::from_artifacts(artifacts),
        }
    }

    /// Validate artifact dispositions against the dependency graph.
    ///
    /// Returns a ValidationResult with warnings about:
    /// - Rejecting artifacts that others depend on (coupled rejections)
    /// - Approving artifacts that depend on rejected ones (broken dependencies)
    /// - "Discuss" artifacts blocking approvals
    ///
    /// And errors for:
    /// - Cyclic dependencies
    /// - Self-dependencies
    pub fn validate(&self, artifacts: &[Artifact]) -> ValidationResult {
        let mut result = ValidationResult::valid();

        // Check for structural errors first
        for cycle in self.graph.detect_cycles() {
            result.add_error(ValidationError::CyclicDependency { cycle });
        }

        for self_dep in self.graph.detect_self_dependencies() {
            result.add_error(ValidationError::SelfDependency { artifact: self_dep });
        }

        // Build disposition map for quick lookup
        let dispositions: HashMap<String, ArtifactDisposition> = artifacts
            .iter()
            .map(|a| (a.resource_uri.clone(), a.disposition.clone()))
            .collect();

        // Check for coupled rejections and broken dependencies
        for artifact in artifacts {
            let uri = &artifact.resource_uri;
            let disposition = &artifact.disposition;

            match disposition {
                ArtifactDisposition::Rejected => {
                    // Check if any approved/discuss artifacts depend on this one
                    let dependents = self.graph.get_dependents(uri);
                    let affected: Vec<String> = dependents
                        .into_iter()
                        .filter(|dep_uri| {
                            matches!(
                                dispositions.get(dep_uri),
                                Some(ArtifactDisposition::Approved)
                                    | Some(ArtifactDisposition::Discuss)
                                    | Some(ArtifactDisposition::Pending)
                            )
                        })
                        .collect();

                    if !affected.is_empty() {
                        result.add_warning(ValidationWarning::CoupledRejection {
                            artifact: uri.clone(),
                            required_by: affected,
                        });
                    }
                }
                ArtifactDisposition::Approved => {
                    // Check if this artifact depends on any rejected ones
                    let dependencies = self.graph.get_dependencies(uri);
                    let rejected_deps: Vec<String> = dependencies
                        .into_iter()
                        .filter(|dep_uri| {
                            matches!(
                                dispositions.get(dep_uri),
                                Some(ArtifactDisposition::Rejected)
                            )
                        })
                        .collect();

                    if !rejected_deps.is_empty() {
                        result.add_warning(ValidationWarning::BrokenDependency {
                            artifact: uri.clone(),
                            depends_on_rejected: rejected_deps,
                        });
                    }
                }
                ArtifactDisposition::Discuss => {
                    // Check if any approved artifacts depend on this discuss item
                    let dependents = self.graph.get_dependents(uri);
                    let blocked: Vec<String> = dependents
                        .into_iter()
                        .filter(|dep_uri| {
                            matches!(
                                dispositions.get(dep_uri),
                                Some(ArtifactDisposition::Approved)
                            )
                        })
                        .collect();

                    if !blocked.is_empty() {
                        result.add_warning(ValidationWarning::DiscussBlockingApproval {
                            artifact: uri.clone(),
                            blocking: blocked,
                        });
                    }
                }
                ArtifactDisposition::Pending => {
                    // Pending is neutral - no validation needed
                }
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_artifact(
        uri: &str,
        disposition: ArtifactDisposition,
        deps: Vec<(&str, DependencyKind)>,
    ) -> Artifact {
        Artifact {
            resource_uri: uri.to_string(),
            change_type: crate::draft_package::ChangeType::Modify,
            diff_ref: "test".to_string(),
            tests_run: Vec::new(),
            disposition,
            rationale: None,
            dependencies: deps
                .into_iter()
                .map(|(target, kind)| ChangeDependency {
                    target_uri: target.to_string(),
                    kind,
                })
                .collect(),
            explanation_tiers: None,
            comments: None,
        }
    }

    #[test]
    fn test_dependency_graph_simple() {
        let artifacts = vec![
            make_artifact(
                "fs://workspace/a.rs",
                ArtifactDisposition::Pending,
                vec![("fs://workspace/b.rs", DependencyKind::DependsOn)],
            ),
            make_artifact("fs://workspace/b.rs", ArtifactDisposition::Pending, vec![]),
        ];

        let graph = DependencyGraph::from_artifacts(&artifacts);

        assert_eq!(
            graph.get_dependencies("fs://workspace/a.rs"),
            vec!["fs://workspace/b.rs"]
        );
        assert_eq!(
            graph.get_dependents("fs://workspace/b.rs"),
            vec!["fs://workspace/a.rs"]
        );
    }

    #[test]
    fn test_coupled_rejection_warning() {
        let artifacts = vec![
            make_artifact(
                "fs://workspace/a.rs",
                ArtifactDisposition::Approved,
                vec![("fs://workspace/b.rs", DependencyKind::DependsOn)],
            ),
            make_artifact("fs://workspace/b.rs", ArtifactDisposition::Rejected, vec![]),
        ];

        let supervisor = SupervisorAgent::new(&artifacts);
        let result = supervisor.validate(&artifacts);

        assert!(result.valid);
        assert_eq!(result.warnings.len(), 2);

        // Should warn about both: rejecting B that A depends on, and approving A that depends on rejected B
        assert!(result
            .warnings
            .iter()
            .any(|w| matches!(w, ValidationWarning::CoupledRejection { .. })));
        assert!(result
            .warnings
            .iter()
            .any(|w| matches!(w, ValidationWarning::BrokenDependency { .. })));
    }

    #[test]
    fn test_no_warning_when_consistent() {
        let artifacts = vec![
            make_artifact(
                "fs://workspace/a.rs",
                ArtifactDisposition::Approved,
                vec![("fs://workspace/b.rs", DependencyKind::DependsOn)],
            ),
            make_artifact("fs://workspace/b.rs", ArtifactDisposition::Approved, vec![]),
        ];

        let supervisor = SupervisorAgent::new(&artifacts);
        let result = supervisor.validate(&artifacts);

        assert!(result.valid);
        assert_eq!(result.warnings.len(), 0);
    }

    #[test]
    fn test_self_dependency_error() {
        let artifacts = vec![make_artifact(
            "fs://workspace/a.rs",
            ArtifactDisposition::Pending,
            vec![("fs://workspace/a.rs", DependencyKind::DependsOn)],
        )];

        let supervisor = SupervisorAgent::new(&artifacts);
        let result = supervisor.validate(&artifacts);

        assert!(!result.valid);
        // Self-dependency is detected as both a self-dep and a cycle
        assert!(!result.errors.is_empty());
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::SelfDependency { .. })));
    }

    #[test]
    fn test_cycle_detection() {
        let artifacts = vec![
            make_artifact(
                "fs://workspace/a.rs",
                ArtifactDisposition::Pending,
                vec![("fs://workspace/b.rs", DependencyKind::DependsOn)],
            ),
            make_artifact(
                "fs://workspace/b.rs",
                ArtifactDisposition::Pending,
                vec![("fs://workspace/c.rs", DependencyKind::DependsOn)],
            ),
            make_artifact(
                "fs://workspace/c.rs",
                ArtifactDisposition::Pending,
                vec![("fs://workspace/a.rs", DependencyKind::DependsOn)],
            ),
        ];

        let supervisor = SupervisorAgent::new(&artifacts);
        let result = supervisor.validate(&artifacts);

        assert!(!result.valid);
        assert_eq!(result.errors.len(), 1);
        assert!(matches!(
            result.errors[0],
            ValidationError::CyclicDependency { .. }
        ));
    }

    #[test]
    fn test_discuss_blocking_approval() {
        let artifacts = vec![
            make_artifact(
                "fs://workspace/a.rs",
                ArtifactDisposition::Approved,
                vec![("fs://workspace/b.rs", DependencyKind::DependsOn)],
            ),
            make_artifact("fs://workspace/b.rs", ArtifactDisposition::Discuss, vec![]),
        ];

        let supervisor = SupervisorAgent::new(&artifacts);
        let result = supervisor.validate(&artifacts);

        assert!(result.valid);
        assert_eq!(result.warnings.len(), 1);
        assert!(matches!(
            result.warnings[0],
            ValidationWarning::DiscussBlockingApproval { .. }
        ));
    }

    #[test]
    fn test_depended_by_relationship() {
        let artifacts = vec![
            make_artifact("fs://workspace/a.rs", ArtifactDisposition::Pending, vec![]),
            make_artifact(
                "fs://workspace/b.rs",
                ArtifactDisposition::Pending,
                vec![("fs://workspace/a.rs", DependencyKind::DependedBy)],
            ),
        ];

        let graph = DependencyGraph::from_artifacts(&artifacts);

        // b.rs is depended by a.rs means a.rs depends on b.rs
        assert_eq!(
            graph.get_dependencies("fs://workspace/a.rs"),
            vec!["fs://workspace/b.rs"]
        );
        assert_eq!(
            graph.get_dependents("fs://workspace/b.rs"),
            vec!["fs://workspace/a.rs"]
        );
    }

    #[test]
    fn test_transitive_dependency_chain() {
        // A → B → C: rejecting C should warn about B (direct dependency)
        // A won't be warned because its direct dependency (B) is approved
        let artifacts = vec![
            make_artifact(
                "fs://workspace/a.rs",
                ArtifactDisposition::Approved,
                vec![("fs://workspace/b.rs", DependencyKind::DependsOn)],
            ),
            make_artifact(
                "fs://workspace/b.rs",
                ArtifactDisposition::Approved,
                vec![("fs://workspace/c.rs", DependencyKind::DependsOn)],
            ),
            make_artifact("fs://workspace/c.rs", ArtifactDisposition::Rejected, vec![]),
        ];

        let supervisor = SupervisorAgent::new(&artifacts);
        let result = supervisor.validate(&artifacts);

        // Should have 2 warnings: C coupled rejection (breaks B) + B broken dependency (depends on rejected C)
        assert!(result.valid);
        assert_eq!(result.warnings.len(), 2);
    }

    #[test]
    fn test_disconnected_subgraphs() {
        // Two independent chains: A→B and C→D
        let artifacts = vec![
            make_artifact(
                "fs://workspace/a.rs",
                ArtifactDisposition::Approved,
                vec![("fs://workspace/b.rs", DependencyKind::DependsOn)],
            ),
            make_artifact("fs://workspace/b.rs", ArtifactDisposition::Rejected, vec![]),
            make_artifact(
                "fs://workspace/c.rs",
                ArtifactDisposition::Approved,
                vec![("fs://workspace/d.rs", DependencyKind::DependsOn)],
            ),
            make_artifact("fs://workspace/d.rs", ArtifactDisposition::Approved, vec![]),
        ];

        let supervisor = SupervisorAgent::new(&artifacts);
        let result = supervisor.validate(&artifacts);

        // Should only warn about A→B, not C→D
        assert!(result.valid);
        assert_eq!(result.warnings.len(), 2); // B coupled rejection + A broken dependency
    }

    #[test]
    fn test_mixed_dispositions() {
        // Complex scenario: some approved, some rejected, some pending, some discuss
        let artifacts = vec![
            make_artifact(
                "fs://workspace/a.rs",
                ArtifactDisposition::Approved,
                vec![("fs://workspace/b.rs", DependencyKind::DependsOn)],
            ),
            make_artifact("fs://workspace/b.rs", ArtifactDisposition::Discuss, vec![]),
            make_artifact(
                "fs://workspace/c.rs",
                ArtifactDisposition::Approved,
                vec![("fs://workspace/d.rs", DependencyKind::DependsOn)],
            ),
            make_artifact("fs://workspace/d.rs", ArtifactDisposition::Pending, vec![]),
        ];

        let supervisor = SupervisorAgent::new(&artifacts);
        let result = supervisor.validate(&artifacts);

        // Should warn about discuss blocking approval
        assert!(result.valid);
        assert_eq!(result.warnings.len(), 1);
        assert!(matches!(
            result.warnings[0],
            ValidationWarning::DiscussBlockingApproval { .. }
        ));
    }

    #[test]
    fn test_empty_artifacts() {
        let artifacts = vec![];
        let supervisor = SupervisorAgent::new(&artifacts);
        let result = supervisor.validate(&artifacts);

        assert!(result.valid);
        assert_eq!(result.warnings.len(), 0);
        assert_eq!(result.errors.len(), 0);
    }

    #[test]
    fn test_all_approved_no_dependencies() {
        let artifacts = vec![
            make_artifact("fs://workspace/a.rs", ArtifactDisposition::Approved, vec![]),
            make_artifact("fs://workspace/b.rs", ArtifactDisposition::Approved, vec![]),
            make_artifact("fs://workspace/c.rs", ArtifactDisposition::Approved, vec![]),
        ];

        let supervisor = SupervisorAgent::new(&artifacts);
        let result = supervisor.validate(&artifacts);

        assert!(result.valid);
        assert_eq!(result.warnings.len(), 0);
        assert_eq!(result.errors.len(), 0);
    }

    #[test]
    fn test_diamond_dependency() {
        // Diamond pattern: A→B, A→C, B→D, C→D
        let artifacts = vec![
            make_artifact(
                "fs://workspace/a.rs",
                ArtifactDisposition::Approved,
                vec![
                    ("fs://workspace/b.rs", DependencyKind::DependsOn),
                    ("fs://workspace/c.rs", DependencyKind::DependsOn),
                ],
            ),
            make_artifact(
                "fs://workspace/b.rs",
                ArtifactDisposition::Approved,
                vec![("fs://workspace/d.rs", DependencyKind::DependsOn)],
            ),
            make_artifact(
                "fs://workspace/c.rs",
                ArtifactDisposition::Approved,
                vec![("fs://workspace/d.rs", DependencyKind::DependsOn)],
            ),
            make_artifact("fs://workspace/d.rs", ArtifactDisposition::Rejected, vec![]),
        ];

        let supervisor = SupervisorAgent::new(&artifacts);
        let result = supervisor.validate(&artifacts);

        // Should warn about D being rejected but depended on by B and C
        assert!(result.valid);
        assert!(result.warnings.len() >= 3); // At least coupled rejection for D and broken deps for B, C
    }
}
