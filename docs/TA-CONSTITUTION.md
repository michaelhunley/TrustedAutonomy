# TA Constitution

> The canonical behavioral contract for Trusted Autonomy.
> Every command, subsystem, and integration must adhere to these rules.
> Pre-release reviews validate conformance against this document.

**Last updated**: v0.10.18-alpha
**Status**: Living document — update when behavior changes.

---

## 1. Core Principles

### 1.1 Agent Invisibility
TA is invisible to the agent. The agent works in a staging copy using native tools (editor, compiler, test runner). It sees a normal project — not a sandboxed environment. TA mediates through injection (CLAUDE.md, settings) and observation (diffs), never by intercepting agent commands.

### 1.2 Default-Deny
All agent actions are denied unless explicitly granted. The policy engine evaluates every request against a capability manifest. No manifest = no access. Expired manifest = no access. Unknown verb = deny.

### 1.3 Human-in-the-Loop
Irreversible side effects always require human approval. The verbs `apply`, `commit`, `send`, and `post` are hardcoded as approval-required regardless of grants. The agent may propose; only the human (or a constitutional auto-approval policy) may execute.

### 1.4 Observable & Actionable
Every outcome must be observable (logged with details) and actionable (user knows what to do next). No silent failures. No bare "Error" messages. Every error path includes: what happened, what was being attempted, and what the user can do about it.

### 1.5 Append-Only Audit
All significant actions are recorded in an append-only, hash-chained audit log. Each event links to the previous via `previous_hash`. The chain is verifiable via `ta audit verify`. No event may be deleted or modified after write.

---

## 2. VCS & Branch Management

### 2.1 Feature Branch Isolation
All TA-generated changes land on feature branches, never directly on the user's current branch or `main`. Branch naming convention: `ta/<sanitized-goal-title>` (truncated to 50 characters).

### 2.2 Branch Restoration Guarantee
`ta draft apply` MUST restore the user's original branch after completion, regardless of success or failure. The sequence is:
1. Save current branch via `adapter.save_state()`
2. Create feature branch, commit, push, open review
3. Restore original branch via `adapter.restore_state()`
4. Step 3 executes even if step 2 fails partially

Violation of this rule leaves the user on an unexpected branch with no indication of how to recover.

### 2.3 Submit Abstraction
VCS operations use three abstract stages, not git-specific terminology:
- **Stage**: Prepare changes for submission (git: branch + commit; p4: changelist; svn: implicit)
- **Submit**: Send to remote (git: push; p4: shelve/submit; svn: commit)
- **Review**: Request human review (git: PR; p4: review; svn: email/external)

CLI flags use `--submit`/`--no-submit` and `--review`/`--no-review`. Legacy `--git-commit`/`--git-push` are backward-compat aliases.

### 2.4 Default Submit Behavior
When a VCS adapter is configured (`[submit].adapter != "none"`), `ta draft apply` defaults to running the full submit workflow (stage + submit + review). The user must explicitly pass `--no-submit` to skip. Plain `ta draft apply <id>` does the right thing.

### 2.5 Commit Message Safety
Goal titles, draft summaries, and other user/AI-generated text MUST be sanitized before use in VCS commit messages or branch names. No shell interpolation — use direct argument passing. Special characters (backticks, single quotes, newlines) must be escaped or removed.

---

## 3. Staging & Overlay

### 3.1 Full Copy Model (V1)
Staging creates a complete copy of the source project in `.ta/staging/<goal-id>/`. The agent works in this copy. Diffs are computed by comparing staging to source.

### 3.2 Infrastructure Exclusion
The following directories are ALWAYS excluded from staging copies and diffs, regardless of `.taignore` configuration:
- `.ta/` — TA state and configuration
- `.claude-flow/` — Agent framework state
- `.hive-mind/` — Agent framework state
- `.swarm/` — Agent framework state

This prevents TA internal state from leaking into agent workspaces or draft artifacts.

### 3.3 Binary Detection
Files with null bytes in the first 8KB are classified as binary. Binary files appear in diffs with size summaries, not content. Both `overlay.rs` and `draft.rs` use this heuristic consistently.

### 3.4 Staging Cleanup
Staging directories for completed/applied goals should be cleaned up. `ta goal gc --include-staging` removes stale staging. Applied goals should auto-clean staging on successful apply (configurable, default: on).

---

## 4. CLAUDE.md Injection & Cleanup

