// drift.rs — Behavioral Drift Detection (v0.4.2).
//
// Detects when an agent's behavior diverges from its historical baseline.
// Five drift signals are computed from the audit log and draft package history:
//
// 1. Resource scope drift — URIs outside historical pattern
// 2. Escalation frequency change — increase/decrease in policy escalations
// 3. Rejection rate drift — draft packages being rejected more often
// 4. Change volume anomaly — unexpectedly large/small diffs
// 5. Dependency pattern shift — unusual rate of new external dependencies
//
// Standards alignment:
// - NIST AI RMF MEASURE 2.6: Monitoring AI system behavior for drift
// - ISO/IEC 42001 A.6.2.6: Performance monitoring of AI systems
// - EU AI Act Article 9: Risk management with continuous monitoring

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::event::{AuditAction, AuditEvent};

// ── Data Model ──

/// A stored behavioral baseline for an agent, computed from historical data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BehavioralBaseline {
    /// Which agent this baseline describes.
    pub agent_id: String,
    /// When this baseline was computed.
    pub computed_at: DateTime<Utc>,
    /// Number of goals/sessions in the baseline sample.
    pub goal_count: usize,
    /// Typical URI path prefixes accessed by this agent.
    pub resource_patterns: Vec<String>,
    /// Average number of artifacts per draft package.
    pub avg_artifact_count: f64,
    /// Average risk score across draft packages.
    pub avg_risk_score: f64,
    /// Fraction of actions that triggered policy escalation.
    pub escalation_rate: f64,
    /// Fraction of draft packages denied by reviewers.
    pub rejection_rate: f64,
}

/// Which behavioral dimension drifted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DriftSignal {
    /// Agent accessing URIs outside its historical pattern.
    ResourceScope,
    /// Significant change in policy escalation frequency.
    EscalationFrequency,
    /// Draft rejection rate changed significantly.
    RejectionRate,
    /// Artifact count deviates from historical average.
    ChangeVolume,
    /// Unusual rate of dependency-related changes.
    DependencyPattern,
    /// Agent accessed URIs not declared in the goal's access constitution (v0.4.3).
    ConstitutionViolation,
}

/// How severe the drift is.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DriftSeverity {
    /// Within normal variance.
    Normal,
    /// Notable deviation, worth monitoring.
    Warning,
    /// Significant deviation, likely indicates changed behavior.
    Alert,
}

impl std::fmt::Display for DriftSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DriftSeverity::Normal => write!(f, "normal"),
            DriftSeverity::Warning => write!(f, "warning"),
            DriftSeverity::Alert => write!(f, "alert"),
        }
    }
}

/// A single detected drift finding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DriftFinding {
    pub signal: DriftSignal,
    pub severity: DriftSeverity,
    /// Human-readable description of the drift.
    pub description: String,
    /// Baseline value (for numeric signals).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_value: Option<f64>,
    /// Current value (for numeric signals).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_value: Option<f64>,
}

/// A complete drift report comparing recent behavior to baseline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DriftReport {
    pub agent_id: String,
    pub computed_at: DateTime<Utc>,
    /// How many recent goals were compared.
    pub window_size: usize,
    /// The findings (may be empty if no drift detected).
    pub findings: Vec<DriftFinding>,
    /// Overall worst severity across all findings.
    pub overall_severity: DriftSeverity,
}

// ── Baseline Store ──

/// Reads and writes baseline JSON files from `.ta/baselines/<agent-id>.json`.
pub struct BaselineStore {
    dir: PathBuf,
}

impl BaselineStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Load a baseline for the given agent. Returns None if not found.
    pub fn load(&self, agent_id: &str) -> Result<Option<BehavioralBaseline>, crate::AuditError> {
        let path = self.dir.join(format!("{}.json", agent_id));
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read_to_string(&path).map_err(|source| crate::AuditError::OpenFailed {
            path: path.clone(),
            source,
        })?;
        let baseline: BehavioralBaseline = serde_json::from_str(&data)?;
        Ok(Some(baseline))
    }

    /// Save a baseline for the given agent.
    pub fn save(&self, baseline: &BehavioralBaseline) -> Result<(), crate::AuditError> {
        fs::create_dir_all(&self.dir).map_err(|source| crate::AuditError::OpenFailed {
            path: self.dir.clone(),
            source,
        })?;
        let path = self.dir.join(format!("{}.json", baseline.agent_id));
        let json = serde_json::to_string_pretty(baseline)?;
        fs::write(&path, json).map_err(|source| crate::AuditError::OpenFailed { path, source })?;
        Ok(())
    }

    /// List all agent IDs that have stored baselines.
    pub fn list_agents(&self) -> Result<Vec<String>, crate::AuditError> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }
        let mut agents = Vec::new();
        for entry in fs::read_dir(&self.dir).map_err(|source| crate::AuditError::OpenFailed {
            path: self.dir.clone(),
            source,
        })? {
            let entry = entry.map_err(crate::AuditError::WriteFailed)?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    agents.push(stem.to_string());
                }
            }
        }
        agents.sort();
        Ok(agents)
    }
}

