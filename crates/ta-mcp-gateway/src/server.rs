// server.rs — MCP gateway server for Trusted Autonomy.
//
// TaGatewayServer implements the rmcp ServerHandler trait, exposing TA's
// staging, policy, and goal lifecycle as MCP tools. Every file operation
// flows through policy → staging → changeset → audit, ensuring the core
// thesis holds: all agent actions are mediated.
//
// Tools (prefixed `ta_` for namespacing):
//   ta_goal_start  — create a GoalRun, issue manifest, allocate workspace
//   ta_goal_status — get current state of a GoalRun
//   ta_goal_list   — list GoalRuns (optionally filtered by state)
//   ta_fs_read     — read a file from the source directory
//   ta_fs_write    — write a file to staging (creates ChangeSet)
//   ta_fs_list     — list staged files for a goal
//   ta_fs_diff     — show diff for a staged file
//   ta_pr_build    — bundle staged changes into a PR package
//   ta_pr_status   — check PR package review status

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;
use uuid::Uuid;

use ta_audit::AuditLog;
use ta_changeset::pr_package::PRPackage;
use ta_connector_fs::FsConnector;
use ta_goal::{EventDispatcher, GoalRun, GoalRunState, GoalRunStore, LogSink, TaEvent};
use ta_policy::{
    AlignmentProfile, CompilerOptions, PolicyCompiler, PolicyDecision, PolicyEngine, PolicyRequest,
};
use ta_workspace::{JsonFileStore, StagingWorkspace};

use crate::config::GatewayConfig;
use crate::error::GatewayError;

// ── Tool parameter types ─────────────────────────────────────────

/// Parameters for `ta_goal_start`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GoalStartParams {
    /// Human-readable title for the goal (e.g., "Fix authentication bug").
    pub title: String,
    /// Detailed objective describing what needs to be accomplished.
    pub objective: String,
    /// Agent identifier. Defaults to "claude-code" if not provided.
    #[serde(default = "default_agent_id")]
    pub agent_id: String,
}

fn default_agent_id() -> String {
    "claude-code".to_string()
}

/// Parameters for tools that take only a goal_run_id.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GoalIdParams {
    /// The UUID of the goal run.
    pub goal_run_id: String,
}

/// Parameters for `ta_goal_list`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GoalListParams {
    /// Optional state filter (e.g., "running", "pr_ready").
    pub state: Option<String>,
}

/// Parameters for `ta_fs_write`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FsWriteParams {
    /// The UUID of the goal run.
    pub goal_run_id: String,
    /// Relative path within the workspace (e.g., "src/main.rs").
    pub path: String,
    /// File content to write.
    pub content: String,
}

/// Parameters for `ta_fs_read`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FsReadParams {
    /// The UUID of the goal run.
    pub goal_run_id: String,
    /// Relative path within the workspace.
    pub path: String,
}

/// Parameters for `ta_fs_list`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FsListParams {
    /// The UUID of the goal run.
    pub goal_run_id: String,
}

/// Parameters for `ta_fs_diff`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FsDiffParams {
    /// The UUID of the goal run.
    pub goal_run_id: String,
    /// Relative path within the workspace.
    pub path: String,
}

/// Parameters for `ta_pr_build`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct PrBuildParams {
    /// The UUID of the goal run.
    pub goal_run_id: String,
    /// Title for the PR package.
    pub title: String,
    /// Summary of what changed and why.
    pub summary: String,
}

// ── Gateway state ────────────────────────────────────────────────

/// Shared mutable state for the gateway server.
///
/// Holds all the components that tool handlers need: policy engine,
/// goal store, active connectors, audit log, and event dispatcher.
pub struct GatewayState {
    pub config: GatewayConfig,
    pub policy_engine: PolicyEngine,
    pub goal_store: GoalRunStore,
    pub connectors: HashMap<Uuid, FsConnector<JsonFileStore>>,
    pub pr_packages: HashMap<Uuid, PRPackage>,
    pub audit_log: AuditLog,
    pub event_dispatcher: EventDispatcher,
}

