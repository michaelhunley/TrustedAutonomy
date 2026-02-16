//! terminal.rs — Terminal output adapter with colored, tiered display.

use crate::error::ChangeSetError;
use crate::output_adapters::{DetailLevel, OutputAdapter, RenderContext};
use crate::pr_package::{Artifact, ChangeType};

pub struct TerminalAdapter {}

impl TerminalAdapter {
    pub fn new() -> Self {
        Self {}
    }

    fn render_header(&self, ctx: &RenderContext) -> String {
        let pkg = ctx.package;
        let status_color = match pkg.status {
            crate::pr_package::PRStatus::Draft => "\x1b[33m", // Yellow
            crate::pr_package::PRStatus::PendingReview => "\x1b[36m", // Cyan
            crate::pr_package::PRStatus::Approved { .. } => "\x1b[32m", // Green
            crate::pr_package::PRStatus::Denied { .. } => "\x1b[31m", // Red
            crate::pr_package::PRStatus::Applied { .. } => "\x1b[32m", // Green
            crate::pr_package::PRStatus::Superseded { .. } => "\x1b[90m", // Gray
        };
        let reset = "\x1b[0m";
        let bold = "\x1b[1m";

        format!(
            "{bold}PR Package: {}{reset}\n\
            Status: {}{}{reset}\n\
            Goal: {}\n\
            Created: {}\n\n\
            {bold}Summary:{reset}\n\
            {}\n\n\
            {bold}Why:{reset}\n\
            {}\n\n\
            {bold}Impact:{reset}\n\
            {}\n\n",
            pkg.package_id,
            status_color,
            pkg.status,
            pkg.goal.title,
            pkg.created_at.format("%Y-%m-%d %H:%M:%S"),
            pkg.summary.what_changed,
            pkg.summary.why,
            pkg.summary.impact,
            bold = bold,
            reset = reset
        )
    }

    fn render_artifact_top(&self, artifact: &Artifact) -> String {
        let change_icon = match artifact.change_type {
            ChangeType::Add => "\x1b[32m+\x1b[0m", // Green +
            ChangeType::Modify => "\x1b[33m~\x1b[0m", // Yellow ~
            ChangeType::Delete => "\x1b[31m-\x1b[0m", // Red -
            ChangeType::Rename => "\x1b[36m>\x1b[0m", // Cyan >
        };

        let disposition_badge = match artifact.disposition {
            crate::pr_package::ArtifactDisposition::Pending => "\x1b[90m[pending]\x1b[0m",
            crate::pr_package::ArtifactDisposition::Approved => "\x1b[32m[approved]\x1b[0m",
            crate::pr_package::ArtifactDisposition::Rejected => "\x1b[31m[rejected]\x1b[0m",
            crate::pr_package::ArtifactDisposition::Discuss => "\x1b[33m[discuss]\x1b[0m",
        };

        let summary = artifact
            .explanation_tiers
            .as_ref()
            .map(|t| t.summary.as_str())
            .or(artifact.rationale.as_deref())
            .unwrap_or("(no explanation)");

        format!(
            "  {} {} {} - {}",
            change_icon, disposition_badge, artifact.resource_uri, summary
        )
    }

    fn render_artifact_medium(&self, artifact: &Artifact) -> String {
        let mut output = self.render_artifact_top(artifact);
        output.push('\n');

        if let Some(tiers) = &artifact.explanation_tiers {
            output.push_str(&format!("    \x1b[2mExplanation:\x1b[0m {}\n", tiers.explanation));

            if !tiers.tags.is_empty() {
                output.push_str(&format!(
                    "    \x1b[2mTags:\x1b[0m {}\n",
                    tiers.tags.join(", ")
                ));
            }

            if !tiers.related_artifacts.is_empty() {
                output.push_str("    \x1b[2mRelated:\x1b[0m\n");
                for related in &tiers.related_artifacts {
                    output.push_str(&format!("      - {}\n", related));
                }
            }
        } else if let Some(rationale) = &artifact.rationale {
            output.push_str(&format!("    \x1b[2mRationale:\x1b[0m {}\n", rationale));
        }

        if !artifact.dependencies.is_empty() {
            output.push_str("    \x1b[2mDependencies:\x1b[0m\n");
            for dep in &artifact.dependencies {
                output.push_str(&format!("      {:?}: {}\n", dep.kind, dep.target_uri));
            }
        }

        output
    }

