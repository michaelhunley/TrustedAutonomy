# Trusted Autonomy — Development Plan

> Canonical plan for the project. Machine-parseable: each phase has a `<!-- status: done|in_progress|pending -->` marker.
> Updated automatically by `ta pr apply` when a goal with `--phase` completes.

## Versioning & Release Policy

- **Version format**: `MAJOR.MINOR.PATCH-alpha` (semver). Current: `v0.1.2-alpha`.
- **Release tags**: Each `vX.Y.0` phase is a **release point** — cut a git tag and publish binaries.
- **Patch phases** (`vX.Y.1`, `vX.Y.2`) are incremental work within a release cycle.
- **When completing a phase**, the implementing agent MUST:
  1. Update `version` in `apps/ta-cli/Cargo.toml` to the phase's version (e.g., `0.2.0-alpha`)
  2. Update the "Current State" section in `CLAUDE.md` with the new version and test count
  3. Mark the phase as `done` in this file
- **Pre-v0.1 phases** (Phase 0–4c) used internal numbering. All phases from v0.1 onward use version-based naming.

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
- [x] **Agent launch configs as YAML**: YAML files in `agents/` (claude-code.yaml, codex.yaml, claude-flow.yaml, generic.yaml). Config search: `.ta/agents/` (project) → `~/.config/ta/agents/` (user) → shipped defaults → hard-coded fallback. Schema: command, args_template (`{prompt}`), injects_context_file, injects_settings, pre_launch, env. Added `serde_yaml` dep, 2 tests.
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
- `ta pr view --file` accepts **comma-separated list** to review select files (e.g., `--file src/main.rs,src/lib.rs`)
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
<!-- status: in_progress -->

### Done
- [x] **GitHub Actions CI** (`.github/workflows/ci.yml`): lint (clippy + fmt), test, build on push/PR
  - Ubuntu + macOS matrix, Nix devShell via DeterminateSystems/nix-installer-action
  - Magic Nix Cache (no auth token needed), step timeouts, graceful degradation
- [x] **Release workflow** (`.github/workflows/release.yml`): triggered by version tag or manual dispatch
  - Cross-compile matrix: macOS aarch64 + x86_64 (native), Linux x86_64 + aarch64 (musl via `cross`)
  - Creates GitHub Release with binary tarballs + SHA256 checksums
  - Publishes to crates.io (requires `CARGO_REGISTRY_TOKEN` secret)

### Remaining
- **Validate release end-to-end** (manual — see checklist below)
- **Install script**: `curl -fsSL https://ta.dev/install.sh | sh` one-liner (download + place in PATH)
- **Version bumping**: `cargo release` or manual Cargo.toml + git tag workflow
- **Auto-generated release notes**: Collect PR titles merged since last tag and format into GitHub Release body. Use `gh api repos/{owner}/{repo}/releases/generate-notes` or `git log --merges --oneline <prev-tag>..HEAD`. Optionally configurable via `.ta/release.toml` (include/exclude labels, group by category).
- **Nix flake output**: `nix run github:trustedautonomy/ta` for Nix users
- **Homebrew formula**: Future — tap for macOS users (`brew install trustedautonomy/tap/ta`)

### Release Validation Checklist (manual, one-time)
These steps must be done by the repo owner to validate the release pipeline:

1. **Set GitHub secrets** (Settings → Secrets and variables → Actions):
   - `CARGO_REGISTRY_TOKEN` — from `cargo login` / crates.io API tokens page
   - (Optional) `CACHIX_AUTH_TOKEN` — only needed if you want to push Nix cache binaries

2. **Verify CI passes on a PR to main**:
   ```bash
   git checkout feature/release-automation
   gh pr create --base main --title "Release Automation" --body "CI + release workflows"
   # Wait for CI checks to pass on both Ubuntu and macOS
   ```

3. **Merge to main** and verify CI runs on the main branch push.

