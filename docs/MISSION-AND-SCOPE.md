# Trusted Autonomy — Mission & Scope

> **Status**: Living document
> **Complements**: ADR-product-concept-model.md (five-layer architecture), WHY-TA-vs-VM.md (containment model comparison), user-experience.md (persona walkthroughs)

---

## Mission

**TA exists so that humans can delegate work to AI agents and trust the outcome.**

Agents propose. Humans dispose. TA is the governance layer that makes this safe, observable, and auditable — for any agent framework, any resource type, any delivery channel.

---

## What TA Is

TA is a **governance substrate for autonomous AI work.** It mediates every state-changing action an agent proposes, holds it for human review, and provides a complete audit trail of what happened and why.

TA's value is the shift from **synchronous conversation** (human drives, agent assists) to **asynchronous delegation** (human sets goals, agent works independently, human reviews at decision points).

## What TA Is Not

- **Not an agent framework.** TA does not provide LLM inference, prompt engineering, or agent logic. It wraps any agent framework (Claude Code, Codex, Ollama, LangChain, LangGraph, or custom) and governs its output.
- **Not a build system.** TA does not compile code, run test suites, or manage dependencies. It triggers build tools and captures results as governed events.
- **Not a CI/CD platform.** TA does not replace GitHub Actions, Jenkins, or ArgoCD. It can trigger releases and observe results, but the execution is delegated to purpose-built tools.
- **Not a version control system.** TA does not replace Git, SVN, or Perforce. It uses VCS adapters to stage and submit changes through whatever VCS the project uses.

---

## The Scope Test

Every feature in TA must pass this test:

> **Does this feature govern a transition in the lifecycle of agent work?**

If yes, it belongs in TA. If it's the work itself, it belongs in the agent, the build tool, or an external system.

### Lifecycle transitions TA governs

```
Goal defined
  → Agent configured and launched
    → Agent works in staging (invisible to TA)
      → Agent asks human a question (interactive mode)
    → Draft built from staging diff
      → Human reviews draft
        → Draft approved / denied / partially approved
          → Changes applied to project
            → VCS sync (feature branch → main)
              → Build verified
                → Release cut
```

Each `→` is a **governed transition** — a point where TA enforces policy, captures audit data, emits events, and optionally pauses for human input.

### The three categories

| Category | TA's role | Examples |
|----------|-----------|---------|
| **Govern** | Enforce policy, require approval, log decisions | Draft review, selective approval, policy violations, access constitutions |
| **Orchestrate** | Coordinate transitions between governed steps | Goal → agent launch → draft build → review → apply → sync → build → release |
| **Observe** | Capture results from external tools as events | Build output, test results, VCS status, release artifacts |

TA governs and orchestrates. It does **not** execute the underlying work — agents write code, build tools compile it, VCS tools version it.

---

## Feature Boundary Decisions

### `ta sync` — In Scope

**Why**: TA already governs the submit flow (stage → commit → push → open PR via `SubmitAdapter`). After a PR merges, the project's main branch is stale. `ta sync` governs the reverse transition: pull main, rebase staging if active, surface conflicts.

**Boundary**: TA calls the VCS adapter's sync method. It does not implement rebase logic, cherry-pick, or conflict resolution — those are VCS operations. TA captures the result (success, conflict, error) as an event and surfaces it to the human.

**Adapter**: `SyncAdapter` trait (or merged into `SourceAdapter` that combines current `SubmitAdapter` + sync). Each VCS provides its own implementation:

| Adapter | Sync operation |
|---------|---------------|
| Git | `git fetch origin && git merge origin/main` (or rebase, per config) |
| SVN | `svn update` |
| Perforce | `p4 sync` |

**Config**: Per-project in `.ta/workflow.toml`:
```toml
[sync]
strategy = "merge"     # merge | rebase | fast-forward
auto_sync = false      # sync main after ta draft apply
```

### `ta build` — In Scope (as governed event wrapper)

**Why**: After code lands, the human needs to know if it builds. TA doesn't compile code — but it needs to know the build result to decide whether to proceed (to release, to next phase, or to surface an error). Without `ta build`, there's a gap: TA applies a draft, then goes silent. The human manually runs the build, mentally tracks the result, and manually continues.

**Boundary**: TA calls a configured build command, captures stdout/stderr, emits `build_completed` or `build_failed` events, and logs the result. It does not parse build output, manage dependencies, or fix build errors.

**Adapter**: `BuildAdapter` trait. Implementations are thin wrappers around project-specific build tools:

| Adapter | Build command |
|---------|--------------|
| Cargo (auto-detected) | `cargo build --workspace` |
| npm (auto-detected) | `npm run build` |
| Script | User-defined command |
| Webhook | HTTP POST to external CI, poll for result |

**Config**: Per-project in `.ta/workflow.toml`:
```toml
[build]
command = "cargo build --workspace"   # or auto-detected from framework
test_command = "cargo test --workspace"
on_fail = "notify"                     # notify | block_release | block_next_phase
```

**What TA adds**: The build result flows through TA's event system. Workflows can gate on it. Channels deliver it. Audit logs record it. Without TA, the build is a side effect; with TA, it's a governed checkpoint.

### `ta release` — In Scope (already exists)

