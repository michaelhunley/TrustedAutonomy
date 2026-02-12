# Claude Code Project Instructions

## Build Environment

Nix provides the Rust toolchain. **Always prefix cargo/just commands with the nix wrapper:**

```bash
export PATH="/nix/var/nix/profiles/default/bin:$HOME/.nix-profile/bin:$PATH"
nix develop --command bash -c "COMMAND_HERE"
```

Or use the helper script for one-liners:
```bash
./dev cargo test --workspace
./dev cargo clippy --workspace --all-targets -- -D warnings
./dev just verify
```

## Verification Before Every Commit

Run these four checks (all must pass):
```bash
./dev cargo build --workspace
./dev cargo test --workspace
./dev cargo clippy --workspace --all-targets -- -D warnings
./dev cargo fmt --all -- --check
```

## Rules

- Never disable or skip tests
- Run tests after every code change, before committing
- Commit in logical working units
- All work stays within ~/development/TrustedAutonomy/
- Use `tempfile::tempdir()` for all test fixtures that need filesystem access

## Current State

- **158 tests passing** across 12 crates (run `ta plan list` for full status)
- See **PLAN.md** for the canonical development roadmap with per-phase status
- `ta plan list` / `ta plan status` show current progress
- Goals can link to plan phases: `ta run "title" --source . --phase 4b`
- `ta pr apply` auto-updates PLAN.md when a phase completes

### How It Works (Phase 3 Overlay Flow)
1. `ta goal start "title" --source . --phase 4b` → copies project to `.ta/staging/`
2. `ta run "title" --source . --phase 4b` → creates goal + injects CLAUDE.md (with plan context) + launches agent + builds PR on exit
3. Agent works normally in staging copy — TA is invisible to the agent
4. `ta pr build --latest` → diffs staging vs source → creates PRPackage with artifacts
5. `ta pr view/approve/deny <id>` → review workflow
6. `ta pr apply <id> --git-commit` → copies changes back to source + updates PLAN.md + optional git commit

### Key Types
- **Artifact.resource_uri**: `"fs://workspace/<path>"` — URI-based identity for all changes
- **PatchSet.target_uri**: Same URI scheme for external resources (gmail://, drive://, etc.)
- **PRStatus**: Draft → PendingReview → Approved/Denied → Applied (currently package-level only)
- **GoalRunState**: Created → Configured → Running → PrReady → UnderReview → Approved → Applied → Completed
- **GoalRun.plan_phase**: Optional link to a PLAN.md phase (e.g., "4b")
- **CLAUDE.md injection**: `ta run` prepends TA context + plan progress, saves backup, restores before diff