// ── Input types for compute functions ──
// These keep ta-audit decoupled from ta-changeset — the CLI provides the data.

/// Minimal draft info needed for baseline/drift computation.
/// The CLI maps `DraftPackage` fields into this.
#[derive(Debug, Clone)]
pub struct DraftSummary {
    pub agent_id: String,
    pub artifact_count: usize,
    pub risk_score: u32,
    pub rejected: bool,
    /// URIs of artifacts that look like dependency files (Cargo.toml, package.json, etc.).
    pub dependency_artifact_count: usize,
}

// ── Compute Functions ──

/// Extract a URI prefix suitable for pattern matching.
/// `"fs://workspace/src/commands/draft.rs"` → `"fs://workspace/src/commands/"`.
fn uri_prefix(uri: &str) -> String {
    if let Some(pos) = uri.rfind('/') {
        uri[..=pos].to_string()
    } else {
        uri.to_string()
    }
}

/// Compute a behavioral baseline from historical audit events and draft summaries.
pub fn compute_baseline(
    agent_id: &str,
    events: &[AuditEvent],
    drafts: &[DraftSummary],
) -> BehavioralBaseline {
    // Filter events for this agent.
    let agent_events: Vec<&AuditEvent> = events.iter().filter(|e| e.agent_id == agent_id).collect();

    // Filter drafts for this agent.
    let agent_drafts: Vec<&DraftSummary> =
        drafts.iter().filter(|d| d.agent_id == agent_id).collect();

    // Resource patterns: collect URI prefixes from all accessed resources.
    let mut prefix_counts: HashMap<String, usize> = HashMap::new();
    for event in &agent_events {
        if let Some(uri) = &event.target_uri {
            let prefix = uri_prefix(uri);
            *prefix_counts.entry(prefix).or_insert(0) += 1;
        }
    }
    let mut resource_patterns: Vec<String> = prefix_counts.into_keys().collect();
    resource_patterns.sort();

    // Escalation rate: fraction of events that are PolicyDecision.
    let total_actions = agent_events.len();
    let escalation_count = agent_events
        .iter()
        .filter(|e| e.action == AuditAction::PolicyDecision)
        .count();
    let escalation_rate = if total_actions > 0 {
        escalation_count as f64 / total_actions as f64
    } else {
        0.0
    };

    // Draft-derived metrics.
    let total_drafts = agent_drafts.len();
    let avg_artifact_count = if total_drafts > 0 {
        agent_drafts
            .iter()
            .map(|d| d.artifact_count as f64)
            .sum::<f64>()
            / total_drafts as f64
    } else {
        0.0
    };
    let avg_risk_score = if total_drafts > 0 {
        agent_drafts
            .iter()
            .map(|d| d.risk_score as f64)
            .sum::<f64>()
            / total_drafts as f64
    } else {
        0.0
    };
    let rejected_count = agent_drafts.iter().filter(|d| d.rejected).count();
    let rejection_rate = if total_drafts > 0 {
        rejected_count as f64 / total_drafts as f64
    } else {
        0.0
    };

    BehavioralBaseline {
        agent_id: agent_id.to_string(),
        computed_at: Utc::now(),
        goal_count: total_drafts.max(1), // At least 1 if there are events.
        resource_patterns,
        avg_artifact_count,
        avg_risk_score,
        escalation_rate,
        rejection_rate,
    }
}

/// Thresholds for drift detection.
const WARNING_RATE_DELTA: f64 = 0.20; // 20% change triggers warning
const ALERT_RATE_DELTA: f64 = 0.50; // 50% change triggers alert
const VOLUME_WARNING_FACTOR: f64 = 2.0; // 2x deviation triggers warning
const VOLUME_ALERT_FACTOR: f64 = 3.0; // 3x deviation triggers alert