**Why**: Releasing is a governed transition — version bumps, changelogs, and artifact publication affect the project's public state. TA already has `ta release run`.

**Extension**: Wire `ta sync` and `ta build` as optional pre-release steps:

```toml
[release]
pre_steps = ["sync", "build", "test"]  # run before cutting release
```

### `ta plan from` — In Scope (via interactive mode)

**Why**: Generating a development plan from a product document is a governed agent interaction — the agent reads the doc, asks clarifying questions (interactive mode), and produces a PLAN.md draft for human review. It's a specific use case of the general interactive mode primitive.

**Boundary**: TA provides the interactive mode infrastructure (`ta_ask_human`, `QuestionRegistry`, conversation history). The planning logic is in the agent's system prompt, not in TA's code.

### Workflow Engine — In Scope (with care)

**Why**: Multi-stage workflows (build → review → deploy) are sequences of governed transitions. TA's workflow engine orchestrates the sequence; it does not execute the stages — agents and adapters do.

**Boundary**: The workflow engine handles stage ordering, dependency resolution, verdict scoring, failure routing, and human-in-the-loop gates. It does not contain domain logic. Stage execution is delegated to agents (via roles) or adapters (via configured commands).

### External Channels (Slack, Discord, Email) — In Scope

**Why**: TA's governance is only useful if humans can interact with it where they already work. Channels are delivery adapters — they render TA's events and collect human responses. They don't add governance logic.

**Boundary**: Channel adapters implement `deliver_question()` and `render_event()`. Response handling flows through the standard HTTP API (`POST /api/interactions/:id/respond`). No channel-specific business logic.

---

## Out of Scope (Belongs in Projects on Top)

These are not part of TA core. They are independent projects that consume TA's extension points (see ADR-product-concept-model.md):

| Capability | Why it's out of scope | Where it belongs |
|------------|----------------------|------------------|
| Role definitions & scheduling | Domain-specific orchestration logic | Virtual Office Runtime |
| Infrastructure provisioning | Domain-specific resource management | Autonomous Infra Ops |
| LLM prompt engineering | Agent framework concern | Agent configs / system prompts |
| Code generation | Agent capability, not governance | The agent itself |
| Dependency management | Build tool concern | npm, cargo, pip, etc. |
| Deployment orchestration | CI/CD concern | GitHub Actions, ArgoCD, etc. |
| Database migrations | Framework concern | ORM / migration tool |
| Compliance rule authoring | Regulatory domain expertise | Compliance add-on project |

---

## The Plugin Line

TA's extension model has two layers:

**Adapters** extend TA's core by implementing a trait for a new system:
- `SourceAdapter` (Git, SVN, Perforce) — VCS operations
- `BuildAdapter` (Cargo, npm, script, webhook) — build verification
- `ChannelAdapter` (CLI, web, Slack, Discord, email) — human interaction
- `ResourceMediator` (fs, email, db, api) — staging for any resource type

Adapters are governed — they operate within TA's policy engine and emit events through TA's event system.

**Plugins** observe and advise but cannot bypass policy:
- Advisors (pre-execution recommendations)
- Reviewers (draft review assistance)
- Auditors (post-execution analysis)
- Optimizers (longitudinal improvements)

See `docs/plugins-architecture-guidance.md` for the full plugin event model.

---

## Design Principles

1. **Invisible to agents.** Agents work in staging using their native tools. They don't know TA exists. TA diffs the result and creates drafts.

2. **Adapters, not implementations.** TA calls external tools through adapter traits. Swapping Git for Perforce, or Cargo for npm, is a config change.

3. **Events, not side effects.** Every transition emits a structured event. Workflows gate on events. Channels deliver events. Audit logs record events. Nothing happens silently.

4. **Human gates, not human labor.** TA pauses at decision points, not at every step. The goal is less human attention per unit of work, not zero human attention.

5. **Config over code.** Workflows, agent profiles, build commands, and channel routing are YAML/TOML configuration. Users customize TA by editing config files, not by writing Rust.

6. **Templates as starting points.** TA ships default templates for workflows, agent profiles, and build configs. Users override per-project. Community publishes to registries.

---

## Summary: Why TA Needs Each Feature

| Feature | Lifecycle transition governed | Without TA |
|---------|------------------------------|------------|
| `ta run` | Goal → agent work → draft | Human babysits agent, manually reviews output |
| `ta draft` | Draft → review → approve/deny → apply | Human manually diffs files, copies them by hand |
| `ta sync` | Feature branch → main current | Human manually merges PR, pulls main |
| `ta build` | Code changed → build verified | Human manually runs build, mentally tracks result |
| `ta release` | Build verified → version cut → published | Human manually bumps version, writes changelog, tags |
| `ta plan` | Product doc → phased development plan | Human manually writes plan or runs agent ad-hoc |
| Interactive mode | Agent paused → human answers → agent continues | Agent runs to completion blind, or human babysits |
| Workflows | Multi-stage pipeline with gates | Human manually sequences steps, tracks state |
| Channels | Event → human notified where they work | Human must watch terminal or check dashboard |
| Policy | Agent action → policy check → allow/deny/escalate | Human trusts agent blindly or reviews everything |
| Audit | Every transition → tamper-evident log | No record beyond chat history |
