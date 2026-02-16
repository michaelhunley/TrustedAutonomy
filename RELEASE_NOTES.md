# Release Notes

## v0.2.2-alpha

### New Features

- External diff routing — agent changes can be diffed and routed through configurable pipelines
- Concurrent session conflict detection — multiple agents working on the same source are detected and warned
- Selective approval with URI-aware pattern matching — approve, reject, or discuss specific files using glob patterns (`ta pr approve --approve "src/**" --reject "tests/**"`)
- Change summary ingestion — agents can produce `change_summary.json` for richer PR descriptions
- YAML agent launch configs — add new agent frameworks without code changes
- Terms acceptance gate — first-run disclaimer review before using TA
- Plan tracking — `ta plan list` and `ta plan status` show PLAN.md progress; `ta pr apply` auto-updates phases

### Improvements

- Simplified disclaimer to MIT-style "AS IS" + "NO LIABILITY" clauses
- Enriched git commit messages and PR bodies with full goal context
- Workflow configuration via `workflow.toml` with auto-approve/auto-commit settings
- Release automation with `scripts/release.sh` for one-command releases
- Cross-platform CI builds (macOS ARM + Intel, Linux x86_64 + ARM)

### Bug Fixes

- Fixed false conflicts from build artifacts in staging detection
- Fixed PR view filtering of `target/` directory artifacts
- Resolved macOS BSD sed compatibility in release tooling

---

Full changelog: https://github.com/michaelhunley/TrustedAutonomy/commits/v0.2.2-alpha