/// Compute a drift report by comparing recent behavior to a stored baseline.
pub fn compute_drift(
    baseline: &BehavioralBaseline,
    recent_events: &[AuditEvent],
    recent_drafts: &[DraftSummary],
    window_size: usize,
) -> DriftReport {
    let agent_id = &baseline.agent_id;
    let mut findings = Vec::new();

    // Filter to this agent.
    let agent_events: Vec<&AuditEvent> = recent_events
        .iter()
        .filter(|e| e.agent_id == *agent_id)
        .collect();
    let agent_drafts: Vec<&DraftSummary> = recent_drafts
        .iter()
        .filter(|d| d.agent_id == *agent_id)
        .collect();

    // 1. Resource scope drift — URIs outside historical patterns.
    let baseline_prefixes: HashSet<&str> = baseline
        .resource_patterns
        .iter()
        .map(|s| s.as_str())
        .collect();
    let mut novel_uris: Vec<String> = Vec::new();
    for event in &agent_events {
        if let Some(uri) = &event.target_uri {
            let prefix = uri_prefix(uri);
            if !baseline_prefixes.contains(prefix.as_str()) {
                novel_uris.push(uri.clone());
            }
        }
    }
    if !novel_uris.is_empty() {
        let novel_count = novel_uris.len();
        let total = agent_events
            .iter()
            .filter(|e| e.target_uri.is_some())
            .count();
        let novel_fraction = if total > 0 {
            novel_count as f64 / total as f64
        } else {
            0.0
        };
        let severity = if novel_fraction > 0.5 {
            DriftSeverity::Alert
        } else if novel_fraction > 0.2 {
            DriftSeverity::Warning
        } else {
            DriftSeverity::Normal
        };
        // Deduplicate for display.
        let unique_prefixes: HashSet<String> = novel_uris.iter().map(|u| uri_prefix(u)).collect();
        let example_prefixes: Vec<&str> =
            unique_prefixes.iter().take(3).map(|s| s.as_str()).collect();
        findings.push(DriftFinding {
            signal: DriftSignal::ResourceScope,
            severity,
            description: format!(
                "{} access(es) to {} novel URI prefix(es) outside baseline (e.g., {})",
                novel_count,
                unique_prefixes.len(),
                example_prefixes.join(", "),
            ),
            baseline_value: Some(baseline.resource_patterns.len() as f64),
            current_value: Some((baseline.resource_patterns.len() + unique_prefixes.len()) as f64),
        });
    }

    // 2. Escalation frequency change.
    let total_recent = agent_events.len();
    let recent_escalations = agent_events
        .iter()
        .filter(|e| e.action == AuditAction::PolicyDecision)
        .count();
    let recent_escalation_rate = if total_recent > 0 {
        recent_escalations as f64 / total_recent as f64
    } else {
        0.0
    };
    if total_recent > 0 {
        let delta = (recent_escalation_rate - baseline.escalation_rate).abs();
        let severity = rate_severity(delta);
        if severity > DriftSeverity::Normal {
            findings.push(DriftFinding {
                signal: DriftSignal::EscalationFrequency,
                severity,
                description: format!(
                    "Escalation rate changed from {:.1}% to {:.1}%",
                    baseline.escalation_rate * 100.0,
                    recent_escalation_rate * 100.0,
                ),
                baseline_value: Some(baseline.escalation_rate),
                current_value: Some(recent_escalation_rate),
            });
        }
    }

    // 3. Rejection rate drift.
    let total_recent_drafts = agent_drafts.len();
    let recent_rejected = agent_drafts.iter().filter(|d| d.rejected).count();
    let recent_rejection_rate = if total_recent_drafts > 0 {
        recent_rejected as f64 / total_recent_drafts as f64
    } else {
        0.0
    };
    if total_recent_drafts > 0 {
        let delta = (recent_rejection_rate - baseline.rejection_rate).abs();
        let severity = rate_severity(delta);
        if severity > DriftSeverity::Normal {
            findings.push(DriftFinding {
                signal: DriftSignal::RejectionRate,
                severity,
                description: format!(
                    "Rejection rate changed from {:.1}% to {:.1}%",
                    baseline.rejection_rate * 100.0,
                    recent_rejection_rate * 100.0,
                ),
                baseline_value: Some(baseline.rejection_rate),
                current_value: Some(recent_rejection_rate),
            });
        }
    }

    // 4. Change volume anomaly.
    if total_recent_drafts > 0 && baseline.avg_artifact_count > 0.0 {
        let recent_avg = agent_drafts
            .iter()
            .map(|d| d.artifact_count as f64)
            .sum::<f64>()
            / total_recent_drafts as f64;
        let ratio = recent_avg / baseline.avg_artifact_count;
        let severity = if ratio >= VOLUME_ALERT_FACTOR || ratio <= 1.0 / VOLUME_ALERT_FACTOR {
            DriftSeverity::Alert
        } else if ratio >= VOLUME_WARNING_FACTOR || ratio <= 1.0 / VOLUME_WARNING_FACTOR {
            DriftSeverity::Warning
        } else {
            DriftSeverity::Normal
        };
        if severity > DriftSeverity::Normal {
            findings.push(DriftFinding {
                signal: DriftSignal::ChangeVolume,
                severity,
                description: format!(
                    "Avg artifact count changed from {:.1} to {:.1} ({:.1}x)",
                    baseline.avg_artifact_count, recent_avg, ratio,
                ),
                baseline_value: Some(baseline.avg_artifact_count),
                current_value: Some(recent_avg),
            });
        }
    }

    // 5. Dependency pattern shift.
    if total_recent_drafts > 0 {
        let recent_dep_count: usize = agent_drafts
            .iter()
            .map(|d| d.dependency_artifact_count)
            .sum();
        // If any recent drafts touch dependency files and baseline had none, that's notable.
        if recent_dep_count > 0 {
            let dep_per_draft = recent_dep_count as f64 / total_recent_drafts as f64;
            let severity = if dep_per_draft > 2.0 {
                DriftSeverity::Alert
            } else if dep_per_draft > 0.5 {
                DriftSeverity::Warning
            } else {
                DriftSeverity::Normal
            };
            if severity > DriftSeverity::Normal {
                findings.push(DriftFinding {
                    signal: DriftSignal::DependencyPattern,
                    severity,
                    description: format!(
                        "{} dependency file change(s) across {} recent draft(s) ({:.1}/draft)",
                        recent_dep_count, total_recent_drafts, dep_per_draft,
                    ),
                    baseline_value: None,
                    current_value: Some(dep_per_draft),
                });
            }
        }
    }

    // Overall severity is the worst across all findings.
    let overall_severity = findings
        .iter()
        .map(|f| f.severity)
        .max()
        .unwrap_or(DriftSeverity::Normal);

    DriftReport {
        agent_id: agent_id.to_string(),
        computed_at: Utc::now(),
        window_size,
        findings,
        overall_severity,
    }
}