### 4.1 Injection Content
`ta run` injects the following into the staging copy before launching the agent:
- `CLAUDE.md` — plan context, memory context, goal objective, interactive mode sections
- `.claude/settings.local.json` — TA-specific tool permissions
- `.mcp.json` — MCP server routing for TA tools

### 4.2 Backup Before Injection
Before modifying any file, the original content is saved as a backup. If the file did not exist, this is recorded so it can be deleted during cleanup.

### 4.3 Cleanup Guarantee
ALL injected content MUST be removed before:
- Computing diffs (`ta draft build`)
- Any early return or error exit from `ta run`
- Follow-up re-injection (restore original first, then inject fresh)

**Invariant**: No injected content appears in diffs, draft artifacts, or commits. The agent's changes are the only things captured.

### 4.4 Follow-Up Re-Injection
When a follow-up goal reuses the parent's staging, CLAUDE.md must be restored from backup before re-injecting. This prevents stale or nested injection content.

---

## 5. Goal Lifecycle

### 5.1 State Machine
Valid states and transitions:

```
Created → Configured → Running → PrReady → UnderReview → Approved → Applied → Completed

Running ↔ AwaitingInput  (interactive mode)
UnderReview → Running    (denied draft, retry)
PrReady → Running        (macro goal inner loop)
Any state → Failed       (always valid)
```

### 5.2 No State Skipping
Transitions that skip intermediate states are rejected. `Created → Running` is invalid (must go through `Configured`). The `transition()` method validates all state changes.

### 5.3 Failure Always Allowed
Any state may transition to `Failed`. This ensures crashed agents, timeouts, and user cancellations can always be recorded.

### 5.4 Goal Process Liveness
A goal in `Running` state must have a live agent process. If the process exits without updating state, the daemon should detect this and transition to `Completed` (exit 0) or `Failed` (non-zero).

### 5.5 Zombie Prevention
Goals stuck in `Running` with no live process are zombies. `ta goal gc` should detect and offer to transition them. Goals dispatched via daemon should have configurable timeouts.

---

## 6. Policy Engine

### 6.1 Evaluation Order
1. Agent has capability manifest? → No → **Deny**
2. Manifest expired? → Yes → **Deny**
3. Path traversal in resource URI? → Yes → **Deny**
4. Verb in approval-required list? → **RequireApproval** (even with matching grant)
5. Matching grant exists? → **Allow**
6. No match → **Deny**

### 6.2 Approval-Required Verbs
`apply`, `commit`, `send`, `post` — these represent irreversible side effects and ALWAYS require human approval regardless of grants.

### 6.3 Path Traversal Guard
Resource URIs containing `..` or absolute paths outside the workspace are rejected. Agents must not escape the staging directory.

### 6.4 Supervised Mode
When security level is `Supervised`, only read verbs (`read`, `list`, `diff`, `status`, `search`) are allowed without approval. All other verbs require approval.

### 6.5 Fail Closed
Invalid glob patterns in grants never match. The system fails closed (deny) rather than open (allow).

---

## 7. Audit & Compliance

### 7.1 Append-Only Writes
The audit log is opened in append mode. Writes are flushed after each event to ensure OS-level durability.

### 7.2 Hash Chain Integrity
Each `AuditEvent` includes a `previous_hash` field linking to the prior event. This forms a tamper-evident chain. `ta audit verify` validates chain integrity.

### 7.3 Tracked Actions
All of the following produce audit events:
- `ToolCall` — MCP tool invocation
- `PolicyDecision` — policy engine evaluation
- `Approval` — human approval action
- `Apply` — changes applied to target
- `Error` — error during processing
- `AutoApproval` — draft auto-approved by policy

### 7.4 Terminal Transition Auditing
Every path that ends a goal's lifecycle MUST write an audit record: apply, deny, close, delete, gc, timeout, agent crash. No goal data should be removed without a trace.

---

## 8. Drift Detection

### 8.1 Behavioral Baseline
Drift is measured against the agent's historical behavior across five signals:
- **ResourceScope** — URIs outside historical pattern
- **EscalationFrequency** — change in policy escalation rate
- **RejectionRate** — change in draft denial rate
- **ChangeVolume** — unexpectedly large/small diffs
- **DependencyPattern** — unusual external dependency additions
- **ConstitutionViolation** — undeclared access per access constitution

### 8.2 Severity Levels
- **Normal** — within historical variance
- **Warning** — notable deviation (20% rate delta, 2x volume factor)
- **Alert** — significant deviation (50% rate delta, 3x volume factor)

Constitution violations are always Warning or higher.

---

## 9. Shell & Daemon Trust Model

