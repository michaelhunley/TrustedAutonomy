# TA Files Reference

A complete guide to every file and directory TA creates or reads under `.ta/`. Covers what each file contains, whether it is committed to version control, and when it is safe to delete.

---

## Quick Reference

| Path | Committed? | Delete safe? | Purpose |
|------|-----------|--------------|---------|
| `.ta/workflow.toml` | Yes | No | Verify commands, submit settings |
| `.ta/workflow.local.toml` | No (gitignored) | Yes | Personal overrides of workflow.toml |
| `.ta/config.toml` | Yes | No | Daemon config, experimental flags |
| `.ta/config.local.toml` | No (gitignored) | Yes | Personal overrides of config.toml |
| `.ta/agents/*.toml` | Yes | No | Agent framework manifests |
| `.ta/plugins/*/channel.toml` | Yes | No | Channel plugin config |
| `.ta/constitution.toml` | Yes | No | Project constitution rules |
| `.ta/pr-template.md` | Yes | No | PR body template |
| `.ta/plan_history.jsonl` | Yes | No* | Timeline of plan phase completions |
| `.ta/release-history.json` | Yes | No* | Project release changelog |
| `.ta/goals/` | No | Yes | Live goal state JSON |
| `.ta/staging/` | No | Yes† | Per-goal source copies |
| `.ta/store/` | No | Yes | Internal KV store |
| `.ta/audit.jsonl` | No | Yes | Local audit trail |
| `.ta/events.jsonl` / `.ta/events/` | No | Yes | Structured event log shards |
| `.ta/goal-history.jsonl` | No | Yes | Machine-specific goal timing |
| `.ta/velocity-stats.jsonl` | No | Yes | CPU/disk-dependent build timing |
| `.ta/operations.jsonl` | No | Yes | Daemon self-healing health log |
| `.ta/memory/` | No | Yes | Per-goal agent memory entries |
| `.ta/memory.rvf` | No | Yes | Memory index file |
| `.ta/pr_packages/` | No | Yes | Draft artifact packages |
| `.ta/backups/` | No | Yes | CLAUDE.md backup during goal injection |
| `.ta/interactive_sessions/` | No | Yes | Interactive session state |
| `.ta/daemon.toml` | No | Yes | Runtime daemon config (PID, port) |
| `.ta/daemon.log` | No | Yes | Daemon log file |
| `.ta/*.pid` | No | Yes | Process ID files |
| `.ta/*.lock` | No | Yes‡ | Lock files |
| `.ta/consent.json` | No | Yes | Terms of use acceptance state |
| `.ta/change_summary.json` | No | Yes | Last-run change summary |
| `.ta/mcp_json_original` | No | Yes | Backup of .mcp.json before injection |

*Append-only by convention — truncating loses history but doesn't break TA.
†Safe after `ta goal gc` confirms the goal is in a terminal state (applied/denied/failed).
‡Safe only when no `ta-daemon` process is running — stale locks from crashed processes.

---

## Config Files (Committed)

### `.ta/workflow.toml`

Team-shared workflow settings. Committed to git.

```toml
[verify]
commands = ["./dev 'cargo test --workspace'"]
on_failure = "block"   # "block" | "warn" | "agent"
timeout = 600          # seconds per command

[submit]
adapter = "git"        # "git" | "none"
auto_commit = true
auto_push = true
auto_review = true

[submit.git]
branch_prefix = "feature/"
target_branch = "main"
merge_strategy = "squash"
remote = "origin"
pr_template = ".ta/pr-template.md"
auto_merge = true

[supervisor]
enabled = true
agent = "builtin"
verdict_on_block = "warn"
```

### `.ta/workflow.local.toml`

Personal overrides for `workflow.toml`. **Gitignored.** Never committed.

```toml
# Example: personal Perforce workspace override
[vcs.agent.p4]
client_template = "michael-mbp-{goal_id}"
port = "ssl:localhost:1666"
```

### `.ta/config.toml`

Project-level daemon config. Committed to git.

```toml
[daemon]
port = 7700
log_level = "info"

[operations]
finalize_timeout_secs = 1800
stale_hint_days = 3
stale_threshold_days = 7

[gc]
auto_gc = true
max_staging_age_days = 14

[experimental]
ollama_agent = false
sandbox = false
```

### `.ta/config.local.toml`

Personal overrides for `config.toml`. **Gitignored.** Use for experimental flags and machine-specific settings.

```toml
[experimental]
ollama_agent = true
sandbox = true
```

### `.ta/agents/*.toml`

Agent framework manifests. Committed. Each file defines one agent: name, framework (`claude` | `codex` | `ollama`), system prompt, and constraints.

```toml
# .ta/agents/code-reviewer.toml
name = "code-reviewer"
framework = "claude"
[system_prompt]
role = "senior code reviewer"
[constraints]
no_file_writes = true
```

### `.ta/plugins/*/channel.toml`

Channel plugin config. Committed. The plugin binary itself is gitignored (built artifact) — only the config is committed.

### `.ta/constitution.toml`

Project constitution. Committed. Defines invariants the agent must not violate (injection/cleanup rules, forbidden patterns, required checks). Read by `ta constitution check` and the supervisor agent.

### `.ta/pr-template.md`

PR body template. Committed. Used by `ta draft apply --submit` when creating the pull request. Supports `{summary}`, `{why}`, `{goal_id}`, `{draft_id}` placeholders.

