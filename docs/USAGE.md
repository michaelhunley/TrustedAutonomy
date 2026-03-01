# Trusted Autonomy — Usage Guide

**Version**: v0.4.2-alpha

Complete guide to using Trusted Autonomy for safe, reviewable AI agent workflows.

---

## Table of Contents

1. [Installation](#installation)
2. [Quick Start](#quick-start)
3. [Core Workflow](#core-workflow)
4. [Configuration](#configuration)
5. [Agent Configuration](#agent-configuration)
6. [PR Review & Approval](#pr-review--approval)
7. [Review Sessions](#review-sessions)
8. [Interactive Sessions](#interactive-sessions)
9. [Macro Goals & Inner-Loop Iteration](#macro-goals--inner-loop-iteration)
10. [Interactive Terminal Sessions](#interactive-terminal-sessions) — detailed workflow for v0.4.1.1
11. [External Diff Handlers](#external-diff-handlers)
12. [Git Integration](#git-integration)
13. [Advanced Workflows](#advanced-workflows)
14. [Claude Flow Optimization](#claude-flow-optimization)
15. [Troubleshooting](#troubleshooting)

---

## Installation

### Binary download (macOS / Linux)

Download the latest release from the [Releases page](https://github.com/trustedautonomy/ta/releases):

```bash
# macOS (Apple Silicon)
curl -LO https://github.com/trustedautonomy/ta/releases/latest/download/ta-aarch64-apple-darwin.tar.gz
tar xzf ta-aarch64-apple-darwin.tar.gz
sudo mv ta /usr/local/bin/

# macOS (Intel)
curl -LO https://github.com/trustedautonomy/ta/releases/latest/download/ta-x86_64-apple-darwin.tar.gz
tar xzf ta-x86_64-apple-darwin.tar.gz
sudo mv ta /usr/local/bin/

# Linux (x86_64)
curl -LO https://github.com/trustedautonomy/ta/releases/latest/download/ta-x86_64-unknown-linux-musl.tar.gz
tar xzf ta-x86_64-unknown-linux-musl.tar.gz
sudo mv ta /usr/local/bin/

# Linux (ARM64 — Raspberry Pi, AWS Graviton, etc.)
curl -LO https://github.com/trustedautonomy/ta/releases/latest/download/ta-aarch64-unknown-linux-musl.tar.gz
tar xzf ta-aarch64-unknown-linux-musl.tar.gz
sudo mv ta /usr/local/bin/
```

### Windows (via WSL2)

There is no native Windows build at this time. Windows users should use [WSL2](https://learn.microsoft.com/en-us/windows/wsl/install) with the Linux binary:

```bash
# 1. Install WSL2 if you haven't already (run in PowerShell as Administrator)
wsl --install

# 2. Inside your WSL2 terminal, download the Linux binary
curl -LO https://github.com/trustedautonomy/ta/releases/latest/download/ta-x86_64-unknown-linux-musl.tar.gz
tar xzf ta-x86_64-unknown-linux-musl.tar.gz
sudo mv ta /usr/local/bin/
```

> **Note**: TA works fully inside WSL2 — filesystem access, git integration, and agent frameworks all function normally. Your Windows files are accessible at `/mnt/c/`. Native Windows support is planned for a future release.

### From crates.io

```bash
cargo install ta-cli
```

### From source

```bash
git clone https://github.com/trustedautonomy/ta
cd ta
cargo build --release -p ta-cli
# Binary at target/release/ta
```

### Verify installation

```bash
ta --version
# ta 0.2.2-alpha (commit-hash date)
```

---

## Quick Start

### Run Your First Goal

```bash
# Start an agent-mediated goal
ta run "Add README badge for build status" --source .

# The agent works in a staging copy
# When done, review the changes
ta pr view <pr-id>

# Approve and apply
ta pr approve <pr-id>
ta pr apply <pr-id>
```

That's it! The agent worked safely in a staging environment, you reviewed the changes, and only then were they applied to your source.

---

## Core Workflow

### Step-by-Step Process

```bash
# 1. Start a goal (creates staging copy)
ta goal start "Fix bug in auth middleware" --source /path/to/project

# 2. Launch agent in staging environment
ta adapter launch claude-code --goal <goal-id>

# Agent works freely in staging copy at ~/.ta/staging/<goal-id>/
# All changes are isolated from your source

# 3. Build PR package from staging changes
ta pr build --latest

# 4. Review changes
ta pr view <pr-id>
# Shows: summary, file list, per-file diffs, agent rationale, dependencies

# 5. Approve
ta pr approve <pr-id>

# 6. Apply to source
ta pr apply <pr-id>
# Optional: --git-commit to create a git commit automatically
```

### All-in-One Command

```bash
# Combines: goal start → agent launch → pr build
ta run "Task description" --source .

# When agent exits, PR is auto-built
# Then review and apply as usual
```

---

## Configuration

### Workflow Configuration (`.ta/workflow.toml`)

Create `.ta/workflow.toml` in your project root to configure automation:

```toml
[submit]
adapter = "git"              # or "none"
auto_commit = true           # auto-commit on ta pr apply
auto_push = false            # auto-push after commit
auto_review = false          # auto-create PR after push

[submit.git]
branch_prefix = "ta/"        # branch naming: ta/goal-title
target_branch = "main"       # base branch for PRs
merge_strategy = "squash"    # squash | merge | rebase
remote = "origin"            # git remote name

[diff]
open_external = true         # open files in external apps by default
handlers_file = ".ta/diff-handlers.toml"  # optional path override
```

**Settings Priority**:
1. CLI flags (`--git-commit`, `--open-external`)
2. `.ta/workflow.toml` settings
3. Defaults (no commit, open external enabled)

### Diff Handlers (`.ta/diff-handlers.toml`)

Configure external apps for viewing non-text files:

```toml
# Unreal Engine assets
[[handler]]
pattern = "*.uasset"
command = "UnrealEditor"
args = ["{file}"]
description = "Unreal Engine asset"

# Images (macOS)
[[handler]]
pattern = "*.png"
command = "open"
args = ["-a", "Preview", "{file}"]
description = "PNG image"

# Blender files
[[handler]]
pattern = "*.blend"
command = "blender"
args = ["{file}"]
description = "Blender scene"

# Deep paths with ** glob
[[handler]]
pattern = "Assets/**/*.unity"
command = "Unity"
args = ["-projectPath", ".", "-openFile", "{file}"]
description = "Unity scene"
```

**Pattern Syntax**:
- `*` — matches within a directory
- `**` — matches recursively
- `{file}` — replaced with absolute path to staged file

---

## PR Review & Approval

### Approve vs Apply

TA separates **approval** (a human judgment) from **application** (file copy + commit). This is intentional:

- **Audit separation** — the approval event records *who* reviewed and *when*, distinct from when changes were applied. Important for compliance and traceability.
- **Selective apply** — approve the whole draft, then apply with `--approve src/** --reject tests/**` to control which files land.
- **Conflict check** — approve first, then run apply which detects conflicts with changes made since the goal started. If conflicts exist, you can resolve before committing.
- **Batch workflow** — review and approve several drafts, then apply them in dependency order.
- **Team handoff (future)** — one person reviews and approves, another applies at the right time.

For simple single-user workflows, you can combine them: `ta draft apply <id>` works on PendingReview drafts directly (skipping explicit approve).

### View PR Details

```bash
# Full review (summary + diffs)
ta pr view <pr-id>

# Summary only (no diffs)
ta pr view <pr-id> --summary

# Single file (opens in external handler if configured)
ta pr view <pr-id> --file src/main.rs

# Force inline diff (ignore handlers)
ta pr view <pr-id> --file image.png --no-open-external
```

### Selective Approval

Approve, reject, or discuss individual files:

```bash
# Approve only source files, reject config changes
ta pr apply <pr-id> \
  --approve "src/**" \
  --reject "config.toml" \
  --discuss "README.md"

# Special values
ta pr apply <pr-id> --approve "all"          # approve everything
ta pr apply <pr-id> --approve "src/**" --reject "rest"  # reject unmatched
```

**Dependency Validation**: TA warns if you approve file A that depends on rejected file B.

### Follow-Up Goals

Fix issues discovered during review:

```bash
# Start a follow-up goal (inherits context from parent)
ta run "Fix clippy warnings from review" --follow-up

# With detailed context
ta run --follow-up --objective-file review-notes.md --source .
```

**Staging Reuse (v0.4.1.2)**: When the parent goal's staging directory still exists, `--follow-up` prompts to reuse it. Choosing "yes" (the default) means iterative work accumulates into a single unified draft — no disconnected packages.

- **Extend** (default): Reuses parent staging. `ta draft build` produces a unified diff superseding the previous draft.
- **Standalone**: Declines the prompt (or `follow_up.default_mode = "standalone"` in `.ta/workflow.toml`). Creates a fresh copy — both drafts remain independently reviewable.

```toml
# .ta/workflow.toml — follow-up behavior
[follow_up]
default_mode = "extend"       # "extend" or "standalone"
auto_supersede = true          # auto-supersede parent draft when extending
rebase_on_apply = true         # re-snapshot source for sequential applies
```

---

## Review Sessions

**⭐ NEW in v0.3.0**: Multi-interaction review workflows with persistent sessions and per-artifact comments.

### Overview

Review Sessions enable you to:
- **Review draft packages across multiple CLI invocations** — pause and resume at any time
- **Add comments to specific artifacts** — provide structured feedback with markdown support
- **Track your progress** — automatically remember which artifacts you've reviewed
- **Collaborate** — comment threads support multiple reviewers and agents

### Data Model

Review sessions persist in `~/.ta/review_sessions/<session-id>.json` and track:
- **Session metadata**: ID, reviewer identity, created/updated timestamps, state (Active/Paused/Completed)
- **Per-artifact reviews**: Comments, dispositions (Approved/Rejected/Discuss/Pending), review timestamps
- **Current focus**: Which artifact you're examining (for "next" navigation)
- **Session notes**: General observations not tied to specific artifacts

### Comment Threads

Each artifact can have a comment thread with multiple comments from:
- **Human reviewers** — your feedback during review
- **Agents** — responses in follow-up workflows
- **Other team members** — collaborative review

Comments support markdown formatting for rich feedback.

### CLI Commands

```bash
# Start a new review session for a draft package
ta draft review start <draft-id> [--reviewer <name>]

# Add a comment to a specific artifact
ta draft review comment <artifact-uri> "Your feedback here"

# Move to the next artifact that hasn't been reviewed
ta draft review next

# Set disposition for current artifact
ta draft review approve <artifact-uri>
ta draft review reject <artifact-uri> --reason "Needs refactoring"
ta draft review discuss <artifact-uri> --comment "Questions about approach"

# Add session-level notes (not tied to specific artifacts)
ta draft review note "Overall: well-structured changes"

# List all review sessions
ta draft review list [--status active|paused|completed]

# Resume a paused session
ta draft review resume <session-id>

# Finish review and apply approved changes
ta draft review finish --approve "src/**" --reject "config.toml"
```

### Architecture

**Modules**:
- `crates/ta-changeset/src/review_session.rs` — Core data model (ReviewSession, CommentThread, etc.)
- `crates/ta-changeset/src/review_session_store.rs` — Persistent JSON storage
- `crates/ta-changeset/src/draft_package.rs` — Artifact.comments field integration

**Tests**: 50 unit tests covering session lifecycle, comment threads, disposition tracking, and persistence.

### Workflow Integration

Review Sessions integrate with existing workflows:

1. **Draft Build**: `ta draft build` creates a draft package as usual
2. **Start Review**: `ta draft review start <draft-id>` creates a persistent session
3. **Iterative Review**: Add comments, set dispositions, pause/resume across multiple CLI invocations
4. **Finish**: `ta draft review finish` applies approved changes (uses existing selective review logic)

### Follow-Up Goals Integration

When artifacts have `Discuss` disposition:
- `ta run --follow-up <goal-id>` injects comment threads as structured context
- Agent addresses each discussed artifact with explanations
- New draft supersedes the original

### Correcting a Draft

When you spot an issue in a draft (duplicated code, a typo, a wrong approach), you have three correction paths depending on the size of the fix:

#### 1. Full re-work (architectural changes)
Use when the issue requires rethinking the approach:
```bash
# Mark problematic artifacts as Discuss with context
ta draft review start <draft-id>
ta draft review comment "fs://workspace/src/auth.rs" "Wrong approach — use JWT not sessions"
ta draft review discuss "fs://workspace/src/auth.rs"
ta draft review finish

# Follow-up goal inherits your comments + discuss items
ta run "Rework auth to use JWT per review feedback" --source . --follow-up <draft-id>
```

#### 2. Scoped agent fix (v0.3.4)
Use when the issue is clear but needs agent help to implement:
```bash
# Agent targets only the discussed artifacts, not the full source tree
ta draft fix <draft-id> --guidance "Remove AgentAlternative, reuse AlternativeConsidered directly"

# Target a specific artifact
ta draft fix <draft-id> "fs://workspace/src/draft.rs" --guidance "Consolidate duplicate struct"

# Set up workspace without launching agent (manual mode)
ta draft fix <draft-id> --guidance "Fix the issue" --no-launch
```
- Creates a scoped follow-up goal with your guidance injected into the agent context
- Agent sees the discuss items, comment threads, and your guidance — nothing else
- New draft supersedes the original — review and apply as normal

#### 3. Direct amendment (v0.3.4)
Use for typos, renames, and small fixes you can make yourself:
```bash
# Replace an artifact's content with a corrected file
ta draft amend <draft-id> "fs://workspace/src/draft.rs" --file corrected_draft.rs

# Shorthand: paths without fs://workspace/ prefix also work
ta draft amend <draft-id> src/draft.rs --file corrected_draft.rs

# Drop an artifact from the draft entirely
ta draft amend <draft-id> "fs://workspace/config.toml" --drop

# Include a reason for the audit trail
ta draft amend <draft-id> src/main.rs --file fixed_main.rs --reason "Fixed typo in function name"
```
- Amends the draft in-place (no new goal or agent run needed)
- Records who amended it, when, and why in the artifact's `amendment` field
- Disposition resets to `pending` (content changed, needs re-review)
- Decision log entry auto-added for every amendment
- Corrected file is written back to the staging workspace for consistency

> **When to use each**: `amend` for typos, renames, and small fixes you can make yourself. `fix` for logic changes that need agent help. Full re-work for architectural rework.

---

## Draft Lifecycle Hygiene

**New in v0.3.6** — Tools for cleaning up stale draft state.

### Closing a Draft

Close a draft without applying it (e.g., hand-merged, abandoned, or obsolete):

```bash
ta draft close <draft-id>
ta draft close <draft-id> --reason "Hand-merged upstream"
```

### Finding Stale Drafts

List drafts that are in reviewable states (Draft, PendingReview, Approved) but older than the configured threshold:

```bash
ta draft list --stale
```

### Garbage Collection

Remove staging directories for drafts in terminal states (Applied, Denied, Closed) older than N days (default 7):

```bash
# Preview what would be removed
ta draft gc --dry-run

# Remove stale staging directories
ta draft gc

# Archive instead of deleting
ta draft gc --archive
```

Configure thresholds in `.ta/workflow.toml`:

```toml
[gc]
stale_threshold_days = 7   # Days before staging dirs become eligible for cleanup
health_check = true        # Show warning on startup if stale drafts exist
```

### Auto-Close on Follow-Up

When a follow-up goal's draft is applied, TA automatically closes the parent draft if it's still in PendingReview or Approved state. **v0.4.1.2**: Auto-close only applies when the follow-up shares the same staging directory as the parent (extend case). Standalone follow-ups with separate staging leave the parent draft independently reviewable.

### Startup Health Check

On every `ta` invocation, a one-line hint is printed to stderr if any drafts have been approved or pending for 3+ days without being applied. Suppress via `[gc] health_check = false`.

---

## Interactive Sessions

**Introduced in v0.3.1.2** — Interactive session orchestration for human-agent collaboration. Enhanced in v0.4.1.1 with the ReviewChannel architecture.

### Starting an Interactive Session

Use `--interactive` to create a session with lifecycle tracking:

```bash
ta run "Implement feature X" --source . --interactive

# Output:
# Interactive session: 8a7b6c5d-...
#   Channel: cli:12345
# Launching claude in staging workspace...
#   Mode: interactive (session orchestration enabled)
```

The session tracks the goal-agent relationship, channel identity, message history, and associated draft reviews.

### Managing Sessions

```bash
# List active sessions
ta session list

# Show all sessions (including completed)
ta session list --all

# View session details and message history
ta session show <session-id>
# Accepts full UUID or prefix (e.g., "8a7b")
```

### Session Lifecycle

Sessions follow this state machine:
- **Active** — agent running, human connected
- **Paused** — agent suspended, can be resumed (Active <-> Paused)
- **Completed** — session finished successfully
- **Aborted** — session killed by human or error

### Per-Agent Interactive Config

Add an `interactive` block to your agent YAML config (`.ta/agents/<name>.yaml`):

```yaml
command: claude
args_template: ["{prompt}"]
injects_context_file: true
interactive:
  enabled: true
  output_capture: pipe   # pipe, pty, or log
  allow_human_input: true
  auto_exit_on: "idle_timeout: 300s"
  resume_cmd: "claude --resume {session_id}"
```

### Multi-Session Orchestration

Multiple sessions can run concurrently (different goals, different agents):

```bash
# Session 1: feature work with Claude
ta run "Implement auth" --source . --interactive --agent claude-code

# Session 2: testing with Codex
ta run "Write tests for auth" --source . --interactive --agent codex

# See all active sessions
ta session list
```

---

## Macro Goals & Inner-Loop Iteration

*(v0.4.1)* Macro goals let agents stay in a single session, decompose work into sub-goals, submit drafts for review, and iterate based on feedback — all without exiting and restarting `ta run`.

### Starting a Macro Goal Session

```bash
ta run "Build v0.5 features" --source . --macro
```

The agent receives MCP tools (`ta_draft`, `ta_goal_inner`, `ta_plan`) for interacting with TA during the session.

### How It Works

1. Agent works on a logical unit of change
2. Agent calls `ta_draft { action: "build" }` to package changes
3. Agent calls `ta_draft { action: "submit" }` to submit for review
4. Human reviews and approves/denies via `ta draft approve/deny`
5. Agent receives the result and either continues or revises

### Sub-Goals

Agents can create sub-goals within a macro session:

```
ta_goal_inner { action: "start", macro_goal_id: "...", title: "Add auth module" }
```

Sub-goals inherit the macro goal's plan phase, source directory, and agent configuration.

### Viewing Macro Goal Status

```bash
# Shows sub-goal tree with approval status
ta goal status <macro-goal-id>

# Drafts show macro context in the list
ta draft list
```

### Security Boundaries

- Agents **can**: propose sub-goals, build drafts, submit for review, read plan status
- Agents **cannot**: approve their own drafts, apply changes, bypass checkpoints
- Every sub-goal draft goes through the same human review gate

### Execution Modes

- **Blocking** (default, v0.4.1): Agent submits draft and waits for human response
- **Optimistic** (future): Agent continues while draft is pending
- **Hybrid** (future): Agent marks sub-goals as blocking or non-blocking based on risk

---

## Interactive Terminal Sessions

**New in v0.4.1.1** — Detailed guide to running interactive macro goal sessions in your terminal, where the agent works continuously and you review drafts inline without leaving the session.

### Overview

When you run `ta run --macro`, TA starts the agent and an MCP gateway server side by side. The agent calls MCP tools (`ta_draft`, `ta_goal_inner`, `ta_plan`) to interact with TA during the session. When the agent needs human input — a draft review, an approval question, a plan update — TA renders the request directly in your terminal and waits for your response. The agent stays alive the whole time.

This is the **inner-loop iteration** pattern: the agent proposes, you decide, the agent continues. No exit-and-restart cycle.

### Starting a Macro Session

```bash
ta run "Refactor auth module and add tests" --source . --macro
```

Output:
```
Goal created: a1b2c3d4-...
  Source: /home/you/project
  Agent: claude-code
  Mode: macro goal (inner-loop iteration enabled)

Launching claude in staging workspace...
  Workspace: ~/.ta/staging/a1b2c3d4/
  MCP gateway: listening on local socket
```

The agent now has access to three MCP tools:

| Tool | Purpose |
|------|---------|
| `ta_draft` | Build and submit draft packages for review |
| `ta_goal_inner` | Create and manage sub-goals within the macro session |
| `ta_plan` | Read plan progress and propose plan updates |

### What a Session Looks Like

Here's a realistic terminal session. The agent works autonomously, pausing only when it needs your input:

```
[Agent] Working on auth module refactor...
[Agent] Extracted JWT validation into separate module.
[Agent] Built draft abc123: 4 files changed

  ┌─────────────────────────────────────────────────┐
  │ Draft Ready for Review                          │
  │                                                 │
  │ Draft: abc123                                   │
  │ Files: src/auth/mod.rs, src/auth/jwt.rs,        │
  │        src/auth/middleware.rs, src/lib.rs        │
  │ Summary: Extract JWT validation into dedicated  │
  │          module, add middleware helper           │
  │                                                 │
  │ [a]pprove  [r]eject  [d]iscuss  [v]iew diff     │
  └─────────────────────────────────────────────────┘
> a

  ✓ Approved. Agent continuing...

[Agent] Starting sub-goal: "Add unit tests for JWT module"
[Agent] Built draft def456: 2 files changed

  ┌─────────────────────────────────────────────────┐
  │ Draft Ready for Review                          │
  │                                                 │
  │ Draft: def456 (sub-goal: "Add unit tests")      │
  │ Files: tests/auth_jwt_test.rs, src/auth/jwt.rs  │
  │ Summary: 12 unit tests covering token parsing,  │
  │          expiry validation, and error cases      │
  │                                                 │
  │ [a]pprove  [r]eject  [d]iscuss  [v]iew diff     │
  └─────────────────────────────────────────────────┘
> d the error case tests should also cover malformed headers

  ✐ Discussion noted. Agent revising...

[Agent] Added malformed header test cases.
[Agent] Built draft ghi789: 1 file changed

  ┌─────────────────────────────────────────────────┐
  │ Draft Ready for Review (revision)               │
  │                                                 │
  │ Draft: ghi789                                   │
  │ Files: tests/auth_jwt_test.rs                   │
  │ Summary: Added 3 tests for malformed JWT        │
  │          headers (missing alg, empty payload,   │
  │          truncated signature)                   │
  │                                                 │
  │ [a]pprove  [r]eject  [d]iscuss  [v]iew diff     │
  └─────────────────────────────────────────────────┘
> a

  ✓ Approved. Agent continuing...

[Agent] All sub-goals complete. Macro goal finished.
  3 drafts: 2 approved, 1 superseded
```

### Response Options

When a draft is presented for review, you have four options:

| Key | Action | What happens |
|-----|--------|-------------|
| `a` | **Approve** | Draft is approved. Agent continues to next work item. |
| `r` | **Reject** | Draft is rejected with your reason. Agent revises or moves on. |
| `d` | **Discuss** | Add a comment. Type your feedback after `d`. Agent receives it and revises. |
| `v` | **View diff** | Show the full diff inline before deciding. Returns to the prompt after. |

For discuss and reject, type your feedback on the same line:

```
> d please use consistent error types — see src/error.rs
> r this duplicates the existing validation in middleware.rs
```

### Viewing Diffs Inline

Press `v` at the review prompt to see the full diff:

```
> v

--- a/src/auth/jwt.rs
+++ b/src/auth/jwt.rs
@@ -1,5 +1,24 @@
+use jsonwebtoken::{decode, DecodingKey, Validation};
+use crate::error::AuthError;
+
+pub fn validate_token(token: &str) -> Result<Claims, AuthError> {
+    let key = DecodingKey::from_secret(b"secret");
+    let data = decode::<Claims>(token, &key, &Validation::default())?;
+    Ok(data.claims)
+}
...

  [a]pprove  [r]eject  [d]iscuss
> a
```

### Sub-Goal Decomposition

The agent can break work into sub-goals. Each sub-goal produces its own draft:

```
[Agent] Creating sub-goal 1 of 3: "Extract JWT validation"
[Agent] Creating sub-goal 2 of 3: "Add middleware helper"
[Agent] Creating sub-goal 3 of 3: "Write integration tests"
```

Sub-goals inherit the macro goal's workspace, plan phase, and agent configuration. You review each sub-goal's draft independently — approve some, discuss others, reject if needed.

Check the sub-goal tree at any time from another terminal:

```bash
ta goal status <macro-goal-id>

# Output:
# Goal: a1b2c3d4 — "Refactor auth module and add tests"
#   State: Running (macro)
#   Sub-goals:
#     ├── e5f6g7h8 — "Extract JWT validation"     ✓ Approved
#     ├── i9j0k1l2 — "Add middleware helper"       ⏳ PendingReview
#     └── m3n4o5p6 — "Write integration tests"     ◌ Running
```

### Plan Updates

The agent can propose plan status updates during the session:

```
  ┌─────────────────────────────────────────────────┐
  │ Plan Update Proposed                            │
  │                                                 │
  │ Phase: v0.4.1.1                                 │
  │ Note: "Auth module refactored, tests passing.   │
  │        JWT extraction complete."                │
  │                                                 │
  │ [a]pprove  [r]eject                             │
  └─────────────────────────────────────────────────┘
> a
```

Plan updates are governance-gated: the agent proposes, you approve. The agent cannot unilaterally mark plan phases as done.

### Monitoring from Another Terminal

While a macro session runs, you can use standard TA commands from a second terminal:

```bash
# See all drafts from the current macro session
ta draft list

# View a specific draft's details
ta draft view <draft-id>

# Check session status
ta session list

# View session message history
ta session show <session-id>
```

### Configuration

Macro sessions use the `TerminalChannel` by default. No configuration is needed for terminal-based interaction.

Future channel adapters (Slack, Discord, email) will be selectable via config:

```yaml
# .ta/config.yaml (future, v0.5.3+)
review:
  channel: terminal    # default — renders in your terminal
  # channel: slack     # future — sends Block Kit cards to Slack
  # channel: discord   # future — embed + reaction handler
```

The `ReviewChannel` trait is interaction-agnostic — it carries draft reviews, approval discussions, plan negotiations, and escalations through the same protocol. Adding a new channel requires only implementing the trait; no changes to TA core or the MCP gateway.

### Combining `--macro` with `--interactive`

The `--interactive` flag adds session lifecycle tracking (session ID, message history, state transitions) on top of `--macro`:

```bash
ta run "Refactor auth" --source . --macro --interactive

# Output includes both:
#   Interactive session: 8a7b6c5d-...
#     Channel: cli:12345
#   Mode: macro goal (inner-loop iteration enabled)
```

This gives you:
- **`--macro`**: MCP tools for the agent, inner-loop draft/review cycle
- **`--interactive`**: Session tracking, message history, pause/resume lifecycle

For most workflows, `--macro` alone is sufficient. Add `--interactive` when you want session history persistence and the ability to resume sessions.

### Tips for Effective Macro Sessions

**Keep sub-goals focused.** A macro session works best when the agent decomposes work into small, reviewable units. If a draft touches 20+ files, consider asking the agent to break it down further.

**Use discuss for guidance, not just critique.** The discuss response is your main tool for steering the agent. Instead of rejecting, discuss: "good direction, but use the existing `AuthError` type from `src/error.rs`".

**Review diffs before approving.** Press `v` to see the full diff. This is especially important for changes to public APIs, config files, and security-sensitive code.

**Let the agent finish.** The agent receives your feedback and revises automatically. You don't need to re-explain the goal — it has full context from the macro session.

**Watch for drift.** In longer sessions, check that the agent's work stays aligned with the original goal. If the agent starts proposing unrelated changes, reject and redirect.

### Troubleshooting Interactive Sessions

**Agent doesn't pause for review**: Ensure you used `--macro`. Without it, the agent has no MCP tools and exits after one pass.

**Draft prompt doesn't appear**: The MCP gateway may not have started. Check for port conflicts or MCP server errors in the agent output.

**Session shows "Aborted"**: The agent process crashed or was killed. Check the session log:
```bash
ta session show <session-id>
```

**Want to review later instead of inline**: You can ignore the terminal prompt (Ctrl+C the session), then review offline:
```bash
ta draft list
ta draft approve <draft-id>
# The agent won't continue, but the draft is approved for later apply
```

---

## External Diff Handlers

### Use Cases

**Game Development**:
```toml
[[handler]]
pattern = "Content/**/*.uasset"
command = "UnrealEditor"
args = ["{file}"]
```

**3D Art**:
```toml
[[handler]]
pattern = "models/**/*.blend"
command = "blender"
args = ["--background", "{file}", "--python", "scripts/preview.py"]
```

**Document Review**:
```toml
[[handler]]
pattern = "docs/**/*.pdf"
command = "open"
args = ["-a", "Skim", "{file}"]
```

### Platform-Specific Examples

**macOS**:
```toml
[[handler]]
pattern = "*.png"
command = "open"
args = ["-a", "Preview", "{file}"]
```

**Linux**:
```toml
[[handler]]
pattern = "*.png"
command = "gimp"
args = ["{file}"]
```

**Windows**:
```toml
[[handler]]
pattern = "*.png"
command = "mspaint"
args = ["{file}"]
```

### Behavior

1. **Handler configured** → Opens in specified app
2. **No handler** → Falls back to OS default (`open`/`xdg-open`/`start`)
3. **OS default fails** → Shows inline diff or `[binary: size]` for binaries

---

## Git Integration

### Automatic Git Workflow

```bash
# Apply changes and create a git commit
ta pr apply <pr-id> --git-commit

# Commit subject: goal title
# Commit body: agent summary + file list

# Full workflow: apply → commit → push → open PR
ta pr apply <pr-id> --submit

# Or configure in workflow.toml
```

### Branch Management

Configured via `.ta/workflow.toml`:

```toml
[submit.git]
branch_prefix = "ta/"        # Creates: ta/goal-title
target_branch = "main"       # PR base branch
```

```bash
# Manual branch creation
git checkout -b feature/my-feature
ta run "Implement feature" --source .
# Changes committed to current branch
```

### PR Templates

```toml
[submit.git]
pr_template = ".ta/pr-template.md"
```

**Example template** (`.ta/pr-template.md`):
```markdown
## Summary
{summary}

## Changes
{artifacts}

## Test Plan
- [ ] Unit tests pass
- [ ] Manual testing completed

## Linked Issues
Closes #

🤖 Generated with [Trusted Autonomy](https://github.com/trustedautonomy/ta)
```

---

## Agent Configuration

TA uses YAML config files to define how each agent is launched. This makes it easy to add new agent frameworks without code changes.

### Built-in agents

TA ships with configs for:
- **claude-code** — Anthropic's Claude Code CLI (default)
- **codex** — OpenAI's Codex CLI
- **claude-flow** — Multi-agent orchestration via Claude Flow

### Custom agents

Create a YAML config for any agent framework:

```yaml
# .ta/agents/my-agent.yaml
name: my-agent
description: "My custom coding agent"
command: my-agent-cli
args_template:
  - "--mode"
  - "autonomous"
  - "{prompt}"
injects_context_file: false
injects_settings: false
env:
  MY_AGENT_LOG_LEVEL: "info"
```

Then use it:

```bash
ta run "Fix the bug" --agent my-agent --source .
```

### Config search order

TA searches for agent configs in priority order:
1. `.ta/agents/<agent>.yaml` — project-specific override
2. `~/.config/ta/agents/<agent>.yaml` — user-wide override
3. Built-in defaults (shipped with TA binary)
4. Hard-coded fallback (runs command with no special args)

### Config fields

| Field | Type | Description |
|-------|------|-------------|
| `command` | string | Command to execute (must be on PATH) |
| `args_template` | string[] | Arguments; `{prompt}` is replaced with the goal text |
| `injects_context_file` | bool | Inject goal context into CLAUDE.md before launch |
| `injects_settings` | bool | Inject `.claude/settings.local.json` with permissions |
| `pre_launch` | object | Optional command to run before agent launch |
| `env` | map | Environment variables for the agent process |

---

## Versioning & Release Lifecycle

### Version Format

TA uses [semver](https://semver.org/): `MAJOR.MINOR.PATCH-prerelease`.

Plan phases in `PLAN.md` use hierarchical IDs for readability (e.g., `v0.4.1.2`). Release versions are strict semver. The conversion:

| Plan Phase | Release Version | `ta release run` |
|---|---|---|
| v0.4 | `0.4.0-alpha` | `ta release run 0.4.0-alpha` |
| v0.4.1 | `0.4.1-alpha` | `ta release run 0.4.1-alpha` |
| v0.4.1.2 | `0.4.1-alpha.2` | `ta release run 0.4.1-alpha.2` |

### Pre-release Tags

| Tag | Meaning | When |
|---|---|---|
| **alpha** | Active development. APIs may change. Not for production. | All `0.x` work (current) |
| **beta** | Feature-complete for the cycle. APIs stabilizing. Early adopters welcome. | All planned phases for the minor version are done; no known critical bugs |
| **rc.N** | Release candidate. Bug fixes only. | Beta testing complete; no API changes expected |
| *(none)* | Stable release. Semver guarantees apply. | RC passes without blockers |

**Current state**: All releases are `alpha`. Beta begins when the core loop is proven (target: v0.8 Department Runtime). Stable `1.0.0` requires all v0.x features hardened, public API frozen, and security audit complete.

**Progression example**:
```
0.4.1-alpha → 0.4.1-alpha.1 → 0.4.1-alpha.2 → 0.4.2-alpha → ...
0.8.0-alpha → 0.8.0-beta → 0.8.0-rc.1 → 0.8.0
1.0.0-beta → 1.0.0-rc.1 → 1.0.0
```

## Release Pipeline

TA includes a configurable release pipeline driven by YAML. Each step is either a shell command or a TA goal (agent-driven), with optional approval gates.

### Quick start

```bash
# Run the built-in release pipeline
ta release run 0.4.0-alpha

# Preview what would run (no side effects)
ta release run 0.4.0-alpha --dry-run

# Show the pipeline steps
ta release show

# Create a customizable .ta/release.yaml from the default template
ta release init
```

### Pipeline configuration

The pipeline is loaded from (in priority order):

1. `--pipeline <path>` flag (explicit override)
2. `.ta/release.yaml` in the project root
3. Built-in default pipeline (compiled into the binary)

### YAML schema

```yaml
name: my-release

steps:
  - name: Build & test
    run: |
      ./dev cargo build --workspace
      ./dev cargo test --workspace

  - name: Generate release notes
    agent:
      id: claude-code
      phase: "v0.4.0"
    objective: |
      Synthesize release notes for ${TAG}.
      Commits since ${LAST_TAG}:
      ${COMMITS}
    output: .release-draft.md

  - name: Push to remote
    requires_approval: true
    run: git push origin main && git push origin ${TAG}
```

Each step must have either `run` (shell command) or `agent` (TA goal). Steps support:

- **`name`** (required): Human-readable step name
- **`run`**: Shell command(s) executed via `sh -c`
- **`agent`**: TA goal with `id` (agent system) and optional `phase`
- **`objective`**: Description for agent steps (supports variable substitution)
- **`requires_approval`**: Pause for human confirmation before executing
- **`output`**: Expected output artifact path (informational)
- **`working_dir`**: Working directory override (relative to project root)
- **`env`**: Environment variables for the step

### Variable substitution

These variables are available in `run`, `objective`, `output`, and `env` values:

| Variable | Description | Example |
|----------|-------------|---------|
| `${VERSION}` | Target version | `0.4.0-alpha` |
| `${TAG}` | Git tag | `v0.4.0-alpha` |
| `${COMMITS}` | Commit messages since last tag | Multi-line text |
| `${LAST_TAG}` | Previous git tag | `v0.3.2-alpha` |

### CLI options

```bash
ta release run <VERSION>        # Run the pipeline
  --yes                         # Skip approval gates (CI mode)
  --dry-run                     # Show steps without executing
  --from-step <N>               # Start from step N (1-indexed)
  --pipeline <PATH>             # Use a custom pipeline file
```

## Advanced Workflows

### Plan-Linked Goals

Link goals to PLAN.md phases for automatic tracking:

```bash
ta run "Complete Phase 4b" --source . --phase 4b

# When applied, PLAN.md is auto-updated to mark phase done
# History is recorded to .ta/plan_history.jsonl
# The next pending phase is auto-suggested
```

### Plan Lifecycle Commands

```bash
# List all plan phases with status
ta plan list

# Show progress summary
ta plan status

# Show next pending phase with suggested ta run command
ta plan next

# Validate a specific phase — shows linked goals and draft summaries
ta plan validate v0.3.1

# View plan change history (status transitions)
ta plan history

# Extract a plan-schema.yaml from an existing plan document
ta plan init                    # interactive — proposes schema, asks to confirm
ta plan init --yes              # non-interactive — writes immediately
ta plan init --source ROADMAP.md  # analyze a different file

# Generate a new plan from a template
ta plan create                            # greenfield template → PLAN.md
ta plan create --template feature         # feature template
ta plan create --template bugfix          # bugfix template
ta plan create --output ROADMAP.md        # different output file
ta plan create --name "My Project"        # custom project name
```

The plan parser is schema-driven via `.ta/plan-schema.yaml`. If no schema file exists, a built-in default is used that supports `## Phase <id>` top-level headers and `### v0.X.Y` sub-phase headers with `<!-- status: ... -->` markers. Custom schemas allow any project to define its own plan format using regex patterns.

### Conflict Detection

If source files change during a goal (v0.2.1):

```bash
ta pr apply <pr-id>
# ⚠️ WARNING: Source files have changed since goal start!
#    3 conflict(s) detected:
#    - src/main.rs (modified)
#    - src/lib.rs (modified)
#    - Cargo.toml (modified)
#    Resolution strategy: abort

# Override with force (dangerous - may lose changes)
ta pr apply <pr-id> --conflict-resolution force-overwrite

# Or use git merge (if git adapter is configured)
ta pr apply <pr-id> --conflict-resolution merge
```

### Multi-Agent Workflows

```bash
# Goal 1: Backend work
ta run "Add REST API endpoint" --source .
ta pr apply <backend-pr-id>

# Goal 2: Frontend work (depends on backend)
ta run "Add UI for new endpoint" --source . --follow-up <backend-goal-id>
ta pr apply <frontend-pr-id>
```

### Audit Trail

```bash
# Verify audit log integrity (hash chain)
ta audit verify

# Show recent audit events
ta audit tail -n 20

# Display decision trail for a goal with reasoning (v0.3.3)
ta audit show <goal-id>

# Export structured audit data for compliance reporting (v0.3.3)
ta audit export <goal-id> --format json

# Compute and store a behavioral baseline for an agent (v0.4.2)
ta audit baseline <agent-id>

# Show behavioral drift report (v0.4.2)
ta audit drift <agent-id>

# Drift summary across all agents with stored baselines (v0.4.2)
ta audit drift --all
```

#### Behavioral Drift Detection (v0.4.2)

TA monitors agent behavior for drift from historical baselines. Five drift signals are computed from the audit log and draft package history:

- **Resource scope drift** — accessing URIs outside the agent's historical pattern
- **Escalation frequency change** — significant shifts in policy escalation rate
- **Rejection rate drift** — drafts getting rejected more or less often
- **Change volume anomaly** — unexpectedly large or small diffs
- **Dependency pattern shift** — unusual rate of new external dependency changes

Baselines are stored in `.ta/baselines/<agent-id>.json` and computed from all historical audit events and draft packages for that agent. Use `ta audit baseline <agent-id>` to create or update a baseline, then `ta audit drift <agent-id>` to compare recent behavior.

#### Decision Observability (v0.3.3)

Every decision in the TA pipeline is now observable — not just *what happened*, but *what was considered and why*:

- **Policy decisions** capture which grants were checked, which matched, and why Allow/Deny/RequireApproval was chosen
- **Agent decisions** can include `alternatives_considered` in `change_summary.json` to document rejected approaches
- **Review decisions** support structured `reasoning` with rationale, alternatives, and applied principles
- **Compliance export** includes ISO 42001, IEEE 7001, and NIST AI RMF alignment metadata

### Agent Alignment Profiles (v0.4.0)

Alignment profiles let you declare **what an agent can do, what it must escalate, and what it must never touch** — before it starts working. TA compiles these declarations into enforceable capability grants. The agent doesn't decide its own permissions; you do.

#### Who this is for

**Team lead / project owner** — You want to let AI agents work autonomously on your codebase, but you need guardrails. Alignment profiles let you say "read anything, write source code, run tests — but never touch credentials or make network calls" in a single config file.

**Developer using TA daily** — You configure agents once per project. When you run `ta run`, the agent gets a capability manifest derived from its alignment profile. If it tries something outside bounds, the policy engine blocks it. You don't have to watch it constantly.

**Non-technical reviewer** — You don't need to write these files yourself. The defaults work out of the box. When reviewing a draft (`ta draft view`), the audit trail shows exactly which capabilities the agent had and whether it stayed within bounds.

#### How it works

Each agent has a YAML config in `agents/`. The `alignment` block declares its constraints:

```yaml
# agents/claude-code.yaml
alignment:
  principal: "project-owner"        # Who authorized this agent
  autonomy_envelope:
    bounded_actions:                 # What the agent CAN do
      - "fs_read"                   # Read any file
      - "fs_write_patch"            # Write/patch files
      - "fs_apply"                  # Apply changesets
      - "exec: cargo test"          # Run tests
      - "exec: cargo build"         # Build the project
    escalation_triggers:             # When to pause and ask a human
      - "new_dependency"            # Adding a new library
      - "security_sensitive"        # Touching auth, crypto, secrets
      - "breaking_change"           # Changing public APIs
    forbidden_actions:               # What the agent must NEVER do
      - "network_external"          # No outbound network calls
      - "credential_access"         # No reading secrets/tokens
  constitution: "default-v1"        # Behavioral ruleset
  coordination:
    allowed_collaborators:           # Other agents it can work with
      - "codex"
      - "claude-flow"
    shared_resources:                # Files visible to collaborators
      - "src/**"
      - "tests/**"
      - "crates/**"
```

When you run `ta run "Fix the login bug"`, TA's **Policy Compiler** reads this profile and produces a `CapabilityManifest` — a set of typed grants scoped to `fs://workspace/**`. The policy engine enforces these grants for every action the agent takes during the goal.

#### Action format reference

| Format | Example | Meaning |
|--------|---------|---------|
| `tool_verb` | `fs_read` | Tool = `fs`, verb = `read` |
| `tool_verb_qualifier` | `fs_write_patch` | Tool = `fs`, verb = `write_patch` |
| `exec: command` | `exec: cargo test` | Shell command = `cargo test` |

#### Common profiles

**Read-only auditor** — Can read everything, write nothing:
```yaml
bounded_actions: ["fs_read"]
forbidden_actions: ["fs_write_patch", "fs_apply", "network_external", "credential_access"]
```

**Full developer** (default) — Read, write, build, test. No network or credentials:
```yaml
bounded_actions: ["fs_read", "fs_write_patch", "fs_apply", "exec: cargo test", "exec: cargo build"]
forbidden_actions: ["network_external", "credential_access"]
```

**Multi-agent orchestrator** — Delegates to other agents, needs coordination:
```yaml
bounded_actions: ["fs_read", "fs_write_patch", "fs_apply"]
forbidden_actions: ["network_external", "credential_access"]
coordination:
  allowed_collaborators: ["claude-code", "codex"]
  shared_resources: ["src/**", "tests/**"]
```

#### Practical workflows

**Starting a new project with TA:**

1. The default agent configs ship with sensible alignment profiles. Run `ta run "Set up the project"` — it just works.
2. Review the draft with `ta draft view`. The audit trail confirms the agent stayed within its declared bounds.

**Tightening permissions for a sensitive repo:**

1. Edit `agents/claude-code.yaml` — remove `fs_write_patch` from `bounded_actions`, add it to `escalation_triggers`.
2. Now the agent can read freely but must ask before writing. Every write gets flagged for human approval.

**Adding a new agent (e.g., a linter):**

1. Copy `agents/generic.yaml` to `agents/my-linter.yaml`.
2. Uncomment the `alignment` block, set `bounded_actions: ["fs_read", "exec: npm run lint"]`.
3. Set `forbidden_actions` to everything else. The agent can only read and lint.

**Non-technical user reviewing agent work:**

1. Run `ta draft list` to see pending drafts.
2. Run `ta draft view <id>` — each changed file shows what the agent did and why.
3. The alignment profile is recorded in the audit trail. You can verify the agent didn't exceed its declared permissions without reading any code.

### Configurable Summary Exemption (v0.4.0)

When an agent finishes work, `ta draft build` checks that every changed file has a human-readable summary explaining what changed and why. But some files — lockfiles, config manifests, generated files — don't need hand-written summaries.

#### Who this is for

**Any TA user** — The defaults cover common cases (lockfiles, `Cargo.toml`, `PLAN.md`, etc.). You only need to customize this if your project has unusual generated or boilerplate files that keep triggering summary enforcement failures.

#### How it works

Create `.ta/summary-exempt` in your project root with `.gitignore`-style patterns:

```
# .ta/summary-exempt
# Files matching these patterns get auto-summaries at draft build time.

# Lockfiles — content is machine-generated
Cargo.lock
package-lock.json
yarn.lock
pnpm-lock.yaml

# Config manifests — usually just version bumps
Cargo.toml
package.json

# Generated files specific to your project
**/*.generated.*
schema/output/**
```

If this file doesn't exist, TA uses built-in defaults (lockfiles, config manifests, `PLAN.md`, `CHANGELOG.md`, `README.md`).

An example file is provided at `examples/summary-exempt`.

#### When to customize

- Your CI generates files that agents edit (e.g., `schema/output/*.rs`) — add the pattern so draft builds don't fail.
- You have a monorepo with many `Cargo.toml` files — they're already exempt by default via filename matching.
- You want *stricter* enforcement — create a `.ta/summary-exempt` with fewer patterns. Only listed patterns are exempt; everything else requires a summary.

---

## Claude Flow Optimization

When using Claude Flow as your agent framework, these optimizations are available:

### Prompt caching

Claude's API automatically caches system prompts and tool definitions. This is handled transparently by Claude Code and Claude Flow — no configuration needed. Cached prompts reduce latency and cost for repeated operations (like multi-agent swarm tasks that share the same tool definitions).

### Smart model selection

Configure model routing in `.claude/settings.json`:

```json
{
  "claudeFlow": {
    "modelPreferences": {
      "default": "claude-opus-4-6",
      "routing": "claude-haiku-4-5-20251001"
    }
  }
}
```

- **default** (Opus): Used for actual code generation and complex reasoning
- **routing** (Haiku): Used for task routing and agent coordination — fast and cheap

This gives you the best quality for real work while keeping orchestration overhead low.

### Swarm configuration

```json
{
  "claudeFlow": {
    "swarm": {
      "topology": "hierarchical-mesh",
      "maxAgents": 15
    }
  }
}
```

- **hierarchical-mesh**: Combines hierarchical coordination (queen → workers) with mesh peer-to-peer communication for resilience
- **maxAgents**: Controls maximum concurrent agents; adjust based on your API rate limits

### Memory backend

```json
{
  "claudeFlow": {
    "memory": {
      "backend": "hybrid",
      "enableHNSW": true
    }
  }
}
```

- **hybrid**: Combines fast in-memory cache with persistent storage
- **HNSW**: Hierarchical Navigable Small World index for fast semantic search (150x-12,500x faster than keyword search)

### Default configuration for new users

See `examples/claude-settings.json` for an optimized starting configuration that includes all the above settings. Copy it to your project:

```bash
cp examples/claude-settings.json .claude/settings.json
```

---

## Troubleshooting

### Problem: Agent can't access files

**Cause**: Exclude patterns (`.taignore`) or missing capabilities.

**Fix**:
```bash
# Check exclude patterns
cat .taignore

# Grant capabilities in agent manifest (future feature)
```

### Problem: External handler doesn't open

**Cause**: Command not found or incorrect path.

**Fix**:
```bash
# Test command directly
blender /path/to/file.blend

# Check handler config
ta pr view <pr-id> --file test.blend --no-open-external
# Fallback to inline diff to debug
```

### Problem: Selective approval fails with dependency errors

**Cause**: Approved file depends on rejected file.

**Fix**:
```bash
# View dependencies
ta pr view <pr-id>
# Check rationale and dependencies for each file

# Approve coupled changes together
ta pr apply <pr-id> --approve "src/main.rs" --approve "src/lib.rs"
```

### Problem: Merge conflicts on apply

**Cause**: Source files changed since goal started (v0.2.1).

**Fix**:
```bash
# Option 1: Abort and start fresh goal
ta pr apply <pr-id> --conflict-resolution abort
ta run "Redo task" --source .

# Option 2: Force overwrite (careful!)
ta pr apply <pr-id> --conflict-resolution force-overwrite

# Option 3: Use git merge (if git adapter configured)
ta pr apply <pr-id> --conflict-resolution merge
```

---

## Getting Help

- **Documentation**: [GitHub](https://github.com/trustedautonomy/ta)
- **Issues**: [Report bugs](https://github.com/trustedautonomy/ta/issues)
- **Roadmap**: See [PLAN.md](../PLAN.md)
- **Architecture**: See [docs/ARCHITECTURE.md](ARCHITECTURE.md)

---

## Next Steps

- **Production setup**: See [docs/DEPLOYMENT.md](DEPLOYMENT.md) (future)
- **Advanced patterns**: See [docs/PATTERNS.md](PATTERNS.md) (future)
- **Security model**: See [docs/SECURITY.md](SECURITY.md) (future)

---

**Happy building with Trusted Autonomy!** 🚀
