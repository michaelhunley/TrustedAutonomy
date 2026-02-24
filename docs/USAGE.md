# Trusted Autonomy — Usage Guide

**Version**: v0.3.0-alpha (In Progress)

Complete guide to using Trusted Autonomy for safe, reviewable AI agent workflows.

> **Note**: v0.3.0 Review Sessions infrastructure is implemented but CLI commands are coming soon.

---

## Table of Contents

1. [Installation](#installation)
2. [Quick Start](#quick-start)
3. [Core Workflow](#core-workflow)
4. [Configuration](#configuration)
5. [Agent Configuration](#agent-configuration)
6. [PR Review & Approval](#pr-review--approval)
7. **[Review Sessions](#review-sessions)** ⭐ NEW in v0.3.0
8. [External Diff Handlers](#external-diff-handlers)
9. [Git Integration](#git-integration)
10. [Advanced Workflows](#advanced-workflows)
11. [Claude Flow Optimization](#claude-flow-optimization)
12. [Troubleshooting](#troubleshooting)

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
```

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

# The follow-up PR supersedes the parent (single unified diff)
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

### CLI Commands (Coming Soon)

The CLI commands for review sessions are planned but not yet implemented. Planned interface:

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

### Follow-Up Goals (v0.1.2 Integration)

When artifacts have `Discuss` disposition:
- `ta run --follow-up <goal-id>` injects comment threads as structured context
- Agent addresses each discussed artifact with explanations
- New PR supersedes the original (see v0.1.2 Follow-Up Goals)

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
```

The plan parser supports both `## Phase <id>` top-level headers and `### v0.X.Y` sub-phase headers with `<!-- status: ... -->` markers.

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
# View audit log
ta audit list

# Export for compliance
ta audit export audit.jsonl
```

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