### 9.1 Shell as Thin Client
`ta shell` is a stateless REPL and renderer. It has no direct file access, no policy enforcement, no business logic. All authority lives in the daemon.

### 9.2 Daemon Mediates All Writes
The agent (and shell) propose actions. The daemon evaluates policy, records audit events, and mediates execution. The agent never writes directly to the source project — all changes flow through staging → diff → draft → apply.

### 9.3 Daemon Auto-Start
If `ta shell` cannot reach the daemon, it MUST auto-start via `daemon::ensure_running()`. The user should never have to manually start the daemon to use the shell. If the daemon is still unreachable after auto-start, shell fails with a clear error.

### 9.4 Daemon Version Guard
If the running daemon version does not match the CLI version, the shell MUST auto-restart the daemon to ensure version parity. The CLI and daemon are tightly coupled — running mismatched versions leads to silent failures, missing features, and protocol incompatibilities. The restart happens automatically before entering the shell; the user sees the version transition in startup output.

### 9.5 Agent Read-Only Inspection
The agent can read daemon state (goal status, draft details, plan progress, logs) through MCP tools or daemon API. It cannot mutate state without daemon mediation and policy evaluation.

---

## 10. Draft Lifecycle

### 10.1 Draft States
```
Draft → PendingReview → Approved { approved_by, approved_at }
                      → Denied { reason, denied_by }

Approved → Applied { applied_at }
         → Superseded { superseded_by }

Any non-terminal → Closed
```

### 10.2 Supersession Rules
- **Same staging follow-up**: New draft auto-supersedes parent draft (same workspace, cumulative changes)
- **Different staging follow-up**: Drafts are independent — no auto-supersession
- Superseded drafts cannot be applied or re-reviewed

### 10.3 Apply Idempotence
`ta draft apply` copies artifacts from the draft package to the source project. If the source has diverged, conflict detection identifies phantom artifacts (changed in source since staging snapshot). The user must resolve conflicts before apply proceeds.

### 10.4 Draft Amend (planned)
A lightweight follow-up that works with an existing feature branch rather than creating new staging. Amends the draft with additional changes without full staging copy overhead.

---

## 11. Plugin Architecture

### 11.1 Plugin Types
- **Channel plugins**: Deliver agent questions to external systems (Discord, Slack, email). JSON-over-stdio protocol.
- **Submit plugins**: VCS adapters for non-built-in systems. Named `ta-submit-<name>`.
- **Data write plugins**: Audit storage backends (database, cloud storage).

### 11.2 Plugin Discovery
Plugins are executables in `~/.ta/plugins/` or project-local `.ta/plugins/`. Named by convention: `ta-<type>-<name>`.

### 11.3 Plugin Isolation
Plugins run as separate processes. They communicate via stdio (channel plugins) or CLI protocol (submit plugins). A misbehaving plugin cannot corrupt TA state.

### 11.4 macOS Code Signing
On macOS, plugin binaries must be re-signed with `codesign --force --sign -` after copying to prevent AppleSystemPolicy from blocking execution.

---

## 12. Build & Test Environment

### 12.1 Nix Toolchain
All cargo commands run inside the Nix devShell. Use `./dev "command"` or `nix develop --command bash -c "command"`.

### 12.2 Pre-Commit Verification
Four checks must pass before every commit:
1. `cargo build --workspace`
2. `cargo test --workspace`
3. `cargo clippy --workspace --all-targets -- -D warnings`
4. `cargo fmt --all -- --check`

### 12.3 Test Fixtures
All tests requiring filesystem access use `tempfile::tempdir()`. No hardcoded paths. No test pollution across runs.

### 12.4 Platform Parity
Tests must pass on macOS, Linux, and Windows CI. Platform-specific tests use `#[cfg(unix)]` / `#[cfg(windows)]` with appropriate implementations for each.

---

## 13. Error Handling

### 13.1 Structured Errors
Error messages include:
- **What happened**: The specific failure
- **What was being attempted**: The operation context
- **What to do**: Next steps for the user

### 13.2 Timeout Reporting
Timeout errors state: which operation, the timeout duration, and how to configure it.

### 13.3 CLI Confirmation
Commands confirm what they did, not just succeed silently. Include counts, paths, IDs, and durations where relevant.

### 13.4 Logging
Use `tracing::warn`/`tracing::error` for operational issues. Include structured fields (command, duration, path), not just string messages.

---

## 14. Autonomous Operations & Self-Healing

### 14.1 Detection Without Mutation
The daemon watchdog may detect issues (dead processes, low disk, crashed plugins) continuously and without human consent. Detection is read-only observation — no state is changed.