4. **Test release workflow** (dry run via manual dispatch):
   ```bash
   # From GitHub Actions tab → Release → Run workflow → enter tag "v0.1.0-alpha"
   # Or from CLI:
   gh workflow run release.yml -f tag=v0.1.0-alpha
   ```
   - Verify: 4 binary artifacts built (2× macOS, 2× Linux musl)
   - Verify: GitHub Release page created with binaries + checksums
   - Verify: crates.io publish attempted (will fail if metadata incomplete — check Cargo.toml)

5. **Test the binaries**:
   ```bash
   # Download and verify on macOS:
   tar xzf ta-v0.1.0-alpha-aarch64-apple-darwin.tar.gz
   ./ta --version
   # Should show: ta 0.1.0-alpha (git-hash date)
   ```

6. **Validate `cargo install`** (after crates.io publish succeeds):
   ```bash
   cargo install ta-cli
   ta --version
   ```

## Phase v0.1.2 — Follow-Up Goals & Iterative Review
<!-- status: done -->
**Goal**: Enable iterative refinement — fix CI failures, address discuss items, revise rejected changes — without losing context from the original goal.

### Core: `ta goal start "title" --follow-up [id]` ✅ **Implemented**
- ✅ `--follow-up` without ID: finds the most recent goal (prefers unapplied, falls back to latest applied)
- ✅ `--follow-up <id-prefix>`: match by first N characters of goal UUID (no full hash needed)
- ✅ `GoalRun` gets `parent_goal_id: Option<Uuid>` linking to the predecessor

### Staging Behavior (depends on parent state)

> **Note (v0.1.2 implementation)**: The optimization to start from parent staging is **deferred to a future release**. Current implementation always starts from source, which works correctly but may require manually re-applying parent changes when parent PR is unapplied. The parent context injection and PR supersession work as designed.

**Parent NOT yet applied** (PrReady / UnderReview / Approved) — *Planned optimization*:
- Follow-up staging should start from the **parent's staging** (preserves in-flight work)
- `ta pr build` should diff against the **original source** (same base as parent)
- The follow-up's PR **supersedes** the parent's PR — single unified diff covering both rounds ✅ **Implemented**
- Parent PR status transitions to `Superseded { superseded_by: Uuid }` ✅ **Implemented**
- Result: one collapsed PR for review, not a chain of incremental PRs

**Parent already applied** (Applied / Completed) — *Current behavior*:
- Follow-up staging starts from **current source** (which already has applied changes) ✅ **Implemented**
- Creates a new, independent PR for the follow-up changes ✅ **Implemented**
- Parent link preserved for audit trail / context injection only ✅ **Implemented**

### Context Injection ✅ **Implemented**
When a follow-up goal starts, `inject_claude_md()` includes parent context:
- ✅ Parent goal title, objective, summary (what was done)
- ✅ Artifact list with dispositions (what was approved/rejected/discussed)
- ✅ Any discuss items with their rationale (from `change_summary.json`)
- ✅ Free-text follow-up context from the objective field

**Specifying detailed context**:
- ✅ Short: `ta run "Fix CI lint failures" --source . --follow-up` (title IS the context)
- ✅ Detailed: `ta run --source . --follow-up --objective "Fix clippy warnings in pr.rs and add missing test for edge case X. Also address the discuss item on config.toml — reviewer wanted env var override support."` (objective field scales to paragraphs)
- ✅ From file: `ta run --source . --follow-up --objective-file review-notes.md` (for structured review notes)
- **Phase 4d integration** (future): When discuss items have comment threads (Phase 4d), those comments auto-populate follow-up context — each discussed artifact's thread becomes a structured section in CLAUDE.md injection. The `--follow-up` flag on a goal with discuss items is the resolution path for Phase 4d's discussion workflow.

### CLI Changes
- ✅ `ta goal start` / `ta run`: add `--follow-up [id-prefix]` and `--objective-file <path>` flags
- ✅ `ta goal list`: show parent chain (`goal-abc → goal-def (follow-up)`)
- ✅ `ta pr list`: show superseded PRs with `[superseded]` marker
- ✅ `ta pr build`: when parent PR exists and is unapplied, mark it superseded