---

## Tracking Files (Committed, Append-Only)

These files are committed because they're project records, not machine state. Git's line-merge handles concurrent appends cleanly.

### `.ta/plan_history.jsonl`

Timeline of plan phase completions. One JSON line per completed phase, written by `ta draft apply` when `--phase` is set.

```json
{"phase": "v0.13.17.1", "completed_at": "2026-03-23T16:30:00Z", "draft_id": "3e897676", "pr": 265}
```

### `.ta/release-history.json`

Project release changelog. Written by `ta release run` on each successful release. Format is an array of release entries with version, tag, date, and notes summary.

---

## Runtime State (Gitignored)

These are ephemeral or machine-specific. Safe to delete when TA is not running (or after `ta goal gc` for staging dirs).

### `.ta/goals/`

One JSON file per goal run (`<goal-id>.json`). Contains the full `GoalRun` state: status, agent PID, progress notes, staging path, plan phase. Updated continuously while the goal runs.

**Do not edit manually** unless recovering a stuck goal (`ta goal recover` is the safe path).

### `.ta/staging/<goal-id>/`

Full copy of the project source at goal-start time. The agent works here. Can be gigabytes if `target/` was present at copy time.

Safe to delete after the goal is in a terminal state. Use `ta goal gc` to clean all terminal-state staging dirs at once. The GC skips staging dirs whose goal is still `running` or `finalizing`.

### `.ta/store/`

Internal KV store used by the daemon for caching and cross-request state. Safe to delete — rebuilt automatically on next daemon start.

### `.ta/audit.jsonl`

Local audit trail. One JSON line per auditable action (goal start, draft approve, draft apply, etc.). Machine-specific — each developer has their own. The Central Daemon (v0.14.4) will aggregate these server-side.

### `.ta/events.jsonl` / `.ta/events/`

Structured event log. High-volume; sharded into `.ta/events/` daily files. Used for debugging and the velocity stats feature. Safe to delete — only affects historical data.

### `.ta/goal-history.jsonl`

Machine-specific goal timing log. Records start time, end time, outcome, and duration for each goal. Used by `ta stats velocity`. Cross-machine comparison is meaningless (CPU/disk-dependent).

### `.ta/velocity-stats.jsonl`

Aggregated velocity metrics. Derived from `goal-history.jsonl`. Safe to delete — will be recomputed from history on next `ta stats velocity` run.

### `.ta/operations.jsonl`

Daemon self-healing log. Records operational health events: low disk space warnings, stale staging alerts, auto-heal actions taken. Resets on each daemon restart.

### `.ta/memory/` and `.ta/memory.rvf`

Per-goal agent memory entries and their index. Written by the memory MCP server during goal runs. Safe to delete — agents can't retrieve past memories after deletion, but current goals are unaffected.

### `.ta/pr_packages/`

Draft artifact packages (`.tar.gz` or similar) prepared for review. Written by `ta draft build`, consumed by `ta draft apply`. Safe to delete after the draft is applied or denied.

### `.ta/backups/`

Backup of `CLAUDE.md` (and `settings.local.json`) taken before goal injection. Restored automatically after the agent exits. Safe to delete when no goals are running.

### `.ta/interactive_sessions/`

State for interactive `ta shell` sessions. Safe to delete when no sessions are active.

### Runtime files (`.ta/daemon.toml`, `.ta/*.log`, `.ta/*.pid`, `.ta/*.lock`)

Daemon runtime state. The `.pid` and `.lock` files are removed cleanly on normal shutdown. If they persist after a crash, delete them manually before restarting.

---

## 7-Layer Config Resolution

TA resolves configuration by merging these layers in order (later layers win):

| Layer | File | Notes |
|-------|------|-------|
| 1 | Built-in defaults | Hardcoded in the binary |
| 2 | `~/.config/ta/config.toml` | User-global config |
| 3 | `.ta/config.toml` | Project config (committed) |
| 4 | `.ta/config.local.toml` | Personal project overrides (gitignored) |
| 5 | `.ta/workflow.toml` | Project workflow settings (committed) |
| 6 | `.ta/workflow.local.toml` | Personal workflow overrides (gitignored) |
| 7 | CLI flags | Highest precedence |

The `*.local.toml` pattern means team-shared settings live in the committed files and personal/machine-specific settings stay in the gitignored local files — no merge conflicts, no accidental leaking of personal paths or credentials.

---

## What to Commit vs Ignore

**Commit (team-shared config):**
- `workflow.toml`, `config.toml`
- `agents/*.toml`, `plugins/*/channel.toml`
- `constitution.toml`, `pr-template.md`
- `plan_history.jsonl`, `release-history.json` (append-only records)

**Gitignore (personal / machine-specific / ephemeral):**
- `*.local.toml`, `config.local.toml`, `workflow.local.toml`
- `goals/`, `staging/`, `store/`, `backups/`, `memory/`, `pr_packages/`
- `audit.jsonl`, `events.jsonl`, `events/`, `goal-history.jsonl`
- `velocity-stats.jsonl`, `operations.jsonl`, `memory.rvf`
- `daemon.toml`, `*.log`, `*.pid`, `*.lock`, `consent.json`, `change_summary.json`
- Plugin binaries (`plugins/**/ta-channel-*`, `plugins/**/ta-plugin-*`)