    fn render_artifact_full(&self, artifact: &Artifact, ctx: &RenderContext) -> String {
        let mut output = self.render_artifact_medium(artifact);

        // Fetch and display full diff if provider is available
        if let Some(provider) = ctx.diff_provider {
            match provider.get_diff(&artifact.diff_ref) {
                Ok(diff) => {
                    output.push_str("\n    \x1b[1mDiff:\x1b[0m\n");
                    for line in diff.lines() {
                        if line.starts_with('+') && !line.starts_with("+++") {
                            output.push_str(&format!("    \x1b[32m{}\x1b[0m\n", line));
                        } else if line.starts_with('-') && !line.starts_with("---") {
                            output.push_str(&format!("    \x1b[31m{}\x1b[0m\n", line));
                        } else if line.starts_with("@@") {
                            output.push_str(&format!("    \x1b[36m{}\x1b[0m\n", line));
                        } else {
                            output.push_str(&format!("    {}\n", line));
                        }
                    }
                }
                Err(e) => {
                    output.push_str(&format!("    \x1b[31m[Error loading diff: {}]\x1b[0m\n", e));
                }
            }
        } else {
            output.push_str(&format!(
                "    \x1b[2m[Diff available at: {}]\x1b[0m\n",
                artifact.diff_ref
            ));
        }

        output
    }
}

impl OutputAdapter for TerminalAdapter {
    fn render(&self, ctx: &RenderContext) -> Result<String, ChangeSetError> {
        let mut output = String::new();

        // Header
        output.push_str(&self.render_header(ctx));

        // Artifacts section
        let artifacts = &ctx.package.changes.artifacts;
        let filtered_artifacts: Vec<&Artifact> = if let Some(filter) = &ctx.file_filter {
            artifacts
                .iter()
                .filter(|a| a.resource_uri.contains(filter))
                .collect()
        } else {
            artifacts.iter().collect()
        };

        if filtered_artifacts.is_empty() && ctx.file_filter.is_some() {
            return Err(ChangeSetError::InvalidData(format!(
                "No artifacts match filter: {}",
                ctx.file_filter.as_ref().unwrap()
            )));
        }

        output.push_str(&format!(
            "\x1b[1mChanges ({} artifacts):\x1b[0m\n",
            filtered_artifacts.len()
        ));

        for artifact in filtered_artifacts {
            match ctx.detail_level {
                DetailLevel::Top => {
                    output.push_str(&self.render_artifact_top(artifact));
                    output.push('\n');
                }
                DetailLevel::Medium => {
                    output.push_str(&self.render_artifact_medium(artifact));
                    output.push('\n');
                }
                DetailLevel::Full => {
                    output.push_str(&self.render_artifact_full(artifact, ctx));
                    output.push('\n');
                }
            }
        }

        // Footer with review guidance
        if ctx.detail_level == DetailLevel::Top || ctx.detail_level == DetailLevel::Medium {
            output.push_str("\n\x1b[2mTip: Use --detail full to see complete diffs\x1b[0m\n");
        }

        Ok(output)
    }