### Data Model Changes
- ✅ `GoalRun`: add `parent_goal_id: Option<Uuid>`
- ✅ `PRStatus`: add `Superseded { superseded_by: Uuid }` variant
- ✅ `PRPackage`: no changes (the new PR package is a complete, standalone package)

### Phase 4d Note
> Follow-up goals are the **resolution mechanism** for Phase 4d discuss items. When 4d adds per-artifact comment threads and persistent review sessions, `--follow-up` on a goal with unresolved discuss items will inject those threads as structured agent instructions. The agent addresses each discussed artifact; the resulting PR supersedes the original. This keeps discuss → revise → re-review as a natural loop without new CLI commands — just `ta run --follow-up`.

---

## v0.2 — Submit Adapters & Workflow Automation *(release: tag v0.2.0-alpha)*

### v0.2.0 — SubmitAdapter Trait & Git Implementation
<!-- status: done -->
**Architecture**: The staging→review→apply loop is VCS-agnostic. "Submit" is a pluggable adapter — git is the first implementation, but the trait supports Perforce, SVN, plain file copy, or non-code workflows (art pipelines, document review).

#### SubmitAdapter Trait (`crates/ta-workspace` or new `crates/ta-submit`)
```rust
pub trait SubmitAdapter: Send + Sync {
    /// Create a working branch/changelist/workspace for this goal.
    fn prepare(&self, goal: &GoalRun, config: &SubmitConfig) -> Result<()>;
    /// Commit/shelve the approved changes from staging.
    fn commit(&self, goal: &GoalRun, pr: &PRPackage, message: &str) -> Result<CommitResult>;
    /// Push/submit the committed changes for review.
    fn push(&self, goal: &GoalRun) -> Result<PushResult>;
    /// Open a review request (GitHub PR, Perforce review, email, etc.).
    fn open_review(&self, goal: &GoalRun, pr: &PRPackage) -> Result<ReviewResult>;
    /// Adapter display name (for CLI output).
    fn name(&self) -> &str;
}
```
`CommitResult`, `PushResult`, `ReviewResult` are adapter-neutral structs carrying identifiers (commit hash, changelist number, PR URL, etc.).

#### Built-in Adapters
- **`git`** (default): Git branching + GitHub/GitLab PR creation
  - `branch_prefix`: naming convention for auto-created branches (e.g., `ta/`, `feature/`)
  - `auto_branch`: create a feature branch automatically on `ta goal start`
  - `auto_review`: open a GitHub/GitLab PR automatically after commit+push
  - `pr_template`: path to PR body template with `{summary}`, `{artifacts}`, `{plan_phase}` substitution
  - `merge_strategy`: `squash` | `merge` | `rebase` (default: `squash`)
  - `target_branch`: base branch for PRs (default: `main`)
  - `remote`: git remote name (default: `origin`)
- **`none`** (fallback): Just copy files back to source. No VCS operations. Current behavior when no config exists.
- **Future adapters** (not in v0.2): `perforce` (changelists + Swarm), `svn`, `art-pipeline` (file copy + notification)

#### Workflow Config (`.ta/workflow.toml`)
```toml
[submit]
adapter = "git"                    # or "none"; future: "perforce", "svn"
auto_commit = true                 # commit on ta pr apply
auto_push = true                   # push after commit
auto_review = true                 # open PR/review after push

[submit.git]                       # adapter-specific settings
branch_prefix = "ta/"
target_branch = "main"
merge_strategy = "squash"
pr_template = ".ta/pr-template.md"
```

#### CLI Changes
- **`ta pr apply <id> --submit`** runs the full adapter pipeline: commit → push → open review
- **`ta pr apply <id> --git-commit`** remains as shorthand (equivalent to `--submit` with git adapter, no push)
- **`ta pr apply <id> --git-commit --push`** equivalent to `--submit` with git adapter + push + open review
- **Branch lifecycle**: `ta goal start` calls `adapter.prepare()` (git: creates branch), `ta pr apply --submit` calls commit → push → open_review

