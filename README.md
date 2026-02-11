# Trusted Autonomy

**Trusted Autonomy** is a local-first, Rust-based substrate for running autonomous AI agents **safely**, **reviewably**, and **without changing how agents behave**.

It is not an agent framework.  
It is not an orchestrator.  

It is a **trust and control plane** that sits underneath *any* agent or multi-agent system and ensures:

- agents can operate autonomously inside a defined charter
- all real-world effects are staged, reviewable, and auditable
- humans remain in control at meaningful boundaries (PR-style milestones)
- orchestration layers remain swappable and unaware of the substrate

---

## Core idea

> **Give agents everything they need to do the work — but nothing that can irreversibly affect the world without passing through a single, auditable gateway.**

Trusted Autonomy achieves this by:
- mediating *all* filesystem, network, execution, and external-service access through MCP tools
- defaulting **mutations** to *collection* (staging), not execution
- defaulting **capabilities** to deny unless explicitly granted
- representing each milestone within a goal as a **Pull Request–like package** for review

---

## What this is (and is not)

### This **is**
- a capability-gated runtime substrate
- a policy-enforced MCP gateway
- a staging and review system for agent actions
- a foundation for high-autonomy workflows with human trust

### This is **not**
- an “Agent OS VM”
- a replacement for LangGraph, Claude Flow, CrewAI, etc.
- a UI-first product
- a monolithic orchestration framework

---

## Design principles
- Normal environment illusion: tools feel like standard filesystem/network access, but all effects are mediated.
- Default collect (staged-by-default): changes accumulate as pending review artifacts (PR package) rather than applying immediately.
- Capability boundary (default deny): agents can only perform actions explicitly granted by a signed capability manifest.
- Single chokepoint: all reads/writes and external effects flow through an MCP Gateway with policy enforcement and audit.
- PR-per-milestone workflow: complex goals are decomposed into major steps, each producing a PR package for approval.
- Replaceable orchestration: the substrate is the trust layer; planners/swarms are pluggable.

## “default deny” vs “default collect”
Trusted Autonomy uses two distinct defaults:
1. Capability default (security boundary): default deny. If an agent lacks an explicit capability, the gateway rejects it.
2.  Mutation default (operational workflow): default collect. If the agent is allowed to write, the gateway routes writes into staging (patches/drafts) and queues them for review. Commit/send/post are gated.

Result: within the agent’s charter, work “just happens” and produces a PR package. Human review occurs at major milestones.

## Why MCP is the abstraction boundary

Trusted Autonomy uses **Model Context Protocol (MCP)** as its sole integration surface with agents.

This is intentional.

### MCP gives us:
- a standardized tool interface
- explicit, inspectable actions
- a single chokepoint for policy, audit, and transformation
- compatibility with existing and future agent frameworks

### MCP is treated as:
> **The agent’s “operating environment”, not an API to bypass.**

Agents do not know they are operating in a staged, policy-controlled system — and they do not need to.

---

## Filesystem abstraction (why it works this way)

### Design goal
Agents should be able to **read, write, and modify files normally**, without learning a new model — while all changes remain reviewable.

### How it works
- Agents interact with filesystem tools exposed via MCP
- Those tools operate on a **staging workspace** (isolated directory per goal)
- Reads snapshot the original file; writes create diffs against the snapshot
- All writes become **ChangeSets with diffs**, bundled into PR packages

```
Agent
  ↓
MCP ta_fs_read / ta_fs_write
  ↓
StagingWorkspace (isolated temp directory)
  ↓
ChangeSet → Diff → PR Package → Human Review → Apply
```

### Why staging directories?
- cross-platform (Windows/macOS/Linux)
- no kernel drivers, FUSE mounts, or Git dependency
- native diff/rollback semantics
- binary changes can be summarized and hashed
- maps cleanly to PR-style review
- each GoalRun gets complete isolation

### Why not a mounted VFS?
Kernel-level VFS (FUSE, sandboxfs, etc.) introduces:
- install friction
- permissions complexity
- platform inconsistencies

Those can be added later, but staging workspaces keep the system **portable and maintainable**.

---

## Network & web access abstraction