    fn name(&self) -> &str {
        "terminal"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pr_package::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn test_package() -> PRPackage {
        PRPackage {
            package_version: "1.0.0".to_string(),
            package_id: Uuid::new_v4(),
            created_at: Utc::now(),
            goal: Goal {
                goal_id: "goal-1".to_string(),
                title: "Test Goal".to_string(),
                objective: "Test objective".to_string(),
                success_criteria: vec![],
                constraints: vec![],
            },
            iteration: Iteration {
                iteration_id: "iter-1".to_string(),
                sequence: 1,
                workspace_ref: WorkspaceRef {
                    ref_type: "staging".to_string(),
                    ref_name: "staging/1".to_string(),
                    base_ref: None,
                },
            },
            agent_identity: AgentIdentity {
                agent_id: "agent-1".to_string(),
                agent_type: "coder".to_string(),
                constitution_id: "default".to_string(),
                capability_manifest_hash: "hash123".to_string(),
                orchestrator_run_id: None,
            },
            summary: Summary {
                what_changed: "Updated auth system".to_string(),
                why: "To improve security".to_string(),
                impact: "All users must re-login".to_string(),
                rollback_plan: "Revert commit".to_string(),
                open_questions: vec![],
            },
            plan: Plan {
                completed_steps: vec![],
                next_steps: vec![],
                decision_log: vec![],
            },
            changes: Changes {
                artifacts: vec![Artifact {
                    resource_uri: "fs://workspace/src/auth.rs".to_string(),
                    change_type: ChangeType::Modify,
                    diff_ref: "changeset:0".to_string(),
                    tests_run: vec![],
                    disposition: ArtifactDisposition::Pending,
                    rationale: Some("JWT migration".to_string()),
                    dependencies: vec![],
                    explanation_tiers: Some(ExplanationTiers {
                        summary: "Migrated to JWT auth".to_string(),
                        explanation: "Full JWT implementation with validation".to_string(),
                        tags: vec!["security".to_string()],
                        related_artifacts: vec![],
                    }),
                }],
                patch_sets: vec![],
            },
            risk: Risk {
                risk_score: 10,
                findings: vec![],
                policy_decisions: vec![],
            },
            provenance: Provenance {
                inputs: vec![],
                tool_trace_hash: "trace123".to_string(),
            },
            review_requests: ReviewRequests {
                requested_actions: vec![],
                reviewers: vec![],
                required_approvals: 1,
                notes_to_reviewer: None,
            },
            signatures: Signatures {
                package_hash: "hash123".to_string(),
                agent_signature: "sig123".to_string(),
                gateway_attestation: None,
            },
            status: PRStatus::PendingReview,
        }
    }

    #[test]
    fn render_top_level() {
        let adapter = TerminalAdapter::new();
        let package = test_package();
        let ctx = RenderContext {
            package: &package,
            detail_level: DetailLevel::Top,
            file_filter: None,
            diff_provider: None,
        };

        let output = adapter.render(&ctx).unwrap();
        assert!(output.contains("PR Package"));
        assert!(output.contains("pending_review"));
        assert!(output.contains("src/auth.rs"));
        assert!(output.contains("Migrated to JWT auth"));
    }

    #[test]
    fn render_medium_level() {
        let adapter = TerminalAdapter::new();
        let package = test_package();
        let ctx = RenderContext {
            package: &package,
            detail_level: DetailLevel::Medium,
            file_filter: None,
            diff_provider: None,
        };

        let output = adapter.render(&ctx).unwrap();
        assert!(output.contains("Full JWT implementation"));
        assert!(output.contains("security"));
    }

    #[test]
    fn file_filter_works() {
        let adapter = TerminalAdapter::new();
        let package = test_package();
        let ctx = RenderContext {
            package: &package,
            detail_level: DetailLevel::Top,
            file_filter: Some("auth.rs".to_string()),
            diff_provider: None,
        };

        let output = adapter.render(&ctx).unwrap();
        assert!(output.contains("auth.rs"));
    }

    #[test]
    fn file_filter_no_match_returns_error() {
        let adapter = TerminalAdapter::new();
        let package = test_package();
        let ctx = RenderContext {
            package: &package,
            detail_level: DetailLevel::Top,
            file_filter: Some("nonexistent.rs".to_string()),
            diff_provider: None,
        };

        let result = adapter.render(&ctx);
        assert!(result.is_err());
    }
}