#### Integration Points
- **CLAUDE.md injection**: injects workflow instructions so agents respect the configured VCS (e.g., commit to feature branches for git, don't touch VCS for `none`)
- **Backwards-compatible**: without `.ta/workflow.toml`, behavior is identical to today (`none` adapter — just file copy)
- **Agent launch configs**: YAML agent configs can reference workflow adapter for prompt context

#### Future Extensibility & Design Evolution
**Vision**: The `SubmitAdapter` pattern is designed to extend beyond VCS to any "submit" workflow where changes need approval before affecting the outside world.

**Potential Non-VCS Adapters** (post-v0.2):
- **Webhook/API adapter**: POST PRPackage JSON to REST endpoints for external review systems
- **Email adapter**: Send PR summaries via SMTP with reply-to-approve workflows (integrates with v0.9 notification connectors)
- **Storage adapter**: Upload artifacts to S3/GCS/Drive with shareable review links
- **Ticketing adapter**: Create JIRA/Linear/GitHub Issues for review workflows
- **Slack/Discord adapter**: Post review requests as interactive messages with approval buttons (v0.9 integration)

**Architectural Decision (v0.3+ if needed)**:
- **Recommendation**: Keep `SubmitAdapter` VCS-focused for clarity. Introduce parallel traits for other domains:
  - `NotifyAdapter` — for notification/communication workflows (v0.9)
  - `PublishAdapter` — for API/webhook publishing workflows (v0.4-v0.5 timeframe)
  - `StorageAdapter` — for artifact upload/sharing workflows (v0.5 timeframe)
- **Rationale**: Specialized traits provide clearer semantics than forcing all workflows through VCS-oriented method names (prepare/commit/push/open_review). Each domain gets methods that make semantic sense for that domain.
- **Alternative considered**: Generalize `SubmitAdapter` methods to `prepare/submit/request_review/finalize`. Rejected because VCS workflows are the primary use case and generic names lose clarity.

**Roadmap Integration**:
- **v0.3-v0.4**: If demand arises, introduce `PublishAdapter` for webhook/API submission workflows
- **v0.5**: Evaluate `StorageAdapter` for external connector integration (Gmail, Drive per existing plan)
- **v0.9**: `NotifyAdapter` integrates with notification connectors (email, Slack, Discord)
- **v1.0**: Virtual office roles can compose multiple adapter types (VCS + notifications + storage) for comprehensive workflows

**Design Principle**: "Submit" isn't just VCS — it's any workflow where changes need approval before affecting external state. The adapter pattern enables pluggable approval workflows across all domains.

### v0.2.1 — Concurrent Session Conflict Detection
<!-- status: done -->
- Detect when source files have changed since staging copy was made (stale overlay)
- On `ta pr apply`: compare source file mtime/hash against snapshot taken at `ta goal start`
- Conflict resolution strategies: abort, merge (delegate to VCS adapter's merge if available), force-overwrite
- `SourceSnapshot` captured automatically at overlay creation (mtime + SHA-256)
- `--conflict-resolution abort|force-overwrite|merge` CLI flag on `ta pr apply`
- `apply_with_conflict_check()` aborts on conflict by default, warns and proceeds on force-overwrite
- 8 unit tests + integration tests
- **Remaining**: lock files or advisory locks for active goals (deferred to future)
- **Adapter integration**: git adapter can use `git merge`/`git diff` for smarter conflict resolution; `none` adapter falls back to mtime/hash comparison only
- **Multi-agent intra-staging conflicts**: When multiple agents work in the same staging workspace (e.g., via Claude Flow swarms), consider integrating [agentic-jujutsu](https://github.com/ruvnet/claude-flow) for lock-free concurrent file operations with auto-merge. This handles agent-to-agent coordination; TA handles agent-to-human review. Different layers, composable.

### v0.2.2 — External Diff Routing
<!-- status: done -->
- ✅ Config file (`.ta/diff-handlers.toml`) maps file patterns to external applications
- ✅ Examples: `*.uasset` → Unreal Editor, `*.png` → image diff tool, `*.blend` → Blender
- ✅ `ta pr view <id> --file model.uasset` opens the file in the configured handler
- ✅ Default handlers: text → inline diff (current), binary → byte count summary
- ✅ Integration with OS `open` / `xdg-open` as fallback
- ✅ New module: `ta-changeset::diff_handlers` with TOML parsing and pattern matching
- ✅ CLI flags: `--open-external` (default) / `--no-open-external` to control behavior
- ✅ Documentation and example config at `.ta/diff-handlers.example.toml`

### v0.2.3 — Tiered Diff Explanations & Output Adapters
<!-- status: done -->
**Goal**: Rich, layered diff review — top-level summary → medium detail → full diff, with pluggable output formatting.

#### Tiered Explanation Model
Each artifact in a PR gets a three-tier explanation:
1. **Top**: One-line summary (e.g., "Refactored auth middleware to use JWT")
2. **Medium**: Paragraph explaining what changed and why, dependencies affected
3. **Detail**: Full unified diff with inline annotations

Agents populate tiers via sidecar files: `<filename>.diff.explanation.yaml` (or JSON) written alongside changes. Schema:
```yaml
file: src/auth/middleware.rs
summary: "Refactored auth middleware to use JWT instead of session tokens"
explanation: |
  Replaced session-based auth with JWT validation. The middleware now
  checks the Authorization header for a Bearer token, validates it
  against the JWKS endpoint, and extracts claims into the request context.
  This change touches 3 files: middleware.rs (core logic), config.rs
  (JWT settings), and tests/auth_test.rs (updated test fixtures).
tags: [security, breaking-change]
related_artifacts:
  - src/auth/config.rs
  - tests/auth_test.rs
```

#### Output Adapters (Plugin System)
Configurable output renderers for `ta pr view`, designed for reuse:
- **terminal** (default): Colored inline diff with collapsible tiers (summary → expand for detail)
- **markdown**: Render PR as `.md` file — useful for GitHub PR bodies or documentation
- **json**: Machine-readable structured output for CI/CD integration
- **html**: Standalone review page with expandable sections (JavaScript-free progressive disclosure)
- Config: `.ta/output.toml` or `--format <adapter>` flag on `ta pr view`
- Plugin interface: adapter receives `PRPackage` + explanation sidecars, returns formatted output
- Adapters are composable: `ta pr view <id> --format markdown > review.md`

#### CLI Changes
- `ta pr view <id> --detail top|medium|full` (default: medium — shows summary + explanation, not full diff)
- `ta pr view <id> --format terminal|markdown|json|html`
- `ta pr build` ingests `*.diff.explanation.yaml` sidecars into PRPackage (similar to `change_summary.json`)
- CLAUDE.md injection instructs agents to produce explanation sidecars alongside changes

#### Data Model
- `Artifact` gains optional `explanation_tiers: Option<ExplanationTiers>` (summary, explanation, tags)
- `PRPackage` stores tier data; output adapters read it at render time
- Explanation sidecars are ingested at `ta pr build` time, not stored permanently in staging

### v0.2.4 — Terminology & Positioning Pass
<!-- status: done -->
**Goal**: Rename user-facing concepts for clarity. TA is an **agentic governance wrapper** — it wraps agent execution transparently, holds proposed changes at a human review checkpoint, and applies approved changes to the user's world. Terminology should work for developers and non-developers alike, and avoid VCS jargon since TA targets Perforce, SVN, document platforms, email, social media, and more.

#### Core Terminology Changes

| Old term | New term | Rationale |
|---|---|---|
| **PRPackage** | **Draft** | A draft is the package of agent work products awaiting review. Implies "complete enough to review, not final until approved." No git connotation. |
| **PRStatus** | **DraftStatus** | Follows from Draft rename. |
| **`ta pr build/view/approve/deny/apply`** | **`ta draft build/view/approve/deny/apply`** | CLI surface rename. Keep `apply` — it's VCS-neutral and universally understood. |
| **PendingReview (status)** | **Checkpoint** | The human-in-the-loop review gate where a Draft is examined for approval. |
| **staging dir / overlay** | **Virtual Workspace** | Where the agent works. Invisible to the agent. Will become lightweight/virtual (V2: reflinks/FUSE). "Staging" is git jargon; "virtual workspace" is self-explanatory. |
| **"substrate" / "layer"** | **Wrapper** | TA wraps agent execution. "Substrate" sounds like marketing; "layer" is vague; "wrapper" is literal and clear. |
| **PR (in docs/README)** | **Draft** | Everywhere user-facing text says "PR" in the TA-specific sense (not git PRs). |

#### Flow in New Terminology
```
Agent works in Virtual Workspace
  -> produces a Draft
    -> human reviews at Checkpoint
      -> Approves / Rejects each change
        -> Approved changes are Applied
```

#### Scope of Changes
- **Code**: Rename `PRPackage` -> `DraftPackage`, `PRStatus` -> `DraftStatus`, `pr_package.rs` -> `draft_package.rs`
- **CLI**: `ta draft` subcommand replaces `ta pr`. Keep `ta pr` as hidden alias for backwards compatibility during transition.
- **Docs**: README, USAGE.md, CLAUDE.md, PLAN.md — replace TA-specific "PR" with "Draft", "staging" with "virtual workspace" in user-facing text
- **Schema**: `schema/pr_package.schema.json` -> `schema/draft_package.schema.json` (or alias)
- **Internal code comments**: Update incrementally, not a big-bang rename. Internal variable names can migrate over time.

#### What Stays the Same
- `apply` — VCS-neutral, universally understood
- `artifact` — standard term for individual changed items within a Draft
- `goal` — clear, no issues
- `checkpoint` — only replaces `PendingReview` status; the concept name for the review gate
- All internal architecture (overlay, snapshot, conflict detection) — implementation names are fine; only user-facing surface changes

#### Positioning Statement (draft)
> **Trusted Autonomy** is an agentic governance wrapper. It lets AI agents work freely using their native tools in a virtual workspace, then holds their proposed changes — code commits, document edits, emails, posts — at a checkpoint for human review before anything takes effect. The human sees what the agent wants to do, approves or rejects each action, and maintains an audit trail of every decision.

#### Open Questions
- Should `DraftPackage` just be `Draft`? Shorter, but `Draft` alone is generic. `DraftPackage` parallels the current data model. Decide during implementation. **Decision**: keep `DraftPackage`
- `Checkpoint` as a status vs. a concept: currently the status enum has `PendingReview`. Rename to `AtCheckpoint`? Or keep `PendingReview` internally and use "checkpoint" only in user-facing text? **Decision**: keep `PendingReview`
- `ta draft` vs `ta review` as the subcommand? `draft` emphasizes the agent's output; `review` emphasizes the human's action. Both valid. `draft` chosen because the subcommand operates on the draft object (`build`, `view`, `apply`). **Decision**: keep `draft` 

---

## v0.3 — Review & Plan Automation *(release: tag v0.3.0-alpha)*

### v0.3.0 — Review Sessions
<!-- status: pending -->
- ReviewSession persists across CLI invocations (multi-interaction review)
- Per-artifact comment threads (stored in PR package or sidecar file)
- Supervisor agent analyzes dependency graph and warns about coupled rejections
- Discussion workflow for `?` (discuss) items
- **Resolution path**: `ta run --follow-up` on a goal with discuss items injects comment threads as structured agent context; the agent addresses each discussed artifact and the resulting PR supersedes the original (see v0.1.2)

### v0.3.1 — Plan Lifecycle Automation
<!-- status: pending -->
- Supervisor agent reads change_summary.json, validates completed work against plan
- Completing one phase auto-suggests/creates goal for next pending phase
- Plan templates for common workflows (feature, bugfix, refactor)
- `ta plan next` command to create goal for next pending phase
- Plan versioning and history

### v0.3.2 — Configurable Release Pipeline (`ta release`)
<!-- status: pending -->
A `ta release` command driven by a YAML task script (`.ta/release.yaml`). Each step is either a TA goal (agent-driven) or a shell command, with optional approval gates. Replaces `scripts/release.sh` with a composable, extensible pipeline.

- **YAML schema**: Steps with `name`, `agent` or `run`, `objective`, `output`, `requires_approval`
- **Agent steps**: Create a TA goal for the agent to execute (e.g., synthesize release notes from commits)
- **Shell steps**: Run build/test/tag commands directly
- **Commit collection**: Automatically gather commits since last tag as context for agent steps
- **Built-in pipeline**: Default release.yaml ships with the binary (version bump, verify, release notes, tag)
- **Customizable**: Users override with `.ta/release.yaml` in their project
- **Approval gates**: `requires_approval: true` pauses for human review before proceeding (e.g., before push)

---

## v0.4 — Agent Intelligence *(release: tag v0.4.0-alpha)*

### v0.4.0 — Intent-to-Access Planner
<!-- status: pending -->
- LLM "Intent-to-Policy Planner" outputs AgentSetupProposal (JSON)
- Deterministic Policy Compiler validates proposal (subset of templates, staged semantics, budgets)
- Agent setup becomes an "Agent PR" requiring approval before activation
- User goal → proposed agent roster + scoped capabilities + milestone plan

---

## v0.5 — External Connectors *(release: tag v0.5.0-alpha)*

### v0.5.0 — First External Connector
<!-- status: pending -->
Options (choose one):
- **Gmail staging**: read threads, create draft (ChangeSet), send gated by approval
- **Drive staging**: read doc, write_patch + diff preview, commit gated
- **DB staging**: write_patch as transaction log + preview, commit gated

---

## v0.6 — Sandbox Isolation *(release: tag v0.6.0-beta)*

### v0.6.0 — Sandbox Runner
<!-- status: pending -->
- OCI/gVisor sandbox for agent execution
- Allowlisted command execution (rg, fmt, test profiles)
- CWD enforcement — agents can't escape workspace
- Command transcripts hashed into audit log

---

## v0.7 — Distribution & Packaging *(release: tag v0.7.0-beta)*

### v0.7.0 — Distribution & Packaging
<!-- status: pending -->
- Developer: `cargo run` + local config + Nix
- Desktop: installer with bundled daemon, git, rg/jq
- Cloud: OCI image for daemon + connectors, ephemeral workspaces
- Web UI for review/approval (localhost → LAN → cloud)

---

## v0.8 — Events & Orchestration *(release: tag v0.8.0-beta)*

### v0.8.0 — Event System & Orchestration API
<!-- status: pending -->
> See `docs/VISION-virtual-office.md` for full vision.
- `--json` output flag on all CLI commands for programmatic consumption
- Event hook execution: call webhooks/scripts on goal + PR state transitions
- `ta events listen` command — stream JSON events for external consumers
- Stable event schema matching `docs/plugins-architecture-guidance.md` hooks
- Non-interactive approval API: token-based approve/reject (for Slack buttons, email replies)
- Foundation for notification connectors and virtual office runtime

---

## v0.9 — Notification Connectors *(release: tag v0.9.0-beta)*

### v0.9.0 — Notification Connectors
<!-- status: pending -->
- `ta-connector-notify-email`: SMTP PR summaries + reply-to-approve parsing
- `ta-connector-notify-slack`: Slack app with Block Kit PR cards + button callbacks
- `ta-connector-notify-discord`: Discord bot with embed summaries + reaction handlers
- Bidirectional: outbound notifications + inbound approval actions
- Unified config: `notification_channel` per role/goal

---

## v1.0 — Virtual Office *(release: tag v1.0.0)*

### v1.0.0 — Virtual Office Runtime
<!-- status: pending -->
> Thin orchestration layer that composes TA, Claude Flow, and notification connectors.
- Role definition schema (YAML): purpose, triggers, agent, capabilities, notification channel
- Trigger system: cron scheduler + webhook receiver + TA event listener
- Office manager daemon: reads role configs, routes triggers, calls `ta run`
- `ta office start/stop/status` CLI commands
- Role-scoped TA policies auto-generated from role capability declarations
- Integration with Claude Flow as the agent coordination backend
- Does NOT duplicate orchestration — composes existing tools with role/trigger glue