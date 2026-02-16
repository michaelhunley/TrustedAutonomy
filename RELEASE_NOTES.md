# Release Notes

## v0.2.4-alpha

### New Features

- Add per-target "what I did" summaries

### Improvements

- updated terminology for better user clarity and proper product framing

### Bug Fixes

- Fix draft view display bugs + revise roadmap for MCP interception architecture
- resolved terminology update status
- Fix cargo fmt formatting in explanation.rs and output_adapters/mod.rs
- Smart conflict auto-resolve: detect phantom artifacts from dirty working tree
- Fix clippy errors in v0.2.3 output adapters
- Remove interactive ta-run from release script, fix crates.io publish

### Other Changes

- v0.2.4 — Terminology & Positioning Pass
- v0.2.3 — Tiered Diff Explanations & Output Adapters
- Make RELEASE_NOTES.md persistent with prepend-on-release
- Added detailed plan for MCP integration in place of connectors for tasks like email so we don't reinvent the wheel, and plan for strong network abstraction to catch unexpected state change network requests in v0.7
---

Full changelog: https://github.com/trustedautonomy/ta/compare/v0.2.2-alpha...v0.2.4-alpha

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