### Design goal
Allow agents to fetch web content **without enabling prompt injection or uncontrolled exfiltration**.

### How it works
- Agents use MCP web tools (`web.fetch`, `web.search`)
- The gateway:
  - enforces allowlists and rate limits
  - sanitizes active content
  - labels provenance and trust level
  - treats fetched content as **data, never instructions**

### Why not raw network sockets?
Raw sockets:
- bypass policy
- bypass audit
- enable hidden side effects

If needed, a transparent local proxy can be added later — but MCP tools are the correct starting point for deterministic autonomy.

---

## Email, social media, databases: everything is a ChangeSet

### Unifying principle
> **Anything that changes the world is a staged artifact.**

This includes:
- emails
- social media posts
- database writes
- API mutations
- permissions changes

### How it works
Each connector implements:
- `read`
- `write_patch` / `create_draft`
- `preview`
- `commit` (gated)

So:
- an email is drafted, not sent
- a post is created, not published
- a DB mutation is recorded, not applied

All appear in the **same PR package** alongside filesystem changes.

---

## “Default deny” vs “default collect”

Trusted Autonomy uses **two defaults**, intentionally separated:

### Capability default: **deny**
If an agent does not have an explicit capability, the gateway rejects the action.

This is the hard security boundary.

### Mutation default: **collect**
If an agent *does* have permission to write:
- the write is staged
- the change is collected
- a PR package is generated

Commit/send/post requires explicit approval or a narrowly scoped write-through capability.

This allows agents to “just work” inside their charter without risk.

---

## Execution environment (why just-bash exists)

### Design goal
Allow agents to:
- search
- format
- run tests
- scaffold code

…without giving them a real shell or OS.

### Approach
- default execution uses a **just-bash-style emulated shell**
- commands are allowlisted
- filesystem access is limited to the workspace
- transcripts are hashed and audited

For workloads requiring real runtimes, isolated containers or microVMs can be added later — still behind the same MCP gateway.

---

## Nix: why and how it’s used

### Why Nix is included
- reproducible Rust toolchains
- deterministic builds in CI
- consistent dev environments

### Why Nix is **not required** for users
- Nix has a learning curve
- some environments prohibit it
- bundling Nix into desktop apps is heavy

### Strategy
- **Use Nix for developers and CI**
- **Ship bundled binaries for end users**
- **Produce OCI images for cloud deployment**

Nix improves correctness without becoming a dependency tax.

---

## Compatibility with existing agent systems

Trusted Autonomy is designed so that:
- Claude Code
- Codex
- LangGraph
- claude-flow
- Ollama-based agents
- future orchestration layers

…can all run **unchanged** on top.

They see:
- a normal workspace
- normal tools
- normal outputs

They do **not** need to know:
- staging exists
- policies exist
- approvals exist

That separation is the core architectural property.

---

## Why PR-style milestones matter

Continuous human oversight destroys autonomy.  
Zero oversight destroys trust.

Trusted Autonomy enforces review at **meaningful boundaries**:
- when a milestone is complete
- when external effects are requested
- when risk increases

This mirrors how high-trust engineering systems already work.

---

## Future extensions (by design, not accident)

- continuous security auditor agents
- automatic least-privilege recommendations
- anomaly detection over audit logs
- richer diff renderers (spreadsheets, docs, binaries)
- multi-tenant cloud deployments
- stronger runtime isolation tiers

These are additive — not architectural rewrites.

---

## Getting Started

### Prerequisites

**Option A: Using Nix (recommended)**

Nix provides a reproducible dev environment with the exact Rust toolchain, formatter, linter, and test runner — identical on macOS, Linux, and WSL.

