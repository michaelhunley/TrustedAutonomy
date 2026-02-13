# Publishing to crates.io

## Current Status

### ta-cli (Primary Binary)

✅ **Ready for publishing** - All required metadata present:
- `name`: ta-cli
- `version`: 0.1.0-alpha
- `edition`: 2021
- `description`: "CLI for goals, PR review, and approvals in Trusted Autonomy"
- `license`: Apache-2.0
- `repository`: https://github.com/trustedautonomy/ta
- `homepage`: https://github.com/trustedautonomy/ta
- `keywords`: ai, agent, autonomy, review, staging
- `categories`: command-line-utilities, development-tools

### Library Crates

The following workspace crates have basic metadata but need additional fields before publishing:

**ta-audit**
- ✅ description, license
- ⚠️ Missing: repository, homepage

**ta-changeset**
- ✅ description, license
- ⚠️ Missing: repository, homepage

**ta-policy**
- ✅ description, license
- ⚠️ Missing: repository, homepage

**ta-workspace**
- ✅ description, license
- ⚠️ Missing: repository, homepage

**ta-goal**
- Status: Unknown (need to check)

**ta-mcp-gateway**
- Status: Unknown (need to check)

**ta-connectors/***
- Status: Unknown (need to check)

## Publishing Strategy

### Phase 1: ta-cli Only (v0.1.x)

For initial alpha releases, we **only publish ta-cli** to crates.io:
- Users can install via `cargo install ta-cli`
- Library crates remain private/workspace-only
- This simplifies the release process and API stability concerns

### Phase 2: Library Crates (v0.2.x+)

Once APIs stabilize, consider publishing library crates individually:
- Allows third-party integrations
- Enables custom connectors
- Requires stronger API stability guarantees

## Adding Metadata to All Crates

To prepare all crates for potential publishing, add these fields to each `Cargo.toml`:

```toml
[package]
# ... existing fields ...
repository = "https://github.com/trustedautonomy/ta"
homepage = "https://github.com/trustedautonomy/ta"
```

Optional but recommended:
```toml
keywords = ["autonomy", "agent", "review"]  # max 5, customize per crate
categories = ["development-tools"]  # see https://crates.io/categories
readme = "README.md"  # if crate has its own README
```

## Publishing Checklist

Before publishing any crate:

1. **Verify metadata** - All required fields present
2. **Add README** - Create crate-specific README.md if needed
3. **API review** - Ensure public API is stable and documented
4. **Version check** - Follow semver, alpha/beta tags if unstable
5. **Dry run** - Test with `cargo publish --dry-run -p <crate>`
6. **Documentation** - Run `cargo doc --no-deps -p <crate>` and review
7. **Examples** - Include usage examples in docs or examples/
8. **License files** - Ensure LICENSE file is included in package

## Workspace Publishing Order

If publishing multiple crates, follow dependency order:

1. `ta-audit` (no internal deps)
2. `ta-policy` (no internal deps)
3. `ta-changeset` (no internal deps)
4. `ta-workspace` (depends on ta-changeset)
5. `ta-goal` (depends on ta-changeset, ta-workspace)
6. `ta-mcp-gateway` (depends on ta-changeset, ta-policy)
7. `ta-connectors/*` (depend on ta-workspace, ta-changeset)
8. `ta-cli` (depends on everything)

## Automated Publishing

The `.github/workflows/release.yml` workflow automatically publishes `ta-cli` when a version tag is pushed:

```yaml
- name: Publish ta-cli to crates.io
  run: |
    cargo publish -p ta-cli --token ${{ secrets.CARGO_REGISTRY_TOKEN }}
```

For library crates, add similar steps in dependency order.

## Manual Publishing

If automated publishing fails or for library crates:

```bash
# 1. Verify package contents
cargo package --list -p ta-cli

# 2. Dry run (checks without publishing)
cargo publish --dry-run -p ta-cli

# 3. Publish to crates.io
cargo publish -p ta-cli --token $CARGO_REGISTRY_TOKEN
```

## Yanking a Release

If a critical bug is discovered after publishing:

```bash
# Yank the bad version (removes from default install)
cargo yank --vers 0.1.0 -p ta-cli

# Un-yank if issue was false alarm
cargo yank --undo --vers 0.1.0 -p ta-cli
```

**Note**: Yanking doesn't delete the version, just hides it from new installs. Always publish a fixed version.

## crates.io Token

The `CARGO_REGISTRY_TOKEN` secret is required for automated publishing:

1. Generate token at https://crates.io/me/settings/tokens
2. Add as GitHub secret: Settings → Secrets → Actions → New repository secret
3. Name: `CARGO_REGISTRY_TOKEN`
4. Value: Your token

## Verification After Publishing

1. **Check crates.io** - https://crates.io/crates/ta-cli
2. **Test installation** - `cargo install ta-cli --version 0.1.0-alpha`
3. **Verify docs** - https://docs.rs/ta-cli
4. **Update README** - Link to crates.io badge

## Troubleshooting

### "already uploaded" error
- Version already exists on crates.io
- Bump version and try again
- Cannot overwrite published versions

### "failed to verify" error
- Missing dependencies or build failures
- Run `cargo publish --dry-run` locally first
- Check that all workspace dependencies use `version` not `path`

### Documentation build failure
- Fix Rust doc warnings: `cargo doc --no-deps`
- docs.rs uses stable Rust, test compatibility