### 14.2 Corrective Action Approval
All corrective actions are proposals. The daemon presents the issue, diagnosis, and proposed fix. The user approves or denies. No corrective mutation happens without consent, unless covered by auto-heal policy.

### 14.3 Auto-Heal Policy Scope
Auto-heal is opt-in and explicitly scoped. Only actions listed in `[operations.auto_heal].allowed` may execute without approval. The allowed list must be conservative — only low-risk, reversible actions qualify (restart plugin, mark zombie failed, clean applied staging). High-risk actions (delete goal, kill process, gc drafts) always require approval.

### 14.4 Diagnostic Goals Are Read-Only
Diagnostic goals spawned by the daemon for issue investigation have read-only access. They produce reports, not changes. The policy engine enforces this via read-only capability manifests with no write/apply grants.

### 14.5 Corrective Action Audit
Every corrective action — whether auto-healed or human-approved — produces an audit event with: what was detected, what was proposed, who/what approved (human or auto-heal policy), and the outcome. The audit trail must be as complete for automated operations as for human-initiated ones.

### 14.6 Escalation Path
If a corrective action fails, or if the daemon detects an issue it cannot diagnose, it escalates to the user via all configured channels. Auto-heal never retries a failed corrective action — it escalates instead.

### 14.7 Runbook Transparency
Operational runbooks execute step-by-step with each step visible to the user. The user can interrupt, modify, or cancel at any step. Runbooks do not execute as opaque batches.

---

## 15. VCS Submit Invariant

### 15.1 Isolation Before Commit
All VCS adapters MUST route agent-produced changes through an isolation mechanism (branch, shelved CL, patch queue) before any commit. `prepare()` is the mandatory enforcement point — failure is always a hard abort, never a warning.

### 15.2 Protected Target Guard
After `prepare()`, the adapter MUST NOT be positioned to commit directly to a protected target. Adapters declare their protected targets via `protected_submit_targets()`. The default protected targets for Git are `main`, `master`, `trunk`, and `dev`. Each adapter may extend or override this list via configuration.

### 15.3 Verification Is Adapter-Owned
Adapters implement `verify_not_on_protected_target()` to assert the post-`prepare()` invariant. The draft apply pipeline calls this method after `prepare()` — regardless of adapter type — via the `SourceAdapter` trait. No adapter-specific special-casing in the pipeline.

### 15.4 Invariant Applies to Plugins
This invariant applies to all adapters: built-in (Git, Perforce, SVN) and plugin-supplied. Plugin adapters MUST implement `protected_targets` and `verify_target` messages in the JSON-over-stdio protocol. A plugin that omits these MUST return an empty protected targets list and document this explicitly.

### 15.5 Failure Mode
A `prepare()` failure or protected-target guard violation produces an error with:
- What was being attempted (commit on branch X)
- Why it failed (prepare error or protected branch match)
- What the user can do (clean working tree, check branch state, re-run)

Silent continuation after a `prepare()` failure is a critical violation of this invariant.

---

## 16. Constitution Review Architecture