1. Install Nix with flakes enabled:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install
   ```

2. (Optional) Install direnv for automatic environment loading:
   ```bash
   nix profile install nixpkgs#direnv
   ```
   Then add the [direnv hook](https://direnv.net/docs/hook.html) to your shell config (`.bashrc`, `.zshrc`, etc.).

3. Enter the dev environment:
   ```bash
   # With direnv (automatic — activates when you cd into the repo):
   direnv allow

   # Without direnv (manual):
   nix develop
   ```

**Option B: Without Nix**

1. Install Rust via [rustup](https://rustup.rs/):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
   The `rust-toolchain.toml` in this repo will automatically select the correct Rust version.

2. Install system dependencies:
   - **macOS:** `brew install openssl pkg-config`
   - **Ubuntu/Debian:** `apt install libssl-dev pkg-config`
   - **Windows:** Use WSL2 with one of the above.

3. Install dev tools (optional but recommended):
   ```bash
   cargo install cargo-nextest just
   ```

### Building

```bash
cargo build --workspace
```

### Running Tests

```bash
# All tests (with cargo-nextest for faster parallel execution)
cargo nextest run --workspace

# Or with standard cargo test
cargo test --workspace
```

### Development Commands

If you have `just` installed (included in the Nix devShell):

```bash
just           # run lint + format check + tests
just build     # build all crates
just test      # run all tests
just check     # format check + clippy lint
just fmt       # auto-format all code
just verify    # full pre-commit check (format, lint, build, test)
```

### Project Structure

```
crates/
  ta-audit/               Append-only event log + SHA-256 hash chain (13 tests)
  ta-changeset/           ChangeSet + PR Package data model (14 tests)
  ta-policy/              Capability manifests + default-deny policy engine (16 tests)
  ta-workspace/           Staging + overlay workspace manager + JSON change store (29 tests)
  ta-goal/                GoalRun lifecycle state machine + event dispatch (20 tests)
  ta-mcp-gateway/         MCP server (rmcp) — 9 tools, policy enforcement (15 tests)
  ta-daemon/              MCP server binary (stdio transport)
  ta-sandbox/             Allowlisted command execution (stub)
  ta-connectors/
    fs/                   Filesystem connector: staging + diffs + apply (11 tests)
    web/                  Web fetch connector (stub)
    mock-drive/           Mock Google Drive (stub)
    mock-gmail/           Mock Gmail (stub)
apps/
  ta-cli/                 CLI: goals, PR review/approve/apply, run, audit, adapters (12 tests + 1 integration)
tests/
  vertical_slice          End-to-end integration test (1 test)
schema/
  pr_package.schema.json  PR package JSON schema
  capability.schema.json  Capability manifest schema
  agent_setup.schema.json Agent setup proposal schema
```

---

## Local Build and Dogfood

### Build

```bash
# Option A: With Nix (recommended)
./dev "cargo build --workspace"

# Option B: Without Nix
cargo build --workspace
```

### Dogfood workflow (transparent overlay)

This is the primary workflow. TA copies your project to a staging directory, the agent works normally in the copy, and TA diffs the result into a PR package for review.

```bash
# 1. Start a goal — creates a staging copy of your project
cargo run -p ta-cli -- goal start "Fix the auth bug" --source .

# 2. Note the staging path and goal ID from the output, then enter staging
cd .ta/staging/<goal-id>/

# 3. Launch your agent (Claude Code, or any tool)
claude

# 4. Agent works normally — reads, writes, runs tests, etc.
#    It doesn't know it's in a staging workspace.
#    When done, exit the agent.

# 5. Build a PR package from the diff
cargo run -p ta-cli -- pr build <goal-id> --summary "Fixed the auth bug"

# 6. Review the changes
cargo run -p ta-cli -- pr view <package-id>

# 7. Approve and apply back to your project (with optional git commit)
cargo run -p ta-cli -- pr approve <package-id>
cargo run -p ta-cli -- pr apply <package-id> --git-commit
```

### One-command shortcut

`ta run claude-code` wraps steps 1-5 into a single command:

```bash
cargo run -p ta-cli -- run claude-code "Fix the auth bug" --source .
# TA creates the staging workspace, injects context into CLAUDE.md,
# launches Claude Code, and auto-builds the PR when Claude exits.
# Then review + approve + apply as above.
```

### Exclude patterns (.taignore)

By default, TA excludes common build artifacts from the staging copy (`target/`, `node_modules/`, `__pycache__/`, etc.) to keep copies fast. To customize, create a `.taignore` file in your project root:

```
# .taignore — one pattern per line
# Lines starting with # are comments

