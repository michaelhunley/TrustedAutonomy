# Trusted Autonomy (Rust) — Development Plan (MVP → Proof → Product)

> Objective: Build a local-first agent substrate that increases autonomy while ensuring all side effects are staged and reviewable via PR packages.
> Key invariant: **capability default deny** at the gateway; **mutation default collect** (staged-by-default) across FS/DB/email/social.

---

## 0) Repo layout (recommended)

```
trusted-autonomy/
  crates/
    ta-daemon/              # long-running local service (HTTP/gRPC + MCP)
    ta-mcp-gateway/         # MCP gateway/router + authn/authz hooks
    ta-policy/              # constitutions, capability manifests, policy evaluation
    ta-audit/               # append-only event log + artifact hashing
    ta-workspace/           # git worktrees + sparse checkout manager
    ta-changeset/           # ChangeSet + preview/diff abstractions (FS + external)
    ta-sandbox/             # just-bash integration + allowlisted exec
    ta-connectors/
      fs/                   # filesystem connector
      web/                  # web fetch connector (sanitized)
      mock-drive/           # mock staging connector (Phase 1–3)
      mock-gmail/           # mock staging connector
  apps/
    ta-cli/                 # CLI for goals, PR review, approvals
  schemas/
    pr_package.schema.json  # PR package schema (existing)
    capability.schema.json  # capability manifest schema (create)
    agent_setup.schema.json # agent setup proposal schema (create)
  docs/
    architecture.md
    threat-model.md
```

---

## 1) Core concepts (data model)

### 1.1 Resource URIs
- `fs://workspace/<path>`
- `web://<url>` (stored as normalized URL + cache key)
- `drive://...` / `gmail://...` / `social://...` (mock in early phases)

### 1.2 ChangeSet (staged mutation)
A ChangeSet is the universal “pending review” unit.
- `changeset_id`
- `target_uri`
- `kind`: `fs_patch | db_patch | email_draft | social_draft | other`
- `preview_ref` (rendered diff/preview)
- `risk_flags[]`
- `commit_intent`: `none | request_commit | request_send | request_post`

### 1.3 Capability manifest (signed)
- issued per agent + goal iteration
- scoped: tool + verb + resource pattern(s)
- time-bounded, budgeted
- signed by policy compiler

### 1.4 PR package
Milestone deliverable: summary + rationale + ChangeSets + provenance + risk + requested approvals.

---

## 2) Phase 1 — Kernel + FS PR loop (ship first)

### Deliverables
- Local daemon (Rust) that hosts:
  - MCP gateway skeleton
  - policy module (static templates)
  - workspace manager (git worktrees + sparse checkout)
  - FS connector: read/write_patch/diff/commit
  - audit logger
  - PR package builder
- CLI:
  - `ta goal start`
  - `ta pr status`
  - `ta pr view`
  - `ta pr approve --commit fs`
  - `ta pr deny`

### Acceptance criteria
- Agent edits files in workspace (or via fs.write_patch) and produces a PR package.
- Human can review a diff and approve merge.
- No direct write-to-host outside workspace is possible.
- Every tool call is logged with hashes.

### Implementation tasks
1. `ta-workspace`
   - initialize repo/workspace root
   - create worktree per iteration: `worktrees/<goal>/<iter>`
   - optional sparse checkout config
2. `ta-audit`
   - append-only log (JSONL) + artifact hashing (sha256)
3. `ta-policy`
   - role templates (research/draft/ops)
   - evaluate(tool_call) → allow/deny/transform/require_approval
   - signed capability manifest (ed25519)
4. `ta-changeset`
   - represent FS changes as ChangeSets (diff ref to git diff)
   - binary change summarizer: type/size/hash/preview hooks
5. `ta-mcp-gateway`
   - request context: agent_id, constitution_id, manifest_id, goal/iter ids
   - enforce capability scope + stage-only invariants
6. `ta-connectors/fs`
   - `read/list/search`
   - `write_patch` (apply unified diff into worktree)
   - `diff` (git diff output + metadata)
   - `commit` (requires approval token)
7. `ta-daemon`
   - host MCP endpoints + internal HTTP endpoints for CLI
8. `ta-cli`
   - simple TUI/CLI for PR review and approvals

---

## 3) Phase 2 — Execution sandbox (just-bash integration)

### Deliverables
- `ta-sandbox` integrates just-bash-style execution for allowlisted commands.
- Commands operate only on the worktree; outputs are captured as artifacts.
- Command transcripts hashed into the audit log and referenced from PR package.

### Acceptance criteria
- Agent can run `rg`, `fmt`, `test` profiles (mapped to allowlisted commands).
- Attempts to access host FS outside worktree are blocked.
- Sandbox runs are reproducible enough for reviewers (inputs/outputs hashed).

### Implementation tasks
- Define command policy map:
  - `repo.search` → `rg`
  - `repo.format` → formatter(s)
  - `repo.test` → preconfigured test commands
- Add `exec` tool to MCP with strict allowlist + cwd enforcement.

---

## 4) Phase 3 — ChangeSet unification + binary previews

### Deliverables
- Unified ChangeSet model across FS + mock connectors.
- Binary preview pipeline:
  - images → thumbnail
  - PDFs → first-page render (optional)
  - unknown binaries → metadata only + risk scoring

### Acceptance criteria
- PR view shows both text diffs and binary summaries consistently.

---

## 5) Phase 4 — Intent→Access “internal planner” (Agent Setup PR)

### Deliverables
- LLM “Intent-to-Policy Planner” outputs an AgentSetupProposal (JSON).
- Deterministic Policy Compiler validates proposal:
  - subset of templates
  - staged semantics present
  - budgets applied
- Agent setup becomes an “Agent PR” requiring approval before activation.

### Acceptance criteria
- User goal → proposed agent roster + scoped capabilities + milestone plan.
- Proposal rejected if it requests commit/send/post without gating.
- Approved setup yields signed manifests used by gateway.

---

## 6) Phase 5 — First real external connector (choose one)

### Option A: Gmail staging
- read threads
- create draft (ChangeSet)
- send gated by approval token

### Option B: Drive staging
- read doc
- write_patch + diff preview
- commit gated

### Option C: DB staging
- write_patch as transaction log + preview
- commit gated

---

## 7) Distribution plan (simple → no-code → cloud)

### Developer (fast)
- `cargo run` + local config
- optional Nix for dev env reproducibility (recommended for CI)

### No-code local
- Desktop app later; for MVP provide an installer that bundles:
  - daemon binary
  - git
  - minimal helper tools (rg/jq)
- Keep FUSE/driver requirements out of v1

### Cloud
- OCI image for daemon + connectors
- ephemeral workspaces + centralized audit store
- optional stronger isolation for runtime

---

## 8) Security and testing (must-have)

### Tests
- policy tests: allow/deny/transform matrix
- capability scope tests: path traversal, URI normalization
- prompt-injection regression tests: ensure retrieved content cannot elevate permissions
- changeset tests: binary summaries correct and stable

### Threat model checkpoints
- Exfiltration attempts via tool outputs
- Side-effect attempts via “hidden” connector methods
- Supply-chain risks in connectors and sandbox commands

---

## 9) Definition of Done (MVP proof)

- PR-per-milestone is end-to-end functional for filesystem work.
- All mutations are staged by default and reviewable.
- Approvals are required for commit and any external side effect tool.
- Audit log supports replay linkage (tool trace hash + artifact hashes).
- Works on Windows/macOS/Linux without kernel drivers.
