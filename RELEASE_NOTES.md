# Trusted Autonomy v0.2.2-alpha Release Notes

## New Features

- **External Diff Routing**: Route your changes to external diff tools and review workflows. Trusted Autonomy now integrates seamlessly with your preferred diff viewer or approval pipeline.

- **Concurrent Session Conflict Detection**: Work on multiple goals simultaneously without stepping on your own toes. TA now detects when changes from different sessions would overlap and prevents accidental conflicts.

- **Workflow Configuration**: Customize your review workflow with `workflow.toml`. Control auto-commit behavior, auto-push settings, and use custom PR templates to standardize your change proposals.

- **Follow-Up Goals**: Create iterative goals that build on previous work. Track dependencies between related changes and manage complex, multi-stage tasks.

- **Release Automation**: Streamlined release process with automated release notes generation and configurable pipelines (Phase v0.3.2 planned).

## Improvements

- **Smarter Conflict Detection**: TA now only aborts when there are real overlapping conflicts, not just any change to the source tree. Build artifacts and unrelated changes no longer trigger false alarms.

- **Richer Context**: Commit messages and PR bodies now include full goal context, making it easier to understand why changes were made and how they relate to your objectives.

- **Better PR Review**: The PR view now filters out build artifacts (like `target/`) so you see only the changes that matter.

- **macOS Compatibility**: Improved cross-platform support including BSD sed compatibility and better macOS build tooling.

- **Documentation Overhaul**: Clearer setup guides, workflow examples, and updated architecture documentation to help you get started faster.

## Bug Fixes

- Fixed false conflict warnings triggered by build artifacts during change detection.
- Resolved macOS cross-compile issues and removed Nix dependencies from release builds.
- Fixed release workflow permissions and updated deprecated GitHub Actions.
- Corrected various code quality issues (clippy lints) for more reliable operation.

---

**Note**: This is an alpha release. Some features are still experimental. See the [PLAN.md](PLAN.md) for the development roadmap and upcoming features.