### 16.1 Static Checking Is Language- and Project-Specific
Static constitution checkers (like the §4 injection/cleanup scanner in TA's own build pipeline) are valid only for the specific project and language they were written for. A checker for TA's Rust codebase MUST NOT run against Python, JavaScript, C++, non-code drafts (emails, posts, documents), or any project with different conventions. The TA core pipeline MUST NOT embed language- or project-specific static checks that silently no-op or mislead on unrelated projects.

### 16.2 Three Review Modes
TA supports three constitution review modes, each suited to different verification needs:

**`agent-constitution-draft-review`** — Hooks into the draft review process. An AI agent reads the project's constitution (`.ta/constitution.md`) and the draft's artifacts and diff, then produces structured findings (violations, concerns, confirmations). Non-blocking by default; can be configured to block approval on high-severity findings. Best for per-change review.

**`agent-constitution-project-review`** — Runs against the full project state, not just a single draft. Invoked on-demand (`ta constitution review --project`) or hooked into the release pipeline. The agent reads the constitution, the project plan, recent git history, and a configurable sample of source files, then reports systemic adherence or drift. Best for periodic health checks and pre-release gates.

**`agent-build-constitution-static-checker`** — An agent that reads the project's constitution and codebase context, then *generates* a project-specific static checker. The output is a human-readable, human-editable checker definition (patterns, rules, assertions) that integrates with `ta draft build` as a plugin. This is how TA's own §4 scanner was conceptually derived — but with the right tool, the derivation is automated and any project can get a tailored checker. Humans can review and modify the generated checker before it is activated.

### 16.3 Project Constitution
Each project using TA SHOULD maintain a project constitution at `.ta/constitution.md`. A project constitution declares the behavioral invariants for that project's agents — what they may inject, how they must clean up, what targets are protected, what content policies apply. TA ships a generator (`ta constitution init`) that drafts a starter constitution from the project's plan and stated objectives. The TA constitution itself (`docs/TA-CONSTITUTION.md`) serves as the reference implementation.

### 16.4 Hook Points
Constitution review integrates through TA's hook system:
- `draft-build-post` — run `agent-constitution-draft-review` after a draft is built
- `draft-approve-pre` — block approval if draft review found blocking findings
- `release-pre` — run `agent-constitution-project-review` before cutting a release
- `constitution-check` — standalone `ta constitution check` command for CI integration

Hooks are configured in `.ta/workflow.toml` and are opt-in. Default: no constitution review hooks are active.

### 16.5 Separation of Concerns
The access constitution (which files the agent may touch, governed by `ta-policy`) is separate from the behavioral constitution (what the agent may do within those files, governed by §16). Both serve human oversight, but through different mechanisms: access is enforced by the policy engine at capability-grant time; behavior is reviewed by agents or static checkers after the fact.

### 16.6 No Project-Specific Logic in TA Core Commands
TA core commands MUST NOT embed logic that is specific to TA's own codebase, language (Rust), naming conventions, or internal patterns. This includes static analyzers, pattern matchers, heuristics, or rules that were derived from observing TA's own source code. Examples of what is prohibited:

- Scanning `.rs` files for `inject_*/restore_*` patterns in the generic draft build pipeline
- Hardcoding TA-internal function name conventions as universal rules
- Skipping non-`.rs` files with an implicit "nothing to check" result on other languages

Such logic belongs exclusively in a project-specific constitution checker plugin, scoped to the project that needs it. For TA's own development, this means the §4 injection/cleanup scanner runs as a TA-project plugin, not as part of the generic `ta draft build` command. Any project using TA can have its own checker — generated or authored — but that checker MUST NOT ship inside TA core.

**Rationale**: TA is a substrate for any project, in any language, producing any kind of output. Embedding TA-specific heuristics in core commands produces false positives (TA patterns flagged in unrelated projects), false negatives (non-Rust projects get no checking at all), and violates the principle that TA is invisible to the projects it mediates.

---

## Appendix: Constitution Compliance Checklist

For pre-release review, verify each command against these rules:

| Command | Key Rules |
|---------|-----------|
| `ta run` | 4.1-4.4 (injection/cleanup), 5.1-5.2 (state machine) |
| `ta draft build` | 4.3 (cleanup before diff), 3.2 (infrastructure exclusion) |
| `ta draft apply` | 2.1-2.2 (branch isolation + restoration), 2.4 (default submit), 7.3 (audit) |
| `ta draft deny` | 7.4 (terminal audit), 10.1 (state transition) |
| `ta goal start` | 3.1 (staging copy), 5.1 (Created → Configured) |
| `ta goal delete` | 7.4 (terminal audit) |
| `ta goal gc` | 5.5 (zombie detection), 7.4 (terminal audit) |
| `ta shell` | 9.1-9.5 (thin client, daemon mediates, auto-start, version guard) |
| `ta plan *` | 9.4 (read-only agent inspection) |
| `ta audit verify` | 7.2 (hash chain validation) |
| Plugins | 11.2-11.4 (discovery, isolation, signing), 15.4 (VCS submit invariant) |
| Watchdog | 14.1 (detection without mutation), 14.5 (audit) |
| Auto-heal | 14.2-14.3 (approval, scoped policy), 14.6 (escalation) |
| Diagnostic goals | 14.4 (read-only), 6.1-6.5 (policy enforcement) |
| Runbooks | 14.7 (step-by-step transparency), 14.2 (approval per step) |
| `ta status` | 13.3 (confirmation), 14.1 (surfaces watchdog findings) |
| `ta draft apply --submit` | 15.1-15.5 (VCS submit invariant, isolation, guard) |
| `ta constitution check` | 16.4 (hook points), 16.2 (review modes) |
| Constitution review hooks | 16.3-16.5 (project constitution, hooks, separation of concerns) |
| Static checkers (project) | 16.1 (language/project specificity), 16.2 (build mode) |
| TA core commands | 16.6 (no TA-specific logic in generic commands) |
