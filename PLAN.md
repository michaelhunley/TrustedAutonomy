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

## Standards & Compliance Reference

TA's architecture maps to emerging AI governance standards. Rather than bolt-on compliance, these standards inform design decisions at the phase where they naturally apply. References below indicate where TA's existing or planned capabilities satisfy a standard's requirements.

| Standard | Relevance to TA | Phase(s) |
|---|---|---|
| **ISO/IEC 42001:2023** (AI Management Systems) | Audit trail integrity (hash-chained logs), documented capability grants, human oversight records | Phase 1 (done), v0.3.3 |
| **ISO/IEC 42005:2025** (AI Impact Assessment) | Risk scoring per draft, policy decision records, impact statements in summaries | Phase 4b (done), v0.3.3 |
| **IEEE 7001-2021** (Transparency of Autonomous Systems) | Structured decision reasoning, alternatives considered, observable policy enforcement | v0.3.3, v0.4.0 |
| **IEEE 3152-2024** (Human/Machine Agency Identification) | Agent identity declarations, capability manifests, constitution references | Phase 2 (done), v0.4.0 |
| **EU AI Act Article 14** (Human Oversight) | Human-in-the-loop checkpoint, approve/reject per artifact, audit trail of decisions | Phase 3 (done), v0.3.0 (done) |
| **EU AI Act Article 50** (Transparency Obligations) | Transparent interception of external actions, human-readable action summaries | v0.5.0, v0.7.1 |
| **Singapore IMDA Agentic AI Framework** (Jan 2026) | Agent boundaries, network governance, multi-agent coordination alignment | v0.6.0, v0.7.x, v1.0 |
| **NIST AI RMF 1.0** (AI Risk Management) | Risk-proportional review, behavioral drift monitoring, escalation triggers | v0.3.3, v0.4.2 |

