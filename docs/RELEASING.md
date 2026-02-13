# Release Process for Trusted Autonomy

This document describes how to create a new release of TA.

## Prerequisites

- Write access to the repository
- GitHub secrets configured (see Setup section below)
- All tests passing on main branch
- Changelog/release notes prepared

## Setup: Configure GitHub Secrets

Before creating your first release, configure the required GitHub secrets. This is a **one-time setup** per repository.

### Step 1: Get a crates.io API Token

1. Visit [crates.io](https://crates.io) and log in
2. Go to [Account Settings → API Tokens](https://crates.io/settings/tokens)
3. Click "New Token"
4. Give it a name (e.g., "TA GitHub Actions")
5. Select scopes: `publish-update` (allows publishing new versions)
6. Click "Create" and copy the token (shown only once!)

### Step 2: Add Secrets to GitHub

1. Go to your GitHub repository
2. Navigate to **Settings → Secrets and variables → Actions**
3. Click "New repository secret"
4. Add the following secrets:

**Required:**
- **Name:** `CARGO_REGISTRY_TOKEN`
- **Value:** The crates.io API token from Step 1
- **Purpose:** Allows GitHub Actions to publish to crates.io

**Optional:**
- **Name:** `CACHIX_AUTH_TOKEN`
- **Value:** Token from [Cachix](https://cachix.org) (if you want to cache Nix builds)
- **Purpose:** Speeds up Nix builds in CI by caching dependencies
- **Note:** Only needed if you're actively using Cachix for the project

### Step 3: Verify Setup

After adding secrets, they'll appear in the Secrets list (values are hidden). The release workflow will use them automatically when triggered by a version tag.

## Version Numbering

TA follows [Semantic Versioning](https://semver.org/):
- `0.1.0-alpha` → Pre-release alpha
- `0.1.0-beta` → Pre-release beta
- `0.1.0` → First stable release
- `0.1.1` → Patch release (bug fixes)
- `0.2.0` → Minor release (new features, backward compatible)
- `1.0.0` → Major release (breaking changes)

## Release Methods

### Method 1: Manual Release (Recommended for v0.1.x)

This method gives you full control and is simple to understand.

#### Step 1: Update Version Numbers

Update the version in all relevant `Cargo.toml` files:

```bash
# Main CLI version
apps/ta-cli/Cargo.toml

# Update flake.nix version if needed
flake.nix (line 40: version = "0.1.0-alpha")
```

**Example version bump (0.1.0-alpha → 0.1.0-beta):**

```toml
# apps/ta-cli/Cargo.toml
[package]
name = "ta-cli"
version = "0.1.0-beta"  # Changed from 0.1.0-alpha
```

#### Step 2: Update Cargo.lock

```bash
./dev cargo update -p ta-cli
```

#### Step 3: Verify Build

```bash
./dev cargo build --release --workspace
./dev cargo test --workspace
```

#### Step 4: Commit Version Bump

```bash
git checkout -b release/v0.1.0-beta
git add apps/ta-cli/Cargo.toml Cargo.lock flake.nix
git commit -m "chore: bump version to 0.1.0-beta

Prepare for beta release"
git push -u origin release/v0.1.0-beta
```

#### Step 5: Create and Merge PR

```bash
gh pr create --title "Release v0.1.0-beta" --body "## Release Checklist

- [x] Version bumped in Cargo.toml
- [x] Cargo.lock updated
- [x] Tests passing
- [x] Release notes prepared

## Changes in this release
- Feature 1
- Feature 2
- Bug fix 3"

# Wait for CI to pass, then merge
gh pr merge --squash
```

#### Step 6: Create Git Tag

```bash
# Pull merged main
git checkout main
git pull

# Create annotated tag
git tag -a v0.1.0-beta -m "Release v0.1.0-beta

Major changes:
- Feature 1
- Feature 2
- Bug fix 3

Full changelog: https://github.com/trustedautonomy/ta/releases/tag/v0.1.0-beta"

# Push tag to trigger release workflow
git push origin v0.1.0-beta
```

#### Step 7: Monitor Release Workflow

The tag push triggers `.github/workflows/release.yml`:

1. Watch the workflow in the Actions tab
2. Workflow builds binaries for 4 platforms:
   - macOS aarch64 (Apple Silicon)
   - macOS x86_64 (Intel)
   - Linux x86_64 (musl)
   - Linux aarch64 (musl)
3. Creates GitHub Release with artifacts
4. Publishes to crates.io

```bash
# Check release status
gh run watch

# View the created release
gh release view v0.1.0-beta
```

#### Step 8: Verify Release Artifacts

Download and test the release binaries:

```bash
# macOS Apple Silicon
curl -fsSL https://github.com/trustedautonomy/ta/releases/download/v0.1.0-beta/ta-v0.1.0-beta-aarch64-apple-darwin.tar.gz -o ta.tar.gz
tar xzf ta.tar.gz
./ta --version

# Verify checksum
curl -fsSL https://github.com/trustedautonomy/ta/releases/download/v0.1.0-beta/ta-v0.1.0-beta-aarch64-apple-darwin.tar.gz.sha256 | shasum -a 256 -c
```

#### Step 9: Test Install Script

```bash
# Test the install script downloads correctly
curl -fsSL https://raw.githubusercontent.com/trustedautonomy/ta/main/install.sh | sh

# Verify it installed the new version
ta --version
```

#### Step 10: Verify crates.io Publication

```bash
# Check if published (may take a few minutes)
cargo search ta-cli

# Try installing from crates.io
cargo install ta-cli --version 0.1.0-beta
```

### Method 2: Using cargo-release (Future)

For automating the release process in the future, consider using [cargo-release](https://github.com/crate-ci/cargo-release).

**Setup:**

```bash
# Install cargo-release
cargo install cargo-release

# Add release configuration to Cargo.toml
[workspace.metadata.release]
pre-release-commit-message = "chore: release {{version}}"
tag-message = "Release {{version}}"
tag-prefix = "v"
publish = false  # We handle publishing via GitHub Actions
```

**Usage:**

```bash
# Dry run to see what would happen
cargo release --workspace --dry-run

# Perform version bump and create tag
cargo release --workspace --execute
```

## Hotfix Releases

For urgent bug fixes on a released version:

1. Create branch from the release tag:
   ```bash
   git checkout -b hotfix/v0.1.1 v0.1.0
   ```

2. Make fix, test, and bump patch version (0.1.0 → 0.1.1)

3. Follow normal release process from Step 4 onward

## Post-Release Tasks

After a successful release:

1. **Announce the release:**
   - Update README.md if there are new features
   - Post announcement (Discord, Twitter, etc.)
   - Send to early adopter mailing list

2. **Update documentation:**
   - Ensure README reflects current version capabilities
   - Update any version-specific docs

3. **Monitor for issues:**
   - Watch GitHub issues for bug reports
   - Monitor install script failures
   - Check crates.io download stats

## Troubleshooting

### Release workflow failed

**Check the workflow logs:**
```bash
gh run list --workflow=release.yml
gh run view <run-id>
```

**Common issues:**
- `CARGO_REGISTRY_TOKEN` not set or expired
- Cross-compilation failure (Linux builds on macOS)
- Network timeout downloading dependencies

**Fix and re-run:**
```bash
# Delete the failed release and tag
gh release delete v0.1.0-beta --yes
git tag -d v0.1.0-beta
git push origin :refs/tags/v0.1.0-beta

# Fix the issue in a PR, merge, then create tag again
```

### Binary doesn't work on user's system

**Check target compatibility:**
- Linux musl binaries work on most Linux distributions
- macOS binaries require matching architecture (Intel vs Apple Silicon)
- Nix build provides universal fallback: `nix run github:trustedautonomy/ta`

### Install script fails

**Common causes:**
- Release artifacts not yet available (wait 2-3 minutes after tag push)
- GitHub API rate limiting (add auth token)
- Checksum file missing (workflow didn't complete)

**Debug:**
```bash
# Test download URL manually
curl -I https://github.com/trustedautonomy/ta/releases/download/v0.1.0-beta/ta-v0.1.0-beta-aarch64-apple-darwin.tar.gz
```

## Release Checklist Template

Copy this for each release:

```markdown
## Release Checklist: v0.X.Y

### Pre-Release
- [ ] All tests passing on main
- [ ] Version bumped in `apps/ta-cli/Cargo.toml`
- [ ] Version bumped in `flake.nix`
- [ ] `Cargo.lock` updated (`cargo update -p ta-cli`)
- [ ] Release notes drafted
- [ ] Breaking changes documented (if any)

### Release
- [ ] Release PR created and merged
- [ ] Git tag created and pushed
- [ ] GitHub Actions release workflow completed successfully
- [ ] GitHub Release page created with all 4 binaries + checksums
- [ ] crates.io publish successful

### Post-Release Verification
- [ ] Download and test macOS aarch64 binary
- [ ] Download and test macOS x86_64 binary
- [ ] Download and test Linux x86_64 binary
- [ ] Test install script: `curl -fsSL ... | sh`
- [ ] Verify `cargo install ta-cli` works
- [ ] Verify `nix run github:trustedautonomy/ta` works
- [ ] Smoke test: `ta run "task" --source .` → review → apply

### Post-Release Tasks
- [ ] Announcement posted
- [ ] Documentation updated
- [ ] Known issues documented
- [ ] Next milestone planned
```

## Version History

- `v0.1.0-alpha` — Initial alpha release
  - Core PR review workflow
  - Basic CLI commands
  - MCP gateway integration

(Update this section with each release)
