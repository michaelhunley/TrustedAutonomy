# Trusted Autonomy — Development Plan

> Canonical plan for the project. Machine-parseable: each phase has a `<!-- status: done|in_progress|pending -->` marker.
> Updated automatically by `ta pr apply` when a goal with `--phase` completes.

---

## Phase 0 — Repo Layout & Core Data Model
<!-- status: done -->
Workspace structure with 12 crates under `crates/` and `apps/`. Resource URIs (`fs://workspace/<path>`, `gmail://`, etc.), ChangeSet as universal staged mutation, capability manifests, PR package schema.

## Phase 1 — Kernel: Audit, Policy, Changeset, Workspace
<!-- status: done -->
- `ta-audit` (13 tests): Append-only JSONL log with SHA-256 hash chain
- `ta-policy` (16 tests): Default-deny capability engine with glob pattern matching on URIs
- `ta-changeset` (14 tests): ChangeSet + PRPackage data model aligned with schema/pr_package.schema.json
- `ta-workspace` (29 tests): StagingWorkspace + OverlayWorkspace + ExcludePatterns + ChangeStore + JsonFileStore

## Phase 2 — MCP Gateway, Goal Lifecycle, CLI
<!-- status: done -->
- `ta-connector-fs` (11+1 tests): FsConnector bridging MCP to staging
- `ta-goal` (20 tests): GoalRun lifecycle state machine + event dispatch
- `ta-mcp-gateway` (15 tests): Real MCP server using rmcp 0.14 with 9 tools
- `ta-daemon`: MCP server binary (stdio transport, tokio async)
- `ta-cli` (15+1 tests): goal start/list/status/delete, pr build/list/view/approve/deny/apply, run, audit, adapter, serve

## Phase 3 — Transparent Overlay Mediation
<!-- status: done -->
- OverlayWorkspace: full copy of source to staging (.ta/ excluded)
- ExcludePatterns (V1 TEMPORARY): .taignore or defaults (target/, node_modules/, etc.)
- Flow: `ta goal start` → copy source → agent works in staging → `ta pr build` → diff → PRPackage → approve → apply
- CLAUDE.md injection: `ta run` prepends TA context, saves backup, restores before diff
- AgentLaunchConfig: per-agent configs (claude-code uses --dangerously-skip-permissions)
- Git integration: `ta pr apply --git-commit` runs git add + commit after applying
- Dogfooding validated: 1.6MB staging copy with exclude patterns

## Phase 4a — Agent Prompt Enhancement
<!-- status: done -->
- CLAUDE.md injection includes instructions for `.ta/change_summary.json`
- Agent writes per-file rationale + dependency info (depends_on, depended_by, independent)
- Foundation for selective approval (Phase 4c)

## Phase 4a.1 — Plan Tracking & Lifecycle
<!-- status: done -->
- Canonical PLAN.md with machine-parseable status markers
- GoalRun.plan_phase links goals to plan phases
- `ta plan list/status` CLI commands
- CLAUDE.md injection includes plan progress context
- `ta pr apply` auto-updates PLAN.md when phase completes

## Phase 4b — Per-Artifact Review Model
<!-- status: in_progress -->
- ArtifactDisposition enum: Pending / Approved / Rejected / Discuss (per artifact, not per package)
- ChangeDependency struct for agent-reported inter-file dependencies
- URI-aware pattern matching: scheme-scoped glob (fs:// patterns can't match gmail:// URIs)
- Bare patterns auto-prefix with `fs://workspace/` for convenience
- `ta pr build` reads `.ta/change_summary.json` into PRPackage dependency metadata

## Phase 4c — Selective Review CLI
<!-- status: pending -->
- `ta pr apply <id> --approve "src/**" --reject "*.test.rs" --discuss "config/*"`
- Special values: `all` (everything), `rest` (everything not explicitly listed)
- Selective apply: only copies approved artifacts; tracks partial application state
- Coupled-change warnings: reject B also requires rejecting A if dependent

## Phase v0.1 — Public Preview & Call for Feedback
<!-- status: pending -->
**Goal**: Get TA in front of early adopters for feedback. Not production-ready — explicitly disclaimed.

### Required for v0.1
- **Simple install**: `cargo install ta-cli` or single binary download (cross-compile for macOS/Linux)
- **Agent setup guides**: Step-by-step for Claude Code, Claude Flow (when available), Codex/similar
- **README rewrite**: Quick-start in <5 minutes, architecture overview, what works / what doesn't
- **`ta adapter install claude-code`** works end-to-end (already partially implemented)
- **Smoke-tested happy path**: `ta run "task" --source .` → review → approve → apply works reliably
- **Error messages**: Graceful failures with actionable guidance (not panics or cryptic errors)
- **.taignore defaults** cover common project types (Rust, Node, Python, Go)

### Disclaimers to include
- "Alpha — not production-ready. Do not use for critical/irreversible operations"
- "The security model is not yet audited. Do not trust it with secrets or sensitive data"
- "Selective approval (Phase 4b-4c) is not yet implemented — review is all-or-nothing"
- "No sandbox isolation yet — agent runs with your permissions in a staging copy"

### Nice-to-have for v0.1
- `ta pr view` shows colored diffs in terminal
- Basic telemetry opt-in (anonymous usage stats for prioritization)
- GitHub repo with issues template for feedback
- Short demo video / animated GIF in README

### What feedback to solicit
- "Does the staging → PR → review → apply flow make sense for your use case?"
- "What agents do you want to use with this? What's missing for your agent?"
- "What connectors matter most? (Gmail, Drive, DB, Slack, etc.)"
- "Would you pay for a hosted version? What would that need to include?"

## Phase 4d — Review Sessions
<!-- status: pending -->
- ReviewSession persists across CLI invocations (multi-interaction review)
- Per-artifact comment threads
- Supervisor agent analyzes dependency graph and warns about coupled rejections
- Discussion workflow for `?` (discuss) items

## Phase 4e — Plan Lifecycle Automation
<!-- status: pending -->
- Supervisor agent reads change_summary.json, validates completed work against plan
- Completing one phase auto-suggests/creates goal for next pending phase
- Plan templates for common workflows (feature, bugfix, refactor)
- `ta plan next` command to create goal for next pending phase
- Plan versioning and history

## Phase 5 — Intent-to-Access Planner
<!-- status: pending -->
- LLM "Intent-to-Policy Planner" outputs AgentSetupProposal (JSON)
- Deterministic Policy Compiler validates proposal (subset of templates, staged semantics, budgets)
- Agent setup becomes an "Agent PR" requiring approval before activation
- User goal → proposed agent roster + scoped capabilities + milestone plan

## Phase 6 — First External Connector
<!-- status: pending -->
Options (choose one):
- **Gmail staging**: read threads, create draft (ChangeSet), send gated by approval
- **Drive staging**: read doc, write_patch + diff preview, commit gated
- **DB staging**: write_patch as transaction log + preview, commit gated

## Phase 7 — Sandbox Runner
<!-- status: pending -->
- OCI/gVisor sandbox for agent execution
- Allowlisted command execution (rg, fmt, test profiles)
- CWD enforcement — agents can't escape workspace
- Command transcripts hashed into audit log

## Phase 8 — Distribution & Packaging
<!-- status: pending -->
- Developer: `cargo run` + local config + Nix
- Desktop: installer with bundled daemon, git, rg/jq
- Cloud: OCI image for daemon + connectors, ephemeral workspaces
- Web UI for review/approval (localhost → LAN → cloud)