/// Classify a rate delta into a severity level.
fn rate_severity(delta: f64) -> DriftSeverity {
    if delta >= ALERT_RATE_DELTA {
        DriftSeverity::Alert
    } else if delta >= WARNING_RATE_DELTA {
        DriftSeverity::Warning
    } else {
        DriftSeverity::Normal
    }
}

/// Extract unique agent IDs from a set of audit events.
pub fn unique_agent_ids(events: &[AuditEvent]) -> Vec<String> {
    let mut ids: Vec<String> = events
        .iter()
        .map(|e| e.agent_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    ids.sort();
    ids
}

/// Create a drift finding from access constitution violations (v0.4.3).
///
/// Constitution violations are always at least Warning severity, and become
/// Alert when more than half the artifacts are undeclared.
pub fn constitution_violation_finding(
    undeclared_uris: &[String],
    total_artifacts: usize,
) -> Option<DriftFinding> {
    if undeclared_uris.is_empty() {
        return None;
    }

    let fraction = undeclared_uris.len() as f64 / total_artifacts.max(1) as f64;
    let severity = if fraction > 0.5 {
        DriftSeverity::Alert
    } else {
        DriftSeverity::Warning
    };

    let examples: Vec<&str> = undeclared_uris.iter().take(3).map(|s| s.as_str()).collect();
    Some(DriftFinding {
        signal: DriftSignal::ConstitutionViolation,
        severity,
        description: format!(
            "{} artifact(s) accessed outside declared constitution (e.g., {})",
            undeclared_uris.len(),
            examples.join(", "),
        ),
        baseline_value: Some(0.0),
        current_value: Some(undeclared_uris.len() as f64),
    })
}

/// Check whether a URI looks like a dependency manifest file.
pub fn is_dependency_file(uri: &str) -> bool {
    let lower = uri.to_lowercase();
    lower.ends_with("/cargo.toml")
        || lower.ends_with("/package.json")
        || lower.ends_with("/go.mod")
        || lower.ends_with("/requirements.txt")
        || lower.ends_with("/pyproject.toml")
        || lower.ends_with("/cargo.lock")
        || lower.ends_with("/package-lock.json")
        || lower.ends_with("/yarn.lock")
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::AuditAction;

    fn make_event(agent_id: &str, action: AuditAction, target: Option<&str>) -> AuditEvent {
        let mut event = AuditEvent::new(agent_id, action);
        if let Some(uri) = target {
            event = event.with_target(uri);
        }
        event
    }

    fn make_draft(
        agent_id: &str,
        artifacts: usize,
        risk: u32,
        rejected: bool,
        dep_count: usize,
    ) -> DraftSummary {
        DraftSummary {
            agent_id: agent_id.to_string(),
            artifact_count: artifacts,
            risk_score: risk,
            rejected,
            dependency_artifact_count: dep_count,
        }
    }

    // ── BehavioralBaseline tests ──

    #[test]
    fn baseline_serialization_round_trip() {
        let baseline = BehavioralBaseline {
            agent_id: "agent-1".to_string(),
            computed_at: Utc::now(),
            goal_count: 5,
            resource_patterns: vec![
                "fs://workspace/src/".to_string(),
                "fs://workspace/tests/".to_string(),
            ],
            avg_artifact_count: 3.5,
            avg_risk_score: 12.0,
            escalation_rate: 0.1,
            rejection_rate: 0.05,
        };

        let json = serde_json::to_string_pretty(&baseline).unwrap();
        let restored: BehavioralBaseline = serde_json::from_str(&json).unwrap();

        assert_eq!(baseline.agent_id, restored.agent_id);
        assert_eq!(baseline.goal_count, restored.goal_count);
        assert_eq!(baseline.resource_patterns, restored.resource_patterns);
        assert!((baseline.avg_artifact_count - restored.avg_artifact_count).abs() < f64::EPSILON);
        assert!((baseline.escalation_rate - restored.escalation_rate).abs() < f64::EPSILON);
        assert!((baseline.rejection_rate - restored.rejection_rate).abs() < f64::EPSILON);
    }

    #[test]
    fn compute_baseline_empty_inputs() {
        let baseline = compute_baseline("agent-1", &[], &[]);
        assert_eq!(baseline.agent_id, "agent-1");
        assert!(baseline.resource_patterns.is_empty());
        assert_eq!(baseline.avg_artifact_count, 0.0);
        assert_eq!(baseline.avg_risk_score, 0.0);
        assert_eq!(baseline.escalation_rate, 0.0);
        assert_eq!(baseline.rejection_rate, 0.0);
    }

    #[test]
    fn compute_baseline_escalation_rate() {
        let events = vec![
            make_event(
                "agent-1",
                AuditAction::ToolCall,
                Some("fs://workspace/src/main.rs"),
            ),
            make_event(
                "agent-1",
                AuditAction::PolicyDecision,
                Some("fs://workspace/src/main.rs"),
            ),
            make_event(
                "agent-1",
                AuditAction::ToolCall,
                Some("fs://workspace/src/lib.rs"),
            ),
            make_event(
                "agent-1",
                AuditAction::ToolCall,
                Some("fs://workspace/tests/test.rs"),
            ),
            // Different agent — should be filtered out.
            make_event(
                "agent-2",
                AuditAction::PolicyDecision,
                Some("fs://workspace/ci/build.sh"),
            ),
        ];
        let baseline = compute_baseline("agent-1", &events, &[]);
        // 1 PolicyDecision out of 4 agent-1 events = 0.25.
        assert!((baseline.escalation_rate - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn compute_baseline_draft_metrics() {
        let drafts = vec![
            make_draft("agent-1", 5, 10, false, 0),
            make_draft("agent-1", 3, 20, true, 1),
            make_draft("agent-1", 4, 15, false, 0),
            // Different agent.
            make_draft("agent-2", 10, 50, true, 2),
        ];
        let baseline = compute_baseline("agent-1", &[], &drafts);
        assert!((baseline.avg_artifact_count - 4.0).abs() < f64::EPSILON);
        assert!((baseline.avg_risk_score - 15.0).abs() < f64::EPSILON);
        // 1 rejected out of 3.
        assert!((baseline.rejection_rate - 1.0 / 3.0).abs() < 0.001);
        assert_eq!(baseline.goal_count, 3);
    }

    #[test]
    fn compute_baseline_resource_patterns() {
        let events = vec![
            make_event(
                "agent-1",
                AuditAction::ToolCall,
                Some("fs://workspace/src/main.rs"),
            ),
            make_event(
                "agent-1",
                AuditAction::ToolCall,
                Some("fs://workspace/src/lib.rs"),
            ),
            make_event(
                "agent-1",
                AuditAction::ToolCall,
                Some("fs://workspace/tests/test.rs"),
            ),
        ];
        let baseline = compute_baseline("agent-1", &events, &[]);
        assert!(baseline
            .resource_patterns
            .contains(&"fs://workspace/src/".to_string()));
        assert!(baseline
            .resource_patterns
            .contains(&"fs://workspace/tests/".to_string()));
    }

    // ── BaselineStore tests ──

    #[test]
    fn baseline_store_save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = BaselineStore::new(dir.path().to_path_buf());

        let baseline = BehavioralBaseline {
            agent_id: "agent-1".to_string(),
            computed_at: Utc::now(),
            goal_count: 3,
            resource_patterns: vec!["fs://workspace/src/".to_string()],
            avg_artifact_count: 4.0,
            avg_risk_score: 12.0,
            escalation_rate: 0.25,
            rejection_rate: 0.1,
        };

        store.save(&baseline).unwrap();
        let loaded = store.load("agent-1").unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.agent_id, "agent-1");
        assert_eq!(loaded.goal_count, 3);
        assert!((loaded.escalation_rate - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn baseline_store_load_returns_none_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let store = BaselineStore::new(dir.path().to_path_buf());
        let result = store.load("nonexistent-agent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn baseline_store_list_agents() {
        let dir = tempfile::tempdir().unwrap();
        let store = BaselineStore::new(dir.path().to_path_buf());

        let b1 = BehavioralBaseline {
            agent_id: "alpha".to_string(),
            computed_at: Utc::now(),
            goal_count: 1,
            resource_patterns: vec![],
            avg_artifact_count: 0.0,
            avg_risk_score: 0.0,
            escalation_rate: 0.0,
            rejection_rate: 0.0,
        };
        let mut b2 = b1.clone();
        b2.agent_id = "beta".to_string();

        store.save(&b1).unwrap();
        store.save(&b2).unwrap();

        let agents = store.list_agents().unwrap();
        assert_eq!(agents, vec!["alpha", "beta"]);
    }

    // ── DriftReport tests ──

    #[test]
    fn drift_report_serialization_round_trip() {
        let report = DriftReport {
            agent_id: "agent-1".to_string(),
            computed_at: Utc::now(),
            window_size: 5,
            findings: vec![DriftFinding {
                signal: DriftSignal::EscalationFrequency,
                severity: DriftSeverity::Warning,
                description: "Escalation rate changed".to_string(),
                baseline_value: Some(0.1),
                current_value: Some(0.4),
            }],
            overall_severity: DriftSeverity::Warning,
        };

        let json = serde_json::to_string_pretty(&report).unwrap();
        let restored: DriftReport = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.agent_id, "agent-1");
        assert_eq!(restored.findings.len(), 1);
        assert_eq!(restored.overall_severity, DriftSeverity::Warning);
    }

    #[test]
    fn compute_drift_no_deviation() {
        let baseline = BehavioralBaseline {
            agent_id: "agent-1".to_string(),
            computed_at: Utc::now(),
            goal_count: 5,
            resource_patterns: vec!["fs://workspace/src/".to_string()],
            avg_artifact_count: 4.0,
            avg_risk_score: 10.0,
            escalation_rate: 0.25,
            rejection_rate: 0.1,
        };

        // Recent behavior matches baseline.
        let events = vec![
            make_event(
                "agent-1",
                AuditAction::ToolCall,
                Some("fs://workspace/src/main.rs"),
            ),
            make_event(
                "agent-1",
                AuditAction::ToolCall,
                Some("fs://workspace/src/lib.rs"),
            ),
            make_event(
                "agent-1",
                AuditAction::ToolCall,
                Some("fs://workspace/src/util.rs"),
            ),
            make_event(
                "agent-1",
                AuditAction::PolicyDecision,
                Some("fs://workspace/src/main.rs"),
            ),
        ];
        let drafts = vec![make_draft("agent-1", 4, 10, false, 0)];

        let report = compute_drift(&baseline, &events, &drafts, 5);
        assert_eq!(report.overall_severity, DriftSeverity::Normal);
        // No significant findings expected (escalation rate 0.25 matches baseline).
        assert!(
            report
                .findings
                .iter()
                .all(|f| f.severity == DriftSeverity::Normal),
            "Expected no warning/alert findings, got: {:?}",
            report.findings,
        );
    }

    #[test]
    fn compute_drift_escalation_spike() {
        let baseline = BehavioralBaseline {
            agent_id: "agent-1".to_string(),
            computed_at: Utc::now(),
            goal_count: 5,
            resource_patterns: vec!["fs://workspace/src/".to_string()],
            avg_artifact_count: 4.0,
            avg_risk_score: 10.0,
            escalation_rate: 0.1, // 10% baseline
            rejection_rate: 0.0,
        };

        // Recent: 80% escalation rate (4/5 events are PolicyDecision).
        let events = vec![
            make_event(
                "agent-1",
                AuditAction::PolicyDecision,
                Some("fs://workspace/src/a.rs"),
            ),
            make_event(
                "agent-1",
                AuditAction::PolicyDecision,
                Some("fs://workspace/src/b.rs"),
            ),
            make_event(
                "agent-1",
                AuditAction::PolicyDecision,
                Some("fs://workspace/src/c.rs"),
            ),
            make_event(
                "agent-1",
                AuditAction::PolicyDecision,
                Some("fs://workspace/src/d.rs"),
            ),
            make_event(
                "agent-1",
                AuditAction::ToolCall,
                Some("fs://workspace/src/e.rs"),
            ),
        ];
        let drafts = vec![make_draft("agent-1", 4, 10, false, 0)];

        let report = compute_drift(&baseline, &events, &drafts, 5);
        let esc_finding = report
            .findings
            .iter()
            .find(|f| f.signal == DriftSignal::EscalationFrequency);
        assert!(esc_finding.is_some(), "Expected escalation drift finding");
        assert!(esc_finding.unwrap().severity >= DriftSeverity::Alert);
    }

    #[test]
    fn compute_drift_novel_uris() {
        let baseline = BehavioralBaseline {
            agent_id: "agent-1".to_string(),
            computed_at: Utc::now(),
            goal_count: 5,
            resource_patterns: vec!["fs://workspace/src/".to_string()],
            avg_artifact_count: 4.0,
            avg_risk_score: 10.0,
            escalation_rate: 0.0,
            rejection_rate: 0.0,
        };

        // Agent now accessing CI configs — not in baseline patterns.
        let events = vec![
            make_event(
                "agent-1",
                AuditAction::ToolCall,
                Some("fs://workspace/.github/workflows/ci.yml"),
            ),
            make_event(
                "agent-1",
                AuditAction::ToolCall,
                Some("fs://workspace/.github/workflows/release.yml"),
            ),
            make_event(
                "agent-1",
                AuditAction::ToolCall,
                Some("fs://workspace/src/main.rs"),
            ),
        ];
        let drafts = vec![make_draft("agent-1", 3, 10, false, 0)];

        let report = compute_drift(&baseline, &events, &drafts, 5);
        let scope_finding = report
            .findings
            .iter()
            .find(|f| f.signal == DriftSignal::ResourceScope);
        assert!(
            scope_finding.is_some(),
            "Expected resource scope drift finding"
        );
        // 2/3 URIs are novel = 66% → should be Alert.
        assert!(scope_finding.unwrap().severity >= DriftSeverity::Alert);
    }

    #[test]
    fn compute_drift_rejection_rate_jump() {
        let baseline = BehavioralBaseline {
            agent_id: "agent-1".to_string(),
            computed_at: Utc::now(),
            goal_count: 10,
            resource_patterns: vec!["fs://workspace/src/".to_string()],
            avg_artifact_count: 4.0,
            avg_risk_score: 10.0,
            escalation_rate: 0.0,
            rejection_rate: 0.05, // 5% historical
        };

        // Recent: 60% rejection rate.
        let drafts = vec![
            make_draft("agent-1", 4, 10, true, 0),
            make_draft("agent-1", 4, 10, true, 0),
            make_draft("agent-1", 4, 10, true, 0),
            make_draft("agent-1", 4, 10, false, 0),
            make_draft("agent-1", 4, 10, false, 0),
        ];

        let report = compute_drift(&baseline, &[], &drafts, 5);
        let rej_finding = report
            .findings
            .iter()
            .find(|f| f.signal == DriftSignal::RejectionRate);
        assert!(
            rej_finding.is_some(),
            "Expected rejection rate drift finding"
        );
        assert!(rej_finding.unwrap().severity >= DriftSeverity::Alert);
    }

    #[test]
    fn compute_drift_volume_anomaly() {
        let baseline = BehavioralBaseline {
            agent_id: "agent-1".to_string(),
            computed_at: Utc::now(),
            goal_count: 5,
            resource_patterns: vec!["fs://workspace/src/".to_string()],
            avg_artifact_count: 4.0,
            avg_risk_score: 10.0,
            escalation_rate: 0.0,
            rejection_rate: 0.0,
        };

        // Recent: 12 artifacts per draft (3x baseline of 4).
        let drafts = vec![make_draft("agent-1", 12, 10, false, 0)];

        let report = compute_drift(&baseline, &[], &drafts, 5);
        let vol_finding = report
            .findings
            .iter()
            .find(|f| f.signal == DriftSignal::ChangeVolume);
        assert!(
            vol_finding.is_some(),
            "Expected change volume drift finding"
        );
        assert!(vol_finding.unwrap().severity >= DriftSeverity::Alert);
    }

    #[test]
    fn compute_drift_dependency_shift() {
        let baseline = BehavioralBaseline {
            agent_id: "agent-1".to_string(),
            computed_at: Utc::now(),
            goal_count: 5,
            resource_patterns: vec!["fs://workspace/src/".to_string()],
            avg_artifact_count: 4.0,
            avg_risk_score: 10.0,
            escalation_rate: 0.0,
            rejection_rate: 0.0,
        };

        // Recent: touching dependency files.
        let drafts = vec![
            make_draft("agent-1", 5, 10, false, 3),
            make_draft("agent-1", 4, 10, false, 2),
        ];

        let report = compute_drift(&baseline, &[], &drafts, 5);
        let dep_finding = report
            .findings
            .iter()
            .find(|f| f.signal == DriftSignal::DependencyPattern);
        assert!(
            dep_finding.is_some(),
            "Expected dependency pattern drift finding"
        );
        assert!(dep_finding.unwrap().severity >= DriftSeverity::Warning);
    }

    // ── Utility tests ──

    #[test]
    fn uri_prefix_extraction() {
        assert_eq!(
            uri_prefix("fs://workspace/src/main.rs"),
            "fs://workspace/src/"
        );
        assert_eq!(uri_prefix("fs://workspace/Cargo.toml"), "fs://workspace/");
        assert_eq!(uri_prefix("just-a-name"), "just-a-name");
    }

    #[test]
    fn is_dependency_file_detection() {
        assert!(is_dependency_file("fs://workspace/Cargo.toml"));
        assert!(is_dependency_file("fs://workspace/crates/foo/Cargo.toml"));
        assert!(is_dependency_file("fs://workspace/package.json"));
        assert!(is_dependency_file("fs://workspace/go.mod"));
        assert!(is_dependency_file("fs://workspace/requirements.txt"));
        assert!(!is_dependency_file("fs://workspace/src/main.rs"));
        assert!(!is_dependency_file("fs://workspace/README.md"));
    }

    #[test]
    fn unique_agent_ids_extraction() {
        let events = vec![
            make_event("agent-b", AuditAction::ToolCall, None),
            make_event("agent-a", AuditAction::ToolCall, None),
            make_event("agent-b", AuditAction::PolicyDecision, None),
            make_event("agent-c", AuditAction::Approval, None),
        ];
        let ids = unique_agent_ids(&events);
        assert_eq!(ids, vec!["agent-a", "agent-b", "agent-c"]);
    }

    // ── Constitution violation drift (v0.4.3) ──

    #[test]
    fn constitution_violation_finding_none_when_empty() {
        let result = constitution_violation_finding(&[], 5);
        assert!(result.is_none());
    }

    #[test]
    fn constitution_violation_finding_warning_for_few() {
        let undeclared = vec!["fs://workspace/extra.rs".to_string()];
        let finding = constitution_violation_finding(&undeclared, 5).unwrap();
        assert_eq!(finding.signal, DriftSignal::ConstitutionViolation);
        assert_eq!(finding.severity, DriftSeverity::Warning);
        assert!(finding.description.contains("1 artifact(s)"));
    }

    #[test]
    fn constitution_violation_finding_alert_for_majority() {
        let undeclared = vec![
            "fs://workspace/a.rs".to_string(),
            "fs://workspace/b.rs".to_string(),
            "fs://workspace/c.rs".to_string(),
        ];
        let finding = constitution_violation_finding(&undeclared, 4).unwrap();
        assert_eq!(finding.signal, DriftSignal::ConstitutionViolation);
        assert_eq!(finding.severity, DriftSeverity::Alert);
    }

    #[test]
    fn constitution_violation_signal_serialization() {
        let signal = DriftSignal::ConstitutionViolation;
        let json = serde_json::to_string(&signal).unwrap();
        assert_eq!(json, "\"constitution_violation\"");
        let restored: DriftSignal = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, DriftSignal::ConstitutionViolation);
    }
}