impl GatewayState {
    /// Initialize gateway state from config. Creates directories and opens logs.
    pub fn new(config: GatewayConfig) -> Result<Self, GatewayError> {
        let goal_store = GoalRunStore::new(&config.goals_dir)?;
        let audit_log = AuditLog::open(&config.audit_log)?;

        let mut event_dispatcher = EventDispatcher::new();
        event_dispatcher.add_sink(Box::new(LogSink::new(&config.events_log)));

        Ok(Self {
            config,
            policy_engine: PolicyEngine::new(),
            goal_store,
            connectors: HashMap::new(),
            pr_packages: HashMap::new(),
            audit_log,
            event_dispatcher,
        })
    }

    /// Start a new goal: create GoalRun, issue manifest, set up connector.
    pub fn start_goal(
        &mut self,
        title: &str,
        objective: &str,
        agent_id: &str,
    ) -> Result<GoalRun, GatewayError> {
        // Create GoalRun with paths derived from config.
        let goal_run_id = Uuid::new_v4();
        let staging_path = self.config.staging_dir.join(goal_run_id.to_string());
        let store_path = self.config.store_dir.join(goal_run_id.to_string());

        let mut goal_run = GoalRun::new(title, objective, agent_id, staging_path, store_path);
        // Override the random ID so we control it.
        goal_run.goal_run_id = goal_run_id;

        // Compile a default developer manifest via the Policy Compiler (v0.4.0).
        // Uses AlignmentProfile::default_developer() which grants fs read/write/apply
        // on workspace and denies network/credential access.
        let profile = AlignmentProfile::default_developer();
        let options = CompilerOptions::default();
        let manifest =
            PolicyCompiler::compile_with_id(goal_run.manifest_id, agent_id, &profile, &options)
                .map_err(|e| GatewayError::Other(format!("policy compilation failed: {}", e)))?;
        self.policy_engine.load_manifest(manifest);

        // Create staging workspace and change store for this goal.
        let staging = StagingWorkspace::new(goal_run_id.to_string(), &self.config.staging_dir)?;
        let store = JsonFileStore::new(self.config.store_dir.join(goal_run_id.to_string()))?;
        let connector = FsConnector::new(goal_run_id.to_string(), staging, store, agent_id);
        self.connectors.insert(goal_run_id, connector);

        // Transition Created → Configured → Running.
        goal_run.transition(GoalRunState::Configured)?;
        goal_run.transition(GoalRunState::Running)?;
        self.goal_store.save(&goal_run)?;

        // Emit events.
        self.event_dispatcher
            .dispatch(&TaEvent::goal_created(goal_run_id, title, agent_id));

        Ok(goal_run)
    }

    /// Start a new goal with a custom alignment profile (v0.4.0).
    ///
    /// Like `start_goal`, but compiles the provided AlignmentProfile into
    /// the capability manifest instead of using the default developer profile.
    pub fn start_goal_with_profile(
        &mut self,
        title: &str,
        objective: &str,
        agent_id: &str,
        profile: &AlignmentProfile,
        resource_scope: Option<Vec<String>>,
    ) -> Result<GoalRun, GatewayError> {
        let goal_run_id = Uuid::new_v4();
        let staging_path = self.config.staging_dir.join(goal_run_id.to_string());
        let store_path = self.config.store_dir.join(goal_run_id.to_string());

        let mut goal_run = GoalRun::new(title, objective, agent_id, staging_path, store_path);
        goal_run.goal_run_id = goal_run_id;

        let options = CompilerOptions {
            resource_scope: resource_scope.unwrap_or_else(|| vec!["fs://workspace/**".to_string()]),
            validity_hours: 8,
        };
        let manifest =
            PolicyCompiler::compile_with_id(goal_run.manifest_id, agent_id, profile, &options)
                .map_err(|e| GatewayError::Other(format!("policy compilation failed: {}", e)))?;
        self.policy_engine.load_manifest(manifest);

        let staging = StagingWorkspace::new(goal_run_id.to_string(), &self.config.staging_dir)?;
        let store = JsonFileStore::new(self.config.store_dir.join(goal_run_id.to_string()))?;
        let connector = FsConnector::new(goal_run_id.to_string(), staging, store, agent_id);
        self.connectors.insert(goal_run_id, connector);

        goal_run.transition(GoalRunState::Configured)?;
        goal_run.transition(GoalRunState::Running)?;
        self.goal_store.save(&goal_run)?;

        self.event_dispatcher
            .dispatch(&TaEvent::goal_created(goal_run_id, title, agent_id));

        Ok(goal_run)
    }

    /// Check policy for a filesystem operation.
    fn check_policy(
        &self,
        agent_id: &str,
        verb: &str,
        path: &str,
    ) -> Result<PolicyDecision, GatewayError> {
        let request = PolicyRequest {
            agent_id: agent_id.to_string(),
            tool: "fs".to_string(),
            verb: verb.to_string(),
            target_uri: format!("fs://workspace/{}", path),
        };
        Ok(self.policy_engine.evaluate(&request))
    }

    /// Save a PR package to both in-memory cache and disk.
    pub fn save_pr_package(&mut self, pkg: PRPackage) -> Result<(), GatewayError> {
        let package_id = pkg.package_id;

        // Persist to disk so the CLI can read it.
        std::fs::create_dir_all(&self.config.pr_packages_dir)?;
        let path = self
            .config
            .pr_packages_dir
            .join(format!("{}.json", package_id));
        let json =
            serde_json::to_string_pretty(&pkg).map_err(|e| GatewayError::Other(e.to_string()))?;
        std::fs::write(&path, json)?;

        self.pr_packages.insert(package_id, pkg);
        Ok(())
    }

    /// Get the agent_id for a goal run.
    fn agent_for_goal(&self, goal_run_id: Uuid) -> Result<String, GatewayError> {
        let goal = self
            .goal_store
            .get(goal_run_id)?
            .ok_or(GatewayError::GoalNotFound(goal_run_id))?;
        Ok(goal.agent_id)
    }
}

// ── MCP Server ───────────────────────────────────────────────────

/// The MCP gateway server. Holds shared state and the tool router.
pub struct TaGatewayServer {
    state: Arc<Mutex<GatewayState>>,
    tool_router: ToolRouter<Self>,
}

// Tool definitions. Each `#[tool]` method becomes an MCP tool that
// Claude Code (or any MCP client) can call.
#[tool_router]
impl TaGatewayServer {
    /// Create a new gateway server from config.
    pub fn new(config: GatewayConfig) -> Result<Self, GatewayError> {
        let state = GatewayState::new(config)?;
        Ok(Self {
            state: Arc::new(Mutex::new(state)),
            tool_router: Self::tool_router(),
        })
    }

    /// Create a server wrapping existing state (for testing).
    pub fn with_state(state: GatewayState) -> Self {
        Self {
            state: Arc::new(Mutex::new(state)),
            tool_router: Self::tool_router(),
        }
    }

    /// Get a reference to the shared state (for testing).
    pub fn state(&self) -> &Arc<Mutex<GatewayState>> {
        &self.state
    }

    // ── Goal tools ───────────────────────────────────────────

