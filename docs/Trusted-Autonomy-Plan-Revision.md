# Trusted Autonomy — Plan Revision for Implementation (Rust)

This plan revises the existing development roadmap to reflect PRs as a conceptual abstraction, sandbox-first execution, and one-click installation.

## Key Changes
- PRs aggregate ChangeSets across endpoints (not Git).
- All agents run in OCI sandboxes (gVisor locally).
- Internet access is mediated (Research Mode).
- Enterprise isolation via Kata Containers.

## Implementation Priorities
1. Refactor ChangeSet model (remove git assumptions).
2. Introduce GoalRun as top-level execution unit.
3. Implement sandbox Runner (WSL2/macOS/Linux).
4. Ensure all connectors emit ChangeSets.
5. Add Smart PR Review + Audit agents.
6. Prepare Goal-to-Rights Advisor (paid).

## README Updates
- Clarify sandbox model.
- Document security modes.
- Add one-click install steps.
- Separate OSS vs paid features.

This file is intended for autonomous coding agents.