# Rust
target/

# Custom project-specific excludes
large-data/
*.sqlite
```

If no `.taignore` exists, sensible defaults are used. The `.ta/` directory is always excluded.

## Using Trusted Autonomy with Claude Code

### Primary: Transparent overlay (recommended)

See **Local Build and Dogfood** above. The agent works in a normal-looking project directory and never knows TA exists. This works with Claude Code, Codex, or any tool.

### Alternative: MCP-native tools

For agents that support MCP tool integration directly, TA can also expose tools via MCP:

```bash
# Install the Claude Code adapter (generates .mcp.json + .ta/config.toml)
cargo run -p ta-cli -- adapter install claude-code

# Start Claude Code — TA tools appear alongside built-in tools
claude
```

| Tool | What it does |
|------|-------------|
| `ta_goal_start` | Create a GoalRun (allocates staging workspace + capabilities) |
| `ta_fs_read` | Read a source file (snapshots the original) |
| `ta_fs_write` | Write to staging (creates a ChangeSet with diff) |
| `ta_fs_list` | List staged files |
| `ta_fs_diff` | Show diff for a staged file |
| `ta_pr_build` | Bundle staged changes into a PR package for review |
| `ta_pr_status` | Check PR package status |
| `ta_goal_status` | Check GoalRun state |
| `ta_goal_list` | List all GoalRuns |

### Review workflow (both modes)

```bash
cargo run -p ta-cli -- pr list                          # List pending PRs
cargo run -p ta-cli -- pr view <package-id>             # View details + diffs
cargo run -p ta-cli -- pr approve <package-id>          # Approve
cargo run -p ta-cli -- pr apply <package-id> --git-commit  # Apply + commit
```

### Architecture

```
Transparent overlay mode:
  Agent works in staging copy (native tools)
    → TA diffs staging vs source
    → PR Package → Human Review → Approve → Apply

MCP-native mode:
  Agent (Claude Code / Codex / any MCP client)
    |  MCP stdio
    v
  TaGatewayServer (ta-mcp-gateway)
    |-- PolicyEngine (ta-policy)        default deny
    |-- StagingWorkspace (ta-workspace)  all writes staged
    |-- ChangeStore (ta-workspace)       JSONL persistence
    |-- AuditLog (ta-audit)             tamper-evident trail
    '-- GoalRunStore (ta-goal)          lifecycle management
    |
    v
  PR Package → Human Review (CLI) → Approve → Apply
```

### Agent teams

Multiple agents can work simultaneously. Each gets an isolated GoalRun with its own staging workspace and capabilities.

---

## Status

Trusted Autonomy is under active development. **138 tests** across 12 crates.

### Implemented
- **Transparent overlay mediation** — agents work in staging copies using native tools, TA is invisible
- Append-only audit log with SHA-256 hash chain
- Default-deny capability engine with glob pattern matching
- ChangeSet + PR Package data model (aligned with JSON schema)
- Staging workspace with snapshot-and-diff
- Overlay workspace with full-copy and exclude patterns (`.taignore`)
- Filesystem connector bridging MCP to staging
- GoalRun lifecycle state machine with event dispatch
- Real MCP server (rmcp 0.14) with 9 tools and policy enforcement
- CLI: `goal start/list/status`, `pr build/list/view/approve/deny/apply`, `run claude-code`, `audit`, `adapter`
- Agent adapter framework (Claude Code + generic MCP)
- Git integration (`ta pr apply --git-commit`)
- End-to-end: agent works in staging → TA diffs → PR package → review → approve → apply → git commit

### Coming next
- V2: Lazy copy-on-write VFS (reflinks/FUSE) — replaces full copy
- Supervisor agent for PR summary generation
- OCI/gVisor sandbox runner
- Plugin trait registry
- Web UI for review/approval
- ed25519 capability signing
- SQLite change store
- Real connectors: Gmail, Drive, databases

---

## License

Apache 2.0

---

## Philosophy (tl;dr)

> Autonomy is not about removing humans.
>  
> It’s about **moving human involvement to the right abstraction layer**.

Trusted Autonomy exists to make that layer explicit, enforceable, and trustworthy.