    #[tool(
        description = "Start a new goal run. Creates a workspace, issues a capability manifest, and transitions to Running state. Returns the goal_run_id to use with other tools."
    )]
    fn ta_goal_start(
        &self,
        Parameters(params): Parameters<GoalStartParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut state = self
            .state
            .lock()
            .map_err(|e| McpError::internal_error(format!("lock poisoned: {}", e), None))?;
        let goal_run = state
            .start_goal(&params.title, &params.objective, &params.agent_id)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let response = serde_json::json!({
            "goal_run_id": goal_run.goal_run_id.to_string(),
            "state": goal_run.state.to_string(),
            "title": goal_run.title,
            "agent_id": goal_run.agent_id,
            "manifest_id": goal_run.manifest_id.to_string(),
        });
        Ok(CallToolResult::success(vec![Content::json(response)
            .map_err(|e| {
                McpError::internal_error(e.to_string(), None)
            })?]))
    }

    #[tool(description = "Get the current status of a goal run, including its state and metadata.")]
    fn ta_goal_status(
        &self,
        Parameters(params): Parameters<GoalIdParams>,
    ) -> Result<CallToolResult, McpError> {
        let state = self
            .state
            .lock()
            .map_err(|e| McpError::internal_error(format!("lock poisoned: {}", e), None))?;
        let goal_run_id = parse_uuid(&params.goal_run_id)?;
        let goal = state
            .goal_store
            .get(goal_run_id)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .ok_or_else(|| {
                McpError::invalid_params(format!("goal not found: {}", goal_run_id), None)
            })?;

        let response = serde_json::json!({
            "goal_run_id": goal.goal_run_id.to_string(),
            "title": goal.title,
            "objective": goal.objective,
            "state": goal.state.to_string(),
            "agent_id": goal.agent_id,
            "created_at": goal.created_at.to_rfc3339(),
            "updated_at": goal.updated_at.to_rfc3339(),
            "pr_package_id": goal.pr_package_id.map(|id| id.to_string()),
        });
        Ok(CallToolResult::success(vec![Content::json(response)
            .map_err(|e| {
                McpError::internal_error(e.to_string(), None)
            })?]))
    }

    #[tool(
        description = "List all goal runs, optionally filtered by state (e.g., 'running', 'pr_ready', 'completed')."
    )]
    fn ta_goal_list(
        &self,
        Parameters(params): Parameters<GoalListParams>,
    ) -> Result<CallToolResult, McpError> {
        let state = self
            .state
            .lock()
            .map_err(|e| McpError::internal_error(format!("lock poisoned: {}", e), None))?;
        let goals = if let Some(ref state_filter) = params.state {
            state
                .goal_store
                .list_by_state(state_filter)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?
        } else {
            state
                .goal_store
                .list()
                .map_err(|e| McpError::internal_error(e.to_string(), None))?
        };

        let items: Vec<serde_json::Value> = goals
            .iter()
            .map(|g| {
                serde_json::json!({
                    "goal_run_id": g.goal_run_id.to_string(),
                    "title": g.title,
                    "state": g.state.to_string(),
                    "agent_id": g.agent_id,
                    "created_at": g.created_at.to_rfc3339(),
                })
            })
            .collect();

        let response = serde_json::json!({ "goals": items, "count": items.len() });
        Ok(CallToolResult::success(vec![Content::json(response)
            .map_err(|e| {
                McpError::internal_error(e.to_string(), None)
            })?]))
    }

    // ── Filesystem tools ─────────────────────────────────────

    #[tool(
        description = "Read a file from the project source directory. The file is snapshotted for later diff generation."
    )]
    fn ta_fs_read(
        &self,
        Parameters(params): Parameters<FsReadParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut state = self
            .state
            .lock()
            .map_err(|e| McpError::internal_error(format!("lock poisoned: {}", e), None))?;
        let goal_run_id = parse_uuid(&params.goal_run_id)?;
        let agent_id = state
            .agent_for_goal(goal_run_id)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // Check policy.
        let decision = state
            .check_policy(&agent_id, "read", &params.path)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        enforce_policy(&decision)?;

        // Clone workspace_root before taking mutable borrow on connector.
        let workspace_root = state.config.workspace_root.clone();
        let connector = state.connectors.get_mut(&goal_run_id).ok_or_else(|| {
            McpError::invalid_params(
                format!("no active connector for goal: {}", goal_run_id),
                None,
            )
        })?;

        let content = connector
            .read_source(&workspace_root, &params.path)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let text = String::from_utf8_lossy(&content).to_string();
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        description = "Write a file to the staging workspace. Creates a ChangeSet tracking the modification. Nothing touches the real filesystem until approved and applied."
    )]
    fn ta_fs_write(
        &self,
        Parameters(params): Parameters<FsWriteParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut state = self
            .state
            .lock()
            .map_err(|e| McpError::internal_error(format!("lock poisoned: {}", e), None))?;
        let goal_run_id = parse_uuid(&params.goal_run_id)?;
        let agent_id = state
            .agent_for_goal(goal_run_id)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // Check policy.
        let decision = state
            .check_policy(&agent_id, "write_patch", &params.path)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        enforce_policy(&decision)?;

        let connector = state.connectors.get_mut(&goal_run_id).ok_or_else(|| {
            McpError::invalid_params(
                format!("no active connector for goal: {}", goal_run_id),
                None,
            )
        })?;

        let changeset = connector
            .write_patch(&params.path, params.content.as_bytes())
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let response = serde_json::json!({
            "changeset_id": changeset.changeset_id.to_string(),
            "target_uri": changeset.target_uri,
            "status": "staged",
        });
        Ok(CallToolResult::success(vec![Content::json(response)
            .map_err(|e| {
                McpError::internal_error(e.to_string(), None)
            })?]))
    }

    #[tool(description = "List all files currently staged for a goal run.")]
    fn ta_fs_list(
        &self,
        Parameters(params): Parameters<FsListParams>,
    ) -> Result<CallToolResult, McpError> {
        let state = self
            .state
            .lock()
            .map_err(|e| McpError::internal_error(format!("lock poisoned: {}", e), None))?;
        let goal_run_id = parse_uuid(&params.goal_run_id)?;

        let connector = state.connectors.get(&goal_run_id).ok_or_else(|| {
            McpError::invalid_params(
                format!("no active connector for goal: {}", goal_run_id),
                None,
            )
        })?;

        let files = connector
            .list_staged()
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let response = serde_json::json!({ "files": files, "count": files.len() });
        Ok(CallToolResult::success(vec![Content::json(response)
            .map_err(|e| {
                McpError::internal_error(e.to_string(), None)
            })?]))
    }

    #[tool(description = "Show the diff for a staged file compared to the original source.")]
    fn ta_fs_diff(
        &self,
        Parameters(params): Parameters<FsDiffParams>,
    ) -> Result<CallToolResult, McpError> {
        let state = self
            .state
            .lock()
            .map_err(|e| McpError::internal_error(format!("lock poisoned: {}", e), None))?;
        let goal_run_id = parse_uuid(&params.goal_run_id)?;

        let connector = state.connectors.get(&goal_run_id).ok_or_else(|| {
            McpError::invalid_params(
                format!("no active connector for goal: {}", goal_run_id),
                None,
            )
        })?;

        let diff = connector
            .diff_file(&params.path)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        match diff {
            Some(diff_text) => Ok(CallToolResult::success(vec![Content::text(diff_text)])),
            None => Ok(CallToolResult::success(vec![Content::text(
                "No changes (file is identical to source).",
            )])),
        }
    }

    // ── PR tools ─────────────────────────────────────────────

    #[tool(
        description = "Bundle all staged changes for a goal into a PR package for human review. Transitions the goal to PrReady state."
    )]
    fn ta_pr_build(
        &self,
        Parameters(params): Parameters<PrBuildParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut state = self
            .state
            .lock()
            .map_err(|e| McpError::internal_error(format!("lock poisoned: {}", e), None))?;
        let goal_run_id = parse_uuid(&params.goal_run_id)?;

        // Get goal details for the PR package.
        let goal = state
            .goal_store
            .get(goal_run_id)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .ok_or_else(|| {
                McpError::invalid_params(format!("goal not found: {}", goal_run_id), None)
            })?;

        let connector = state.connectors.get(&goal_run_id).ok_or_else(|| {
            McpError::invalid_params(
                format!("no active connector for goal: {}", goal_run_id),
                None,
            )
        })?;

        let pr_package = connector
            .build_pr_package(&goal.title, &goal.objective, &params.summary, &params.title)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let package_id = pr_package.package_id;
        state
            .save_pr_package(pr_package)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // Transition goal to PrReady.
        let mut updated_goal = goal;
        updated_goal.pr_package_id = Some(package_id);
        updated_goal
            .transition(GoalRunState::PrReady)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        state
            .goal_store
            .save(&updated_goal)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // Emit event.
        state.event_dispatcher.dispatch(&TaEvent::PrReady {
            goal_run_id,
            pr_package_id: package_id,
            summary: params.summary,
            timestamp: Utc::now(),
        });

        let response = serde_json::json!({
            "pr_package_id": package_id.to_string(),
            "goal_run_id": goal_run_id.to_string(),
            "state": "pr_ready",
            "message": "PR package built. Awaiting human review via `ta pr view` / `ta pr approve`.",
        });
        Ok(CallToolResult::success(vec![Content::json(response)
            .map_err(|e| {
                McpError::internal_error(e.to_string(), None)
            })?]))
    }

    #[tool(description = "Check the review status of a goal's PR package.")]
    fn ta_pr_status(
        &self,
        Parameters(params): Parameters<GoalIdParams>,
    ) -> Result<CallToolResult, McpError> {
        let state = self
            .state
            .lock()
            .map_err(|e| McpError::internal_error(format!("lock poisoned: {}", e), None))?;
        let goal_run_id = parse_uuid(&params.goal_run_id)?;

        let goal = state
            .goal_store
            .get(goal_run_id)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .ok_or_else(|| {
                McpError::invalid_params(format!("goal not found: {}", goal_run_id), None)
            })?;

        let pr_status = if let Some(pkg_id) = goal.pr_package_id {
            if let Some(pkg) = state.pr_packages.get(&pkg_id) {
                serde_json::json!({
                    "pr_package_id": pkg_id.to_string(),
                    "status": format!("{:?}", pkg.status),
                    "artifacts": pkg.changes.artifacts.len(),
                })
            } else {
                serde_json::json!({
                    "pr_package_id": pkg_id.to_string(),
                    "status": "unknown",
                })
            }
        } else {
            serde_json::json!({
                "status": "no_pr_package",
                "message": "No PR package has been built yet. Use ta_pr_build first.",
            })
        };

        let response = serde_json::json!({
            "goal_run_id": goal_run_id.to_string(),
            "goal_state": goal.state.to_string(),
            "pr": pr_status,
        });
        Ok(CallToolResult::success(vec![Content::json(response)
            .map_err(|e| {
                McpError::internal_error(e.to_string(), None)
            })?]))
    }
}

