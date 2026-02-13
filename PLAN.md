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
- AgentLaunchConfig: per-agent configs with settings injection (replaces --dangerously-skip-permissions)
- Settings injection: `.claude/settings.local.json` with allow/deny lists + community `.ta-forbidden-tools` deny file
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
<!-- status: done -->
- [x] ArtifactDisposition enum: Pending / Approved / Rejected / Discuss (per artifact, not per package)
- [x] ChangeDependency struct for agent-reported inter-file dependencies
- [x] URI-aware pattern matching: scheme-scoped glob (fs:// patterns can't match gmail:// URIs)
- [x] Bare patterns auto-prefix with `fs://workspace/` for convenience; `*` respects `/`, `**` for deep
- [x] `ta pr build` reads `.ta/change_summary.json` into PRPackage (rationale, dependencies, summary)
- [x] `ta pr view` displays per-artifact rationale and dependencies

## Phase 4c — Selective Review CLI
<!-- status: done -->
- `ta pr apply <id> --approve "src/**" --reject "*.test.rs" --discuss "config/*"`
- Special values: `all` (everything), `rest` (everything not explicitly listed)
- Selective apply: only copies approved artifacts; tracks partial application state
- Coupled-change warnings: reject B also requires rejecting A if dependent

## Phase v0.1 — Public Preview & Call for Feedback
<!-- status: pending -->
**Goal**: Get TA in front of early adopters for feedback. Not production-ready — explicitly disclaimed.

### Required for v0.1
- [x] **Version info**: `ta --version` shows `0.1.0-alpha (git-hash date)`, build.rs embeds git metadata
- **Simple install**: `cargo install ta-cli` or single binary download (cross-compile for macOS/Linux)
- **Agent launch configs as YAML**: Replace hard-coded `AgentLaunchConfig` match arms with discoverable YAML files (e.g., `agents/claude-code.yaml`, `agents/claude-flow.yaml`). Ship built-in defaults, allow user overrides in `.ta/agents/` or `~/.config/ta/agents/`. Schema: command, args_template (`{prompt}` substitution), injects_context_file, env vars, description.
- **Agent setup guides**: Step-by-step for Claude Code, Claude Flow (when available), Codex/similar
- **README rewrite**: Quick-start in <5 minutes, architecture overview, what works / what doesn't
- **`ta adapter install claude-code`** works end-to-end (already partially implemented)
- **Smoke-tested happy path**: `ta run "task" --source .` → review → approve → apply works reliably
- **Error messages**: Graceful failures with actionable guidance (not panics or cryptic errors)
- **.taignore defaults** cover common project types (Rust, Node, Python, Go)

### Disclaimers to include (added to README)
- "Alpha — not production-ready. Do not use for critical/irreversible operations"
- "The security model is not yet audited. Do not trust it with secrets or sensitive data"
- ~~"Selective approval (Phase 4b-4c) is not yet implemented — review is all-or-nothing"~~ — DONE (Phase 4b-4c complete)
- "No sandbox isolation yet — agent runs with your permissions in a staging copy"
- "No conflict detection yet — editing source files while a TA session is active may lose changes on apply (git protects committed work)"

### Nice-to-have for v0.1
- `ta pr view` shows colored diffs in terminal
- Basic telemetry opt-in (anonymous usage stats for prioritization)
- GitHub repo with issues template for feedback
- Short demo video / animated GIF in README
- **Git workflow config** (`.ta/workflow.toml`): branch naming, auto-PR on apply — see Phase v0.2

### What feedback to solicit
- "Does the staging → PR → review → apply flow make sense for your use case?"
- "What agents do you want to use with this? What's missing for your agent?"
- "What connectors matter most? (Gmail, Drive, DB, Slack, etc.)"
- "Would you pay for a hosted version? What would that need to include?"

## Phase v0.1.1 — Release Automation & Binary Distribution
<!-- status: pending -->
- **GitHub Actions CI**: lint (clippy + fmt), test, build on push/PR
- **Cross-compile matrix**: macOS aarch64 + x86_64, Linux aarch64 + x86_64 (musl static)
- **GitHub Releases**: `gh release create v0.1.0-alpha --generate-notes` with attached binaries
- **`cargo install ta-cli`**: Ensure crate publishes to crates.io (verify metadata, dependencies)
- **Install script**: `curl -fsSL https://ta.dev/install.sh | sh` one-liner (download + place in PATH)
- **Version bumping**: `cargo release` or manual Cargo.toml + git tag workflow
- **Nix flake output**: `nix run github:trustedautonomy/ta` for Nix users
- **Homebrew formula**: Future — tap for macOS users (`brew install trustedautonomy/tap/ta`)

## Phase v0.2 — Git Workflow Automation
<!-- status: pending -->
- **Workflow config** (`.ta/workflow.toml`): user-defined git workflow preferences
  - `branch_prefix`: naming convention for auto-created branches (e.g., `ta/`, `feature/`)
  - `auto_branch`: create a feature branch automatically on `ta goal start`
  - `auto_pr`: open a GitHub/GitLab PR automatically after `ta pr apply --git-commit`
  - `pr_template`: path to PR body template with `{summary}`, `{artifacts}`, `{plan_phase}` substitution
  - `merge_strategy`: `squash` | `merge` | `rebase` (default: `squash`)
  - `target_branch`: base branch for PRs (default: `main`)
- **`ta pr apply --git-commit --push`** creates branch + commit + push + PR in one command
- **Branch lifecycle**: `ta goal start` creates `ta/<goal-id-short>-<slug>`, `ta pr apply` pushes and opens PR
- **CLAUDE.md injection**: injects branch workflow instructions so agents commit to feature branches, not `main`
- **Backwards-compatible**: workflow config is optional; without it, current behavior is preserved

## Phase 4c.1 — Concurrent Session Conflict Detection
<!-- status: pending -->
- Detect when source files have changed since staging copy was made (stale overlay)
- On `ta pr apply`: compare source file mtime/hash against snapshot taken at `ta goal start`
- Conflict resolution strategies: abort, merge (delegate to git merge), force-overwrite
- **Current limitation**: if you edit source files while a TA session is active, `ta pr apply` will silently overwrite those changes. Git handles this for committed code, but uncommitted edits can be lost.
- Display warnings at PR review time if source has diverged
- Future: lock files or advisory locks for active goals
- **Multi-agent intra-staging conflicts**: When multiple agents work in the same staging workspace (e.g., via Claude Flow swarms), consider integrating [agentic-jujutsu](https://github.com/ruvnet/claude-flow) for lock-free concurrent file operations with auto-merge. This handles agent-to-agent coordination; TA handles agent-to-human review. Different layers, composable.

## Phase 4c.2 — External Diff Routing
<!-- status: pending -->
- Config file (`.ta/diff-handlers.toml` or similar) maps file patterns to external applications
- Examples: `*.uasset` → Unreal Editor, `*.png` → image diff tool, `*.blend` → Blender
- `ta pr view <id> --file model.uasset` opens the file in the configured handler
- Default handlers: text → inline diff (current), binary → byte count summary
- Integration with OS `open` / `xdg-open` as fallback

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

## Phase 9 — Event System & Orchestration API
<!-- status: pending -->
> See `docs/VISION-virtual-office.md` for full vision.
- `--json` output flag on all CLI commands for programmatic consumption
- Event hook execution: call webhooks/scripts on goal + PR state transitions
- `ta events listen` command — stream JSON events for external consumers
- Stable event schema matching `docs/plugins-architecture-guidance.md` hooks
- Non-interactive approval API: token-based approve/reject (for Slack buttons, email replies)
- Foundation for notification connectors and virtual office runtime

## Phase 10 — Notification Connectors
<!-- status: pending -->
- `ta-connector-notify-email`: SMTP PR summaries + reply-to-approve parsing
- `ta-connector-notify-slack`: Slack app with Block Kit PR cards + button callbacks
- `ta-connector-notify-discord`: Discord bot with embed summaries + reaction handlers
- Bidirectional: outbound notifications + inbound approval actions
- Unified config: `notification_channel` per role/goal

## Phase 11 — Virtual Office Runtime
<!-- status: pending -->
> Thin orchestration layer that composes TA, Claude Flow, and notification connectors.
- Role definition schema (YAML): purpose, triggers, agent, capabilities, notification channel
- Trigger system: cron scheduler + webhook receiver + TA event listener
- Office manager daemon: reads role configs, routes triggers, calls `ta run`
- `ta office start/stop/status` CLI commands
- Role-scoped TA policies auto-generated from role capability declarations
- Integration with Claude Flow as the agent coordination backend
- Does NOT duplicate orchestration — composes existing tools with role/trigger glue