> **Design principle**: TA achieves compliance through architectural enforcement (staging + policy + checkpoint), not self-declaration. An agent's compliance is *verified by TA's constraints*, not *claimed by the agent*. This is stronger than transparency-only protocols like [AAP](https://github.com/mnemom/aap) — TA doesn't ask agents to declare alignment; it enforces boundaries regardless of what agents declare.

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
- **v0.2.4 update**: Added `what` field (per-target "what I did" description) alongside existing `why` (motivation). `what` populates `explanation_tiers.summary`; `why` populates `explanation_tiers.explanation`. Backward compatible — old summaries with only `why` still work via `rationale` field.

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
<!-- status: done -->
**Completed**:
- ✅ ReviewSession data model with persistent storage (review_session.rs, review_session_store.rs)
- ✅ Per-artifact comment threads integrated into Artifact model (`comments: Option<Vec<Comment>>`)
- ✅ Session state tracking (Active, Paused, Completed, Abandoned)
- ✅ Disposition counts and summary methods
- ✅ CLI review workflow: `ta draft review start/comment/next/finish/list/show`
- ✅ 50+ new unit tests (total: 258 tests across 12 crates)
- ✅ **Supervisor agent** (`crates/ta-changeset/src/supervisor.rs`): Dependency graph analysis with cycle detection, self-dependency detection, coupled rejection warnings, and broken dependency warnings. Integrated into `ta draft apply` with enhanced error/warning display (13 new tests, total: 271 tests)
- ✅ **Discussion workflow implementation**: Comment threads from discuss items are now injected into CLAUDE.md when creating follow-up goals. The `build_parent_context_section` function in `apps/ta-cli/src/commands/run.rs` includes full comment threads, explanation tiers, and agent rationale for each discussed artifact. Agents receive structured discussion history as context, enabling them to address reviewer concerns in follow-up iterations. (2 new tests, total: 273 tests)

- ✅ **Per-target summary enforcement**: At `ta draft build` time, configurable enforcement (ignore/warning/error via `[build] summary_enforcement` in `.ta/workflow.toml`) warns or errors when artifacts lack a `what` description. Lockfiles, config manifests, and docs are auto-exempt via hardcoded list. (3 new tests, total: 289 tests) *(Exemption patterns become configurable in v0.4.0; per-goal access constitutions in v0.4.3)*
- ✅ **Disposition badges in HTML output**: HTML adapter renders per-artifact disposition badges (pending/approved/rejected/discuss) with color-coded CSS classes. Added `.status.discuss` styling. (3 new tests)
- ✅ **Config bugfix**: Added `#[serde(default)]` to `WorkflowConfig.submit` field so partial `.ta/workflow.toml` files parse correctly without requiring a `[submit]` section.

### v0.3.0.1 — Consolidate `pr.rs` into `draft.rs`
<!-- status: pending -->
**Completed**:
- ✅ `pr.rs` reduced from 2205 lines to ~160 lines: thin shim that converts `PrCommands` → `DraftCommands` and delegates to `draft::execute()`
- ✅ `run.rs` updated to call `draft::DraftCommands::Build` instead of `pr::PrCommands::Build`
- ✅ `run.rs` follow-up context updated to use `draft::load_package` and `draft_package::ArtifactDisposition`
- ✅ All ~20 duplicated private functions removed from `pr.rs` (~2050 lines eliminated)
- ✅ `ta pr` remains as a hidden alias for backward compatibility
- ✅ All 278 tests passing (11 duplicate pr.rs tests removed; all functionality covered by draft.rs tests)

### v0.3.1 — Plan Lifecycle Automation
<!-- status: pending -->
**Completed** (294 tests across 12 crates):
- ✅ Supervisor `validate_against_plan()` reads change_summary.json, validates completed work against plan at `ta draft build` time (4 new tests)
- ✅ Completing one phase auto-suggests/creates goal for next pending phase (output after `ta draft apply --phase`)
- ✅ Plan parser extended to handle `### v0.X.Y` sub-phase headers in addition to `## Phase` top-level headers
- ✅ `ta plan next` command shows next pending phase and suggests `ta run` command (new CLI command)
- ✅ `ta plan validate <phase>` command shows phase status, linked goals, and latest draft summary (new CLI command)
- ✅ Plan versioning and history: status transitions recorded to `.ta/plan_history.jsonl`, viewable via `ta plan history` (new CLI command)
- ✅ Git commit message in `ta draft apply` now includes complete draft summary with per-artifact descriptions (`build_commit_message` function)
- ✅ 16 new tests: plan parsing for sub-phases (4), plan lifecycle (find_next, suggest, history — 8), supervisor plan validation (4)

### v0.3.1.1 — Configurable Plan Format Parsing
<!-- status: pending -->

**Completed** (307 tests across 12 crates):
- ✅ `PlanSchema` data model with `PhasePattern` and YAML serde support (`.ta/plan-schema.yaml`)
- ✅ `parse_plan_with_schema()` — regex-driven plan parser that replaces hardcoded parsing logic
- ✅ `parse_plan()` and `load_plan()` now delegate to schema-driven parser with default schema (full backward compatibility)
- ✅ `update_phase_status_with_schema()` — schema-aware status updates
- ✅ `PlanSchema::load_or_default()` — loads `.ta/plan-schema.yaml` or falls back to built-in default
- ✅ `ta plan init` command — auto-detects plan format, proposes schema, writes `.ta/plan-schema.yaml`
- ✅ `ta plan create` command — generates plan documents from templates (greenfield, feature, bugfix)
- ✅ `detect_schema_from_content()` — heuristic schema detection for `ta plan init`
- ✅ Bug fix: `strip_html()` in terminal adapter prevents HTML tags from leaking into terminal output (garbled `ÆpendingÅ` display)
- ✅ `regex` crate added to workspace dependencies
- ✅ 13 new tests: schema round-trip (1), schema loading (2), custom schema parsing (2), schema detection (2), template parsing (1), custom schema status update (1), custom schema load_plan (1), invalid regex handling (2), terminal HTML regression (3)

#### Problem
`plan.rs` hardcodes this project's PLAN.md format (`## v0.X`, `### v0.X.Y`, `<!-- status: -->` markers). Any other project using TA would need to adopt the same markdown conventions or nothing works. The parser should be schema-driven, not format-hardcoded.

#### Solution: `.ta/plan-schema.yaml`
Declarative config describing how to parse a project's plan document. Shipped with sensible defaults that match common markdown patterns.
```yaml
# .ta/plan-schema.yaml
source: PLAN.md                          # or ROADMAP.md, TODO.md, etc.
phase_patterns:
  - regex: "^##+ (?:v?[\\d.]+[a-z]? — |Phase \\d+ — )(.+)"
    id_capture: "version_or_phase_number"
status_marker: "<!-- status: (\\w+) -->"   # regex with capture group
statuses: [done, in_progress, pending]     # valid values
```

#### CLI
- **`ta plan init`**: Agent-guided schema extraction — reads an existing plan document, proposes a `plan-schema.yaml`, human approves. Zero effort for projects that already have a plan.
- **`ta plan create`**: Generate a new plan document from a template + schema. Templates for common workflows (feature, bugfix, greenfield).
- Refactor `parse_plan()` to read schema at runtime instead of hardcoded regexes. Existing behavior preserved as the default schema (zero-config for projects that adopt the current convention).

#### Bug fix: garbled HTML in terminal output
`ta draft view` renders `ÆpendingÅ` instead of `[pending]` — HTML `<span>` tags leaking into terminal output with encoding corruption. Fix: `strip_html()` helper in `TerminalAdapter` sanitizes all user-provided text fields before rendering. Regression test asserts terminal output contains no HTML tags.

### v0.3.1.2 — Interactive Session Orchestration
<!-- status: pending -->

#### Vision
The human orchestrates construction iteratively across multiple goal sessions — observing agent work, injecting guidance, reviewing drafts, and resuming sessions — through a unified interaction layer. This phase builds the **session interaction protocol** that underpins both the local CLI experience and the future TA web app / messaging integrations (Discord, Slack, email).

> **Design principle**: Every interaction between human and TA is a **message** on a **channel**. The CLI is one channel. A Discord thread is another. The protocol is the same — TA doesn't care where the message came from, only that it's authenticated and routed to the right session.

#### Session Interaction Protocol
The core abstraction: a `SessionChannel` trait that any frontend implements.

```rust
/// A bidirectional channel between a human and a TA-mediated agent session.
pub trait SessionChannel: Send + Sync {
    /// Display agent output to the human (streaming).
    fn emit(&self, event: SessionEvent) -> Result<()>;
    /// Receive human input (blocks until available or timeout).
    fn receive(&self, timeout: Duration) -> Result<Option<HumanInput>>;
    /// Channel identity (for audit trail).
    fn channel_id(&self) -> &str;  // "cli:tty0", "discord:thread:123", "slack:C04..."
}

pub enum SessionEvent {
    AgentOutput { stream: Stream, content: String },  // stdout/stderr
    DraftReady { draft_id: Uuid, summary: String },   // checkpoint
    GoalComplete { goal_id: Uuid },
    WaitingForInput { prompt: String },                // agent needs guidance
}

pub enum HumanInput {
    Message(String),                    // guidance injected into agent context
    Approve { draft_id: Uuid },         // inline review
    Reject { draft_id: Uuid, reason: String },
    Abort,                              // kill session
}
```

#### CLI implementation (`ta run --interactive`)
The first `SessionChannel` implementation — wraps the agent CLI with PTY capture.

- **Observable output**: Agent stdout/stderr piped through TA, displayed to human, captured for audit.
- **Session wrapping**: TA launches agent CLI inside a session envelope. Agent doesn't know TA exists. TA controls environment injection and exit.
- **Human interrogation**: stdin interleaving lets human inject guidance. Agent responds using existing context — no token cost for re-learning state.
- **Context preservation on resume**: Uses agent-framework-native resume (Claude `--resume`, Codex session files) when available. Falls back to CLAUDE.md context injection.
- **Per-agent config**: `agents/<name>.yaml` gains `interactive` block:
```yaml
interactive:
  launch_cmd: "claude --resume {session_id}"
  output_capture: "pty"              # pty, pipe, or log
  allow_human_input: true
  auto_exit_on: "idle_timeout: 300s" # or "goal_complete"
```

#### MCP integration surface (for messaging channels)
The `SessionChannel` trait is designed so that messaging platform integrations are thin adapters, not new features. Each maps platform primitives to `SessionEvent` / `HumanInput`:

| Platform | `emit()` | `receive()` | Session identity |
|----------|----------|-------------|-----------------|
| CLI (v0.3.1.2) | PTY stdout | stdin | `cli:{tty}` |
| Discord (future) | Thread message | Thread reply | `discord:{thread_id}` |
| Slack (future) | Channel message | Thread reply | `slack:{channel}:{ts}` |
| Email (future) | Reply email | Incoming email | `email:{thread_id}` |
| Web app (future) | WebSocket push | WebSocket message | `web:{session_id}` |

Each adapter is ~100-200 lines: authenticate, map to `SessionChannel`, route to the correct TA session. All governance (draft review, audit, policy) is handled by TA core — the channel just carries messages.

#### Stepping stones to the TA app
This phase deliberately builds the protocol layer that the TA local/web app will consume:
- **Session list + status**: `ta session list` shows active sessions across all channels. Web app renders the same data.
- **Draft review inline**: Human can approve/reject drafts from within the session (any channel), not just via separate `ta draft approve` commands.
- **Multi-session orchestration**: Human can have multiple active sessions (different goals/agents) and switch between them. Web app shows them as tabs; Discord shows them as threads.
- Relates to v0.4.1 (macro goals) — interactive sessions are the human-facing complement to the agent-facing MCP tools in macro goal mode.

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

### v0.3.3 — Decision Observability & Reasoning Capture
<!-- status: pending -->
**Goal**: Make every decision in the TA pipeline observable — not just *what happened*, but *what was considered and why*. Foundation for drift detection (v0.4.2) and compliance reporting (ISO 42001, IEEE 7001).

> **Research note**: Evaluated [AAP](https://github.com/mnemom/aap) (Agent Alignment Protocol) for this role. AAP provides transparency through self-declared alignment cards and traced decisions, but is a Python/TypeScript decorator-based SDK that can't instrument external agents (Claude Code, Codex). TA's approach is stronger: enforce constraints architecturally, then capture the reasoning of TA's own decision pipeline. The *agent's* internal reasoning is captured via `change_summary.json`; TA's *governance* reasoning is captured here.

#### Data Model: `DecisionReasoning` in `ta-audit`
```rust
pub struct DecisionReasoning {
    /// What alternatives were considered.
    pub alternatives: Vec<Alternative>,
    /// Why this outcome was selected.
    pub rationale: String,
    /// Values/principles that informed the decision.
    pub applied_principles: Vec<String>,
}

pub struct Alternative {
    pub description: String,
    pub score: Option<f64>,
    pub rejected_reason: String,
}
```
Extends `AuditEvent` with an optional `reasoning: Option<DecisionReasoning>` field. Backward-compatible — existing events without reasoning still deserialize.

#### Integration Points
- **PolicyEngine.evaluate()**: Log which grants were checked, which matched, why allow/deny/require-approval. Captures the full capability evaluation chain, not just the final verdict.
- **Supervisor.validate()**: Log dependency graph analysis — which warnings were generated, which artifacts triggered them, what the graph structure looked like.
- **Human review decisions**: Extend ReviewSession comments with structured `reasoning` field — reviewer can explain *why* they approved/rejected, not just leave a text comment.
- **`ta draft build`**: Log why each artifact was classified (Add/Modify/Delete), what diff heuristics were applied.
- **`ta draft apply`**: Log conflict detection reasoning — which files conflicted, which were phantom (auto-resolved), what resolution strategy was applied and why.

#### Agent-Side: Extend `change_summary.json`
Add optional `alternatives_considered` field per change entry:
```json
{
  "path": "src/auth.rs",
  "what": "Migrated to JWT",
  "why": "Session tokens don't scale to multiple servers",
  "alternatives_considered": [
    { "description": "Sticky sessions", "rejected_reason": "Couples auth to infrastructure" },
    { "description": "Redis session store", "rejected_reason": "Adds operational dependency" }
  ]
}
```
Agents that support it get richer review context; agents that don't still work fine (field is optional).

#### CLI
- `ta audit show <goal-id>` — display decision trail for a goal with reasoning
- `ta audit export <goal-id> --format json` — structured export for compliance reporting

#### Standards Alignment
- **ISO/IEC 42001**: Documented decision processes with rationale (Annex A control A.6.2.3)
- **IEEE 7001**: Transparent autonomous systems — decisions are explainable to stakeholders
- **NIST AI RMF**: MAP 1.1 (intended purpose documentation), GOVERN 1.3 (decision documentation)

---

## v0.4 — Agent Intelligence *(release: tag v0.4.0-alpha)*

### v0.4.0 — Intent-to-Access Planner & Agent Alignment Profiles
<!-- status: pending -->
- LLM "Intent-to-Policy Planner" outputs AgentSetupProposal (JSON)
- Deterministic Policy Compiler validates proposal (subset of templates, staged semantics, budgets)
- Agent setup becomes an "Agent PR" requiring approval before activation
- User goal → proposed agent roster + scoped capabilities + milestone plan
- Agent setup evaluates how to run the agents efficiently at lowest cost (model selection, prompt caching, etc) and advises tradeoffs with human opt in where appropriate

#### Agent Alignment Profiles (extends YAML agent configs)
Inspired by [AAP alignment cards](https://github.com/mnemom/aap) but *enforced* rather than self-declared. Each agent's YAML config gains a structured `alignment` block:
```yaml
# agents/claude-code.yaml
alignment:
  principal: "project-owner"           # Who this agent serves
  autonomy_envelope:
    bounded_actions: ["fs_read", "fs_write", "exec: cargo test"]
    escalation_triggers: ["new_dependency", "security_sensitive", "breaking_change"]
    forbidden_actions: ["network_external", "credential_access"]
  constitution: "default-v1"           # Reference to enforcement rules
  coordination:
    allowed_collaborators: ["codex", "claude-flow"]
    shared_resources: ["src/**", "tests/**"]
```
- **Key difference from AAP**: These declarations are *compiled into CapabilityManifest grants* by the Policy Compiler. An agent declaring `forbidden_actions: ["network_external"]` gets a manifest with no network grants — it's not a promise, it's a constraint.
- **Coordination block**: Used by v0.4.1 macro goals and v1.0 virtual office to determine which agents can co-operate on shared resources.
- **Configurable summary exemption patterns**: Replace hardcoded `is_auto_summary_exempt()` with a `.gitignore`-style pattern file (e.g., `.ta/summary-exempt`), seeded by workflow templates and refined by the supervisor agent based on project structure analysis. Patterns would match against `fs://workspace/` URIs. (see v0.3.0 per-target summary enforcement)

#### Standards Alignment
- **IEEE 3152-2024**: Agent identity + capability declarations satisfy human/machine agency identification
- **ISO/IEC 42001**: Agent setup proposals + human approval = documented AI lifecycle management
- **NIST AI RMF GOVERN 1.1**: Defined roles and responsibilities for each agent in the system

### v0.4.1 — Macro Goals & Inner-Loop Iteration
<!-- status: pending -->
**Goal**: Let agents stay in a single session, decompose work into sub-goals, submit drafts, and iterate — without exiting and restarting `ta run` each time.

> **Core insight**: Currently each `ta run` session is one goal → one draft → exit. For complex tasks (e.g., "build Trusted Autonomy v0.5"), the agent must exit, the human must approve, then another `ta run` starts. Macro goals keep the agent in-session while maintaining governance at every checkpoint.

#### MCP Tools Exposed to Agent (Passthrough Model)
TA injects MCP tools that mirror the CLI structure — same commands, same arguments:
- **`ta_draft`** `action: build|submit|status|list` — package, submit, and query drafts
- **`ta_goal`** `action: start|status` — create sub-goals, check status
- **`ta_plan`** `action: read|update` — read plan progress, propose updates

> **Design**: Passthrough mirrors the CLI (`ta draft build` = `ta_draft { action: "build" }`). No separate tool per subcommand — agents learn one pattern, new CLI commands are immediately available as MCP actions. Arguments map 1:1 to CLI flags.

#### Security Boundaries
- Agent **CAN**: propose sub-goals, build drafts, submit for review, read plan status
- Agent **CANNOT**: approve its own drafts, apply changes, bypass checkpoints, modify policies
- Every sub-goal draft goes through the same human review gate as a regular draft
- Agent sees approval/rejection results and can iterate (revise and resubmit)
- `ta_draft { action: "submit" }` blocks until human responds (blocking mode) — agent cannot self-approve

#### Execution Modes
- **Blocking** (default): Agent submits draft, blocks until human responds. Safest — human reviews each step.
- **Optimistic** (future): Agent continues to next sub-goal while draft is pending. Human reviews asynchronously. Faster but requires rollback capability if earlier draft is rejected.
- **Hybrid** (future): Agent marks sub-goals as blocking or non-blocking based on risk. High-risk changes block; low-risk ones proceed optimistically.

#### CLI
- `ta run "Build v0.5" --source . --macro` — starts a macro goal session
- Agent receives MCP tools for inner-loop iteration alongside standard workspace tools
- `ta goal status <id>` shows sub-goal tree with approval status

#### Integration
- Sub-goals inherit the macro goal's plan phase, source dir, and agent config
- Each sub-goal draft appears in `ta draft list` as a child of the macro goal
- PLAN.md updates proposed via `ta_plan_update` are held at checkpoint (agent proposes, human approves)
- Works with existing follow-up goal mechanism — macro goals are the automated version of `--follow-up`

### v0.4.2 — Behavioral Drift Detection
<!-- status: pending -->
**Goal**: Detect when an agent's behavior patterns diverge from its historical baseline or declared alignment profile. Uses the decision reasoning data from v0.3.3 and alignment profiles from v0.4.0.

> **Why built-in, not AAP**: AAP's drift detection (`aap drift`) compares traces against self-declared alignment cards. TA's approach compares *actual enforced behavior* across goals — what resources an agent accesses, what kinds of changes it makes, how often it triggers escalation, what rejection rate it has. This is empirical, not declarative.

#### Drift Signals (computed from `ta-audit` event log)
- **Resource scope drift**: Agent accessing files/URIs outside its historical pattern (e.g., suddenly modifying CI configs when it normally only touches `src/`)
- **Escalation frequency change**: Significant increase/decrease in policy escalations may indicate changed behavior or stale manifest
- **Rejection rate drift**: If an agent's drafts start getting rejected more often, something changed
- **Change volume anomaly**: Unexpectedly large or small diffs compared to historical baseline
- **Dependency pattern shift**: Agent introducing new external dependencies at unusual rates

#### CLI
- `ta audit drift <agent-id>` — show drift report comparing recent N goals against historical baseline
- `ta audit drift --all` — drift summary across all agents
- `ta audit baseline <agent-id>` — compute and store behavioral baseline from historical data
- Warning integration: `ta draft build` optionally warns if current goal's behavior diverges from baseline

#### Data Model
```rust
pub struct BehavioralBaseline {
    pub agent_id: String,
    pub computed_at: DateTime<Utc>,
    pub goal_count: usize,      // Number of goals in baseline
    pub resource_patterns: Vec<String>,  // Typical URI patterns accessed
    pub avg_artifact_count: f64,
    pub avg_risk_score: f64,
    pub escalation_rate: f64,   // Fraction of actions triggering escalation
    pub rejection_rate: f64,    // Fraction of artifacts rejected by reviewers
}
```

#### Standards Alignment
- **NIST AI RMF MEASURE 2.6**: Monitoring AI system behavior for drift from intended purpose
- **ISO/IEC 42001 A.6.2.6**: Performance monitoring and measurement of AI systems
- **EU AI Act Article 9**: Risk management system with continuous monitoring

### v0.4.3 — Access Constitutions
<!-- status: pending -->
**Goal**: Human-authorable or TA-agent-generated "access constitutions" that declare what URIs an agent should need to access to complete a given goal. Serves as a pre-declared intent contract — any deviation from the constitution is a behavioral drift signal.

> **Relationship to v0.4.0**: Alignment profiles describe an agent's *general* capability envelope. Access constitutions are *per-goal* — scoped to a specific task. An agent aligned for `src/**` access (v0.4.0 profile) might have a goal-specific constitution limiting it to `src/commands/draft.rs` and `crates/ta-submit/src/config.rs`.

- **Authoring**: Human writes constitution directly, or TA supervisor agent proposes one based on the goal objective + plan phase + historical access patterns
- **Format**: URI-scoped pattern list with intent annotations, stored alongside goal metadata
```yaml
# .ta/constitutions/goal-<id>.yaml
access:
  - pattern: "fs://workspace/src/commands/draft.rs"
    intent: "Add summary enforcement logic"
  - pattern: "fs://workspace/crates/ta-submit/src/config.rs"
    intent: "Add BuildConfig struct"
  - pattern: "fs://workspace/crates/ta-changeset/src/output_adapters/html.rs"
    intent: "Add disposition badges"
```
- **Enforcement**: At `ta draft build` time, compare actual artifacts against declared access constitution. Undeclared access triggers a warning (or error in strict mode).
- **Drift integration** (depends on v0.4.2): Constitution violations feed into the behavioral drift detection pipeline as a high-signal indicator.

#### Standards Alignment
- **IEEE 3152-2024**: Pre-declared intent satisfies transparency requirements for autonomous system actions
- **NIST AI RMF GOVERN 1.4**: Documented processes for mapping AI system behavior to intended purpose
- **EU AI Act Article 14**: Human oversight mechanism — constitution is a reviewable, pre-approved scope of action

---

## v0.5 — MCP Interception & External Actions *(release: tag v0.5.0-alpha)*

> **Architecture shift**: Instead of building custom connectors per service (Gmail, Drive, etc.),
> TA intercepts MCP tool calls that represent state-changing actions. MCP servers handle the
> integration. TA handles the governance. Same pattern as filesystem: hold changes at a
> checkpoint, replay on apply.

### v0.5.0 — MCP Tool Call Interception
<!-- status: pending -->
**Core**: Intercept outbound MCP tool calls that change external state. Hold them in the draft as pending actions. Replay on apply.

- **MCP action capture**: When an agent calls an MCP tool (e.g., `gmail_send`, `slack_post`, `tweet_create`), TA intercepts the call, records the tool name + arguments + timestamp in the draft as a `PendingAction`
- **Action classification**: Read-only calls (search, list, get) pass through immediately. State-changing calls (send, post, create, update, delete) are captured and held
- **Draft action display**: `ta draft view` shows pending actions alongside file artifacts — "Gmail: send to alice@example.com, subject: Q3 Report" with full payload available at `--detail full`
- **Selective approval**: Same `--approve`/`--reject` pattern works for actions. URI scheme distinguishes them: `mcp://gmail/send`, `mcp://slack/post_message`, etc.
- **Apply = replay**: `ta draft apply` replays approved MCP calls against the live MCP server. Failed replays are reported with retry option.
- **Data model**: `DraftPackage.changes` gains `pending_actions: Vec<PendingAction>` alongside existing `artifacts` and `patch_sets`

```rust
pub struct PendingAction {
    pub action_uri: String,           // mcp://server/tool_name
    pub tool_name: String,            // Original MCP tool name
    pub arguments: serde_json::Value, // Captured arguments
    pub captured_at: DateTime<Utc>,
    pub disposition: ArtifactDisposition, // Same approval model as file artifacts
    pub summary: String,              // Human-readable description (LLM-generated)
    pub reversible: bool,             // Can this action be undone? (send email = no, create draft = yes)
}
```

#### First integration targets (via existing MCP servers)
- **Gmail**: Send email, create draft, reply — using Google MCP server
- **Slack/Discord**: Post message, react — using respective MCP servers
- **Social media**: Post, schedule — using platform MCP servers
- **Google Drive/Docs**: Create, update, share — using Google MCP server
- **Database**: INSERT/UPDATE/DELETE — using DB MCP servers

#### What TA does NOT build
- No Gmail API client. No Slack bot. No Twitter SDK. The MCP servers handle all service-specific logic.
- TA only adds: interception, capture, display, approval, replay. The governance wrapper.

#### Standards Alignment
- **EU AI Act Article 50**: Transparency obligations — humans see exactly what the agent wants to do externally before it happens. MCP action summaries satisfy the "inform the natural person" requirement.
- **ISO/IEC 42001 A.10.3**: Third-party AI component management — MCP servers are third-party components; TA's interception provides the governance wrapper around them.

---

## v0.6 — Sandbox Isolation *(release: tag v0.6.0-beta)*

### v0.6.0 — Sandbox Runner
<!-- status: pending -->
- OCI/gVisor sandbox for agent execution
- Allowlisted command execution (rg, fmt, test profiles)
- CWD enforcement — agents can't escape virtual workspace
- Command transcripts hashed into audit log
- Network access policy: allow/deny per-domain (foundation for v0.7 network layer)

> **Standards**: Sandbox isolation directly addresses **Singapore IMDA Agentic AI Framework** (agent boundary enforcement) and **NIST AI RMF GOVERN 1.4** (processes for AI risk management including containment). Command transcripts hashed into audit log satisfy **ISO/IEC 42001** provenance requirements.

---

## v0.7 — Network Abstraction Layer *(release: tag v0.7.0-beta)*

> **Separate agent protocol layer**: The network intercept capability may be packaged as a
> standalone protocol that TA uses but others can adopt independently. Think modern Wireshark
> with LLM interpretation — understands what traffic means, not just what bytes flow.

### v0.7.0 — Research: Existing Network Intercept Projects
<!-- status: pending -->
**Goal**: Survey existing tools before building. Identify what to adopt vs. build.

- **Survey targets**:
  - mitmproxy / mitmproxy-rs — transparent HTTPS interception with Python/Rust API
  - Wireshark/tshark dissectors — protocol parsing at scale
  - Envoy/Istio sidecar proxy — service mesh traffic interception patterns
  - eBPF-based tools (bpftrace, Cilium) — kernel-level packet observation without proxy
  - Burp Suite / ZAP — security-focused HTTP intercept with plugin systems
  - OpenTelemetry — distributed tracing as a traffic observation model
  - pcap/npcap libraries — raw packet capture
- **Evaluation criteria**:
  - Can it run as a transparent proxy for a sandboxed process? (v0.6 integration)
  - Does it handle TLS interception with a local CA?
  - Can we get structured data (method, URL, headers, body) not just raw bytes?
  - Rust/C FFI available? Or do we shell out?
  - License compatibility (Apache-2.0 / MIT preferred)
- **Output**: Decision document — what to adopt, what to wrap, what to build

### v0.7.1 — Network Traffic Capture & Governance
<!-- status: pending -->
**Core**: Transparent proxy that captures network traffic from agent processes, classifies it, and holds state-changing requests at a checkpoint.

- **Capture layer**: Transparent proxy (likely mitmproxy-based or custom Rust proxy) that agent traffic routes through
- **Traffic classification**: LLM-assisted categorization of requests:
  - Read-only (GET, search queries) → pass through, log for audit
  - State-changing (POST, PUT, DELETE, form submissions) → capture and hold in draft
  - Sensitive (auth tokens, PII, credentials) → flag for review, never auto-approve
- **AI summary**: Each captured request gets an LLM-generated plain-English description: "Sending email to 3 recipients with Q3 financial report attached" instead of raw HTTP POST body
- **Draft integration**: Captured network actions appear in `ta draft view` alongside MCP actions and file changes. URI scheme: `net://api.gmail.com/POST/send`
- **Replay on apply**: Approved network requests are replayed. Handles auth token refresh, idempotency keys, retry logic.
- **Fallback for non-MCP agents**: Any agent that makes direct HTTP calls (no MCP) still gets governance via network capture. MCP interception (v0.5) is preferred; network capture is the safety net.

### v0.7.2 — LLM Traffic Intelligence
<!-- status: pending -->
**Goal**: The network layer becomes an expert on traffic patterns — learns from public data and community contributions.

- **Protocol understanding**: Train/fine-tune models on common API patterns (REST, GraphQL, gRPC) so summaries are accurate and actionable
- **Security intelligence**: Integrate public vulnerability databases (CVE, NVD), known-bad endpoints, credential leak patterns
- **Anomaly detection**: Flag unusual traffic — agent calling an API it's never called before, unexpected data exfiltration patterns, credential stuffing
- **Community traffic patterns**: Opt-in sharing of anonymized traffic signatures (not payloads) — "Gmail send via OAuth looks like X" — so new users get expert-level classification immediately
- **Training pipeline**: Each approved/rejected traffic decision feeds back into the classifier. Over time, TA learns what each user considers safe vs. risky.

### v0.7.3 — Standalone Agent Protocol Layer (packaging decision)
<!-- status: pending -->
**Decision point**: Package the network intercept + LLM intelligence as:
- (a) A built-in TA module (current path)
- (b) A standalone protocol/library that TA depends on but others can use independently
- (c) Both — library with TA integration as the reference implementation

Criteria: community interest, standalone utility, maintenance burden. If (b) or (c), define the protocol spec and publish separately.

### v0.7.4 — Community Memory
<!-- status: pending -->
**Goal**: Shared knowledge base where users and agents contribute solutions to unknowns. When someone solves a problem (integration quirk, error resolution, workflow pattern), that solution becomes available to the community.

**Why after v0.7.2**: Community memory is most valuable when it includes network/traffic pattern intelligence — "Gmail OAuth flow looks like X", "this API returns 429 after Y requests". Without the network intelligence layer, community memory would be limited to code-level patterns. After v0.7.2, community contributions can include traffic signatures, protocol quirks, and security patterns that make the shared knowledge base substantially richer.

- **Memory schema**: Problem → context → solution → confidence score → contributor
- **Local-first**: Each TA instance maintains its own memory store (existing `ta-audit` append-only log pattern)
- **Opt-in sharing**: Users can publish solved unknowns to a community registry (anonymized by default)
- **Agent-accessible**: MCP tool `ta_memory_search` lets agents query community solutions during goal execution
- **Feedback loop**: Solutions that work get upvoted (automatically, via "did applying this fix the issue?"); stale/wrong solutions decay
- **Integration with goal context**: CLAUDE.md injection can include relevant community solutions for the current task
- **Community traffic patterns**: Anonymized traffic signatures from v0.7.2 — "Gmail send via OAuth", "Stripe API charge" — so new users get expert-level classification immediately
- **Not a chatbot knowledge base** — focused on actionable problem→solution pairs with provenance

---

## v0.8 — Events & Orchestration *(release: tag v0.8.0-beta)*

### v0.8.0 — Event System & Orchestration API
<!-- status: pending -->
> See `docs/VISION-virtual-office.md` for full vision.
- `--json` output flag on all CLI commands for programmatic consumption
- Event hook execution: call webhooks/scripts on goal + draft state transitions
- `ta events listen` command — stream JSON events for external consumers
- Stable event schema matching `docs/plugins-architecture-guidance.md` hooks
- Non-interactive approval API: token-based approve/reject (for Slack buttons, email replies)
- Foundation for notification connectors and virtual office runtime
- **Compliance event export**: Structured event stream enables external compliance dashboards. Events carry decision reasoning (v0.3.3), drift alerts (v0.4.2), and policy decisions — sufficient for ISO/IEC 42001 continuous monitoring and EU AI Act record-keeping obligations.

---

## v0.9 — Distribution & Packaging *(release: tag v0.9.0-beta)*

### v0.9.0 — Distribution & Packaging
<!-- status: pending -->
- Developer: `cargo run` + local config + Nix
- Desktop: installer with bundled daemon, git, rg/jq
- Cloud: OCI image for daemon + MCP servers, ephemeral virtual workspaces
- Web UI for review/approval (localhost → LAN → cloud)

---

## v0.10 — Notification Channels *(release: tag v0.10.0-beta)*

### v0.10.0 — Notification Channels
<!-- status: pending -->
> Leverages MCP interception (v0.5) — TA doesn't build Slack/email clients.
> Instead, MCP servers handle delivery. TA adds bidirectional approval flows.
- Outbound: Draft review summaries sent via MCP to email/Slack/Discord
- Inbound: Approval actions received via MCP (reply-to-approve email, Slack button callback)
- Unified config: `notification_channel` per role/goal in `.ta/workflow.toml`
- Bidirectional: outbound notifications + inbound approval actions

---

## v1.0 — Virtual Office *(release: tag v1.0.0)*

### v1.0.0 — Virtual Office Runtime
<!-- status: pending -->
> Thin orchestration layer that composes TA, Claude Flow, and MCP servers.
- Role definition schema (YAML): purpose, triggers, agent, capabilities, notification channel
- Trigger system: cron scheduler + webhook receiver + TA event listener
- Office manager daemon: reads role configs, routes triggers, calls `ta run`
- `ta office start/stop/status` CLI commands
- Role-scoped TA policies auto-generated from role capability declarations
- Integration with Claude Flow as the agent coordination backend
- Network governance (v0.7) active by default for all agent roles
- Community memory (v0.7.4) shared across office roles — one role's solutions available to all
- Does NOT duplicate orchestration — composes existing tools with role/trigger glue
- **Multi-agent alignment verification**: Before agents co-operate on shared resources, TA verifies alignment profile compatibility (v0.4.0 profiles). Checks: overlapping resource grants, compatible escalation policies, no conflicting forbidden actions. Conceptually similar to [AAP value coherence handshake](https://github.com/mnemom/aap) but enforced — incompatible agents are blocked from shared-resource goals, not just warned.
- **Compliance dashboard**: Aggregate decision reasoning (v0.3.3), drift reports (v0.4.2), policy decisions, and approval records into a per-role compliance view. Exportable as ISO/IEC 42001 evidence package for audit.

> **Standards**: Virtual office with defined roles, capability boundaries, human oversight at checkpoints, and continuous drift monitoring satisfies the end-to-end requirements of **ISO/IEC 42001** (AI Management Systems), **EU AI Act** (high-risk AI system requirements including Articles 9, 14, and 50), and **Singapore IMDA Agentic AI Framework** (agent governance for multi-agent systems).