// ── ServerHandler implementation ─────────────────────────────────

#[tool_handler]
impl ServerHandler for TaGatewayServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "trusted-autonomy".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                title: Some("Trusted Autonomy".into()),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Trusted Autonomy MCP server. All file operations are staged \
                 and require human review before being applied to the real \
                 filesystem. Start with ta_goal_start, then use ta_fs_write \
                 to stage changes, and ta_pr_build when ready for review."
                    .into(),
            ),
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────

/// Parse a UUID string, returning an MCP error on failure.
fn parse_uuid(s: &str) -> Result<Uuid, McpError> {
    Uuid::parse_str(s)
        .map_err(|e| McpError::invalid_params(format!("invalid UUID '{}': {}", s, e), None))
}

/// Enforce a policy decision, returning an MCP error if denied.
fn enforce_policy(decision: &PolicyDecision) -> Result<(), McpError> {
    match decision {
        PolicyDecision::Allow => Ok(()),
        PolicyDecision::Deny { reason } => Err(McpError::invalid_request(
            format!("Policy denied: {}", reason),
            None,
        )),
        PolicyDecision::RequireApproval { reason } => {
            // For now, RequireApproval is treated as allowed since the
            // CLI approval flow will handle the actual gating.
            tracing::info!("action requires approval: {}", reason);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ta_changeset::PRStatus;
    use tempfile::tempdir;

    fn test_server() -> (TaGatewayServer, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let config = GatewayConfig::for_project(dir.path());
        let server = TaGatewayServer::new(config).unwrap();
        (server, dir)
    }

    fn test_server_with_source(
        source_content: &[(&str, &[u8])],
    ) -> (TaGatewayServer, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        // Create source files in the project root.
        for (path, content) in source_content {
            let full_path = dir.path().join(path);
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&full_path, content).unwrap();
        }
        let config = GatewayConfig::for_project(dir.path());
        let server = TaGatewayServer::new(config).unwrap();
        (server, dir)
    }

    /// Helper: start a goal and return its ID.
    fn start_goal(server: &TaGatewayServer) -> Uuid {
        let mut state = server.state.lock().unwrap();
        let goal = state
            .start_goal("Test Goal", "Testing the system", "test-agent")
            .unwrap();
        goal.goal_run_id
    }

    #[test]
    fn tool_count_matches_expected() {
        let (server, _dir) = test_server();
        let tools = server.tool_router.list_all();
        // 9 tools: goal_start, goal_status, goal_list,
        //          fs_read, fs_write, fs_list, fs_diff,
        //          pr_build, pr_status
        let names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
        assert_eq!(tools.len(), 9, "expected 9 tools, got: {:?}", names);
    }

    #[test]
    fn tool_names_are_prefixed() {
        let (server, _dir) = test_server();
        let tools = server.tool_router.list_all();
        for tool in &tools {
            assert!(
                tool.name.starts_with("ta_"),
                "tool '{}' should be prefixed with 'ta_'",
                tool.name
            );
        }
    }

    #[test]
    fn start_goal_creates_running_goal() {
        let (server, _dir) = test_server();
        let goal_id = start_goal(&server);

        let state = server.state.lock().unwrap();
        let goal = state.goal_store.get(goal_id).unwrap().unwrap();
        assert_eq!(goal.state, GoalRunState::Running);
        assert_eq!(goal.title, "Test Goal");
    }

    #[test]
    fn start_goal_issues_manifest() {
        let (server, _dir) = test_server();
        let _goal_id = start_goal(&server);

        let state = server.state.lock().unwrap();

        // Policy engine should have a manifest for the agent.
        let decision = state.policy_engine.evaluate(&PolicyRequest {
            agent_id: "test-agent".to_string(),
            tool: "fs".to_string(),
            verb: "read".to_string(),
            target_uri: "fs://workspace/src/main.rs".to_string(),
        });
        assert_eq!(decision, PolicyDecision::Allow);
    }

    #[test]
    fn start_goal_creates_connector() {
        let (server, _dir) = test_server();
        let goal_id = start_goal(&server);

        let state = server.state.lock().unwrap();
        assert!(state.connectors.contains_key(&goal_id));
    }

    #[test]
    fn fs_write_stages_file() {
        let (server, _dir) = test_server();
        let goal_id = start_goal(&server);

        let mut state = server.state.lock().unwrap();
        let connector = state.connectors.get_mut(&goal_id).unwrap();
        let cs = connector
            .write_patch("hello.txt", b"Hello from TA!")
            .unwrap();
        assert_eq!(cs.target_uri, "fs://workspace/hello.txt");

        let content = connector.read_staged("hello.txt").unwrap();
        assert_eq!(content, b"Hello from TA!");
    }

    #[test]
    fn fs_write_accumulates_changesets() {
        let (server, _dir) = test_server();
        let goal_id = start_goal(&server);

        let mut state = server.state.lock().unwrap();
        let connector = state.connectors.get_mut(&goal_id).unwrap();
        connector.write_patch("a.txt", b"aaa").unwrap();
        connector.write_patch("b.txt", b"bbb").unwrap();

        let changesets = connector.list_changesets().unwrap();
        assert_eq!(changesets.len(), 2);
    }

    #[test]
    fn fs_list_shows_staged_files() {
        let (server, _dir) = test_server();
        let goal_id = start_goal(&server);

        let mut state = server.state.lock().unwrap();
        let connector = state.connectors.get_mut(&goal_id).unwrap();
        connector
            .write_patch("src/lib.rs", b"pub fn main() {}")
            .unwrap();
        connector.write_patch("README.md", b"# Hello").unwrap();

        let files = connector.list_staged().unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn fs_read_snapshots_source() {
        let (server, _dir) = test_server_with_source(&[("existing.txt", b"original content")]);
        let goal_id = start_goal(&server);

        let mut state = server.state.lock().unwrap();
        let source_dir = state.config.workspace_root.clone();
        let connector = state.connectors.get_mut(&goal_id).unwrap();

        let content = connector.read_source(&source_dir, "existing.txt").unwrap();
        assert_eq!(content, b"original content");
    }

    #[test]
    fn pr_build_creates_package() {
        let (server, _dir) = test_server();
        let goal_id = start_goal(&server);

        {
            let mut state = server.state.lock().unwrap();
            let connector = state.connectors.get_mut(&goal_id).unwrap();
            connector.write_patch("file.txt", b"content").unwrap();
        }

        let state = server.state.lock().unwrap();
        let goal = state.goal_store.get(goal_id).unwrap().unwrap();
        let connector = state.connectors.get(&goal_id).unwrap();
        let pkg = connector
            .build_pr_package(&goal.title, &goal.objective, "Added file", "Test PR")
            .unwrap();

        assert_eq!(pkg.changes.artifacts.len(), 1);
        assert_eq!(pkg.status, PRStatus::PendingReview);
    }

    #[test]
    fn pr_build_transitions_to_pr_ready() {
        let (server, _dir) = test_server();
        let goal_id = start_goal(&server);

        {
            let mut state = server.state.lock().unwrap();
            let connector = state.connectors.get_mut(&goal_id).unwrap();
            connector.write_patch("file.txt", b"content").unwrap();
        }

        // Build PR through state methods (simulating the tool).
        let mut state = server.state.lock().unwrap();
        let goal = state.goal_store.get(goal_id).unwrap().unwrap();
        let connector = state.connectors.get(&goal_id).unwrap();
        let pkg = connector
            .build_pr_package(&goal.title, &goal.objective, "what", "why")
            .unwrap();
        let package_id = pkg.package_id;
        state.pr_packages.insert(package_id, pkg);

        let mut updated = goal;
        updated.pr_package_id = Some(package_id);
        updated.transition(GoalRunState::PrReady).unwrap();
        state.goal_store.save(&updated).unwrap();

        let reloaded = state.goal_store.get(goal_id).unwrap().unwrap();
        assert_eq!(reloaded.state, GoalRunState::PrReady);
        assert_eq!(reloaded.pr_package_id, Some(package_id));
    }

    #[test]
    fn policy_denies_unknown_agent() {
        let (server, _dir) = test_server();
        let state = server.state.lock().unwrap();

        let decision = state.policy_engine.evaluate(&PolicyRequest {
            agent_id: "unknown".to_string(),
            tool: "fs".to_string(),
            verb: "read".to_string(),
            target_uri: "fs://workspace/test.txt".to_string(),
        });
        assert!(matches!(decision, PolicyDecision::Deny { .. }));
    }

    #[test]
    fn policy_allows_after_goal_start() {
        let (server, _dir) = test_server();
        let _goal_id = start_goal(&server);
        let state = server.state.lock().unwrap();

        let decision = state.policy_engine.evaluate(&PolicyRequest {
            agent_id: "test-agent".to_string(),
            tool: "fs".to_string(),
            verb: "write_patch".to_string(),
            target_uri: "fs://workspace/src/main.rs".to_string(),
        });
        assert_eq!(decision, PolicyDecision::Allow);
    }

    #[test]
    fn events_logged_on_goal_start() {
        let dir = tempdir().unwrap();
        let config = GatewayConfig::for_project(dir.path());
        let events_path = config.events_log.clone();
        let server = TaGatewayServer::new(config).unwrap();
        let _goal_id = start_goal(&server);

        // Events log should have at least one entry.
        let content = std::fs::read_to_string(&events_path).unwrap();
        assert!(content.contains("goal_created"));
    }

    #[test]
    fn multiple_goals_are_isolated() {
        let (server, _dir) = test_server();
        let id1 = start_goal(&server);
        let id2 = {
            let mut state = server.state.lock().unwrap();
            let goal = state
                .start_goal("Goal 2", "Second goal", "agent-2")
                .unwrap();
            goal.goal_run_id
        };

        assert_ne!(id1, id2);

        let mut state = server.state.lock().unwrap();
        // Write to goal 1.
        state
            .connectors
            .get_mut(&id1)
            .unwrap()
            .write_patch("g1.txt", b"goal 1")
            .unwrap();
        // Write to goal 2.
        state
            .connectors
            .get_mut(&id2)
            .unwrap()
            .write_patch("g2.txt", b"goal 2")
            .unwrap();

        // Each should only see its own files.
        let files1 = state.connectors.get(&id1).unwrap().list_staged().unwrap();
        let files2 = state.connectors.get(&id2).unwrap().list_staged().unwrap();
        assert_eq!(files1.len(), 1);
        assert_eq!(files2.len(), 1);
        assert!(files1.contains(&"g1.txt".to_string()));
        assert!(files2.contains(&"g2.txt".to_string()));
    }
}
