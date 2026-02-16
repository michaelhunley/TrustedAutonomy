# Release Notes: Trusted Autonomy v0.2.2-alpha

## New Features

**External Diff Routing**
Apply changes to resources outside the staging workspace. TA can now route diffs to external files using URI patterns, enabling goals that modify configuration files, documentation, or other resources beyond the project directory.

**Workflow Configuration**
Customize goal and PR behavior with `workflow.toml` configuration files. Control automatic commit creation, PR opening, and approval workflows per project or per goal.

**Follow-Up Goals**
Create iterative refinement workflows by launching new goals based on previous work. Enables multi-step development where each goal builds on the last, with full review at each stage.

**Concurrent Session Conflict Detection**
TA now detects when multiple goals would modify the same files and prevents overlapping changes. Only actual conflicts trigger warnings—normal source changes during a goal's lifetime no longer cause false positives.

**PR Template Configuration**
Customize pull request descriptions with `pr-template.md` files. Control how TA formats PR bodies when automatically creating pull requests from approved goals.

## Improvements

**Smarter Conflict Detection**
Build artifacts and temporary files no longer trigger false conflict warnings. TA uses the PR package's artifact list to check only relevant changes.

**Richer Commit Messages and PR Bodies**
Automatically includes full goal context (title, description, plan phase) in commit messages and pull request descriptions. Makes it easier to understand why changes were made when reviewing git history.

**Enhanced Documentation**
Consolidated release guides, clearer setup instructions, and improved README with architecture diagrams and usage examples.

**Simplified License Terms**
Updated disclaimer to MIT-style clauses for clarity.

## Bug Fixes

- Fixed macOS compatibility in release builds (removed Nix dependency, fixed BSD sed issues)
- Fixed PR view filtering to properly exclude build artifacts from diffs
- Resolved CI/CD pipeline issues with action permissions and deprecated dependencies
- Fixed various clippy lints for cleaner codebase

---

*Note: v0.2.2-alpha is an early preview release. Expect rough edges and breaking changes as we iterate toward v0.3 and beyond. See DISCLAIMER.md for full terms.*
