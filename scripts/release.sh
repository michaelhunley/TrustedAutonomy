#!/usr/bin/env bash
# scripts/release.sh — Automate the TA release process.
#
# Usage:
#   ./scripts/release.sh <version>
#
# Examples:
#   ./scripts/release.sh 0.3.0-alpha    # Pre-release (alpha/beta → GitHub marks as prerelease)
#   ./scripts/release.sh 1.0.0          # Stable release
#
# What this script does:
#   1. Validates the version format
#   2. Updates version in all Cargo.toml files
#   3. Runs the full verification suite (build, test, clippy, fmt)
#   4. Updates Cargo.lock
#   5. Commits the version bump
#   6. Creates a git tag
#   7. Pushes the tag (which triggers the GitHub Actions release workflow)
#
# Prerequisites:
#   - Clean working tree (no uncommitted changes)
#   - Nix devShell available (./dev script)
#   - gh CLI installed (for optional PR creation)

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m' # No Color

info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }

# ── Argument validation ─────────────────────────────────────

VERSION="${1:-}"
if [ -z "$VERSION" ]; then
    echo "Usage: $0 <version>"
    echo ""
    echo "Examples:"
    echo "  $0 0.3.0-alpha"
    echo "  $0 1.0.0"
    exit 1
fi

# Validate semver-ish format (allows -alpha, -beta, -rc.1, etc.)
if ! echo "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$'; then
    error "Invalid version format: '$VERSION'. Expected semver (e.g., 0.3.0-alpha, 1.0.0)"
fi

TAG="v${VERSION}"

# ── Pre-flight checks ───────────────────────────────────────

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

info "Release ${BOLD}${TAG}${NC} from ${REPO_ROOT}"

# Check for clean working tree
if ! git diff --quiet || ! git diff --cached --quiet; then
    error "Working tree is not clean. Commit or stash changes first."
fi

# Check we're on main
BRANCH="$(git branch --show-current)"
if [ "$BRANCH" != "main" ]; then
    warn "Not on 'main' branch (current: ${BRANCH}). Continue? [y/N]"
    read -r answer
    if [ "$answer" != "y" ] && [ "$answer" != "Y" ]; then
        error "Aborted."
    fi
fi

# Check tag doesn't already exist
if git rev-parse "$TAG" >/dev/null 2>&1; then
    error "Tag '$TAG' already exists."
fi

# ── Version bump ─────────────────────────────────────────────

info "Bumping version to ${VERSION} in all Cargo.toml files..."

# Workspace root Cargo.toml — update the workspace.package.version
sed -i.bak "s/^version = \".*\"/version = \"${VERSION}\"/" Cargo.toml
rm -f Cargo.toml.bak

# All member crate Cargo.toml files
for cargo_toml in crates/*/Cargo.toml apps/*/Cargo.toml; do
    if [ -f "$cargo_toml" ]; then
        # Only update the [package] version, not dependency versions
        sed -i.bak "/^\[package\]/,/^\[/{s/^version = \".*\"/version = \"${VERSION}\"/}" "$cargo_toml"
        rm -f "${cargo_toml}.bak"
    fi
done

# Update Cargo.lock
info "Updating Cargo.lock..."
./dev "cargo update --workspace"

# ── Verification ─────────────────────────────────────────────

info "Running verification suite..."

info "  Building..."
./dev "cargo build --workspace"

info "  Testing..."
./dev "cargo test --workspace"

info "  Clippy..."
./dev "cargo clippy --workspace --all-targets -- -D warnings"

info "  Format check..."
./dev "cargo fmt --all -- --check"

info "${GREEN}All checks passed.${NC}"

# ── Commit and tag ───────────────────────────────────────────

info "Committing version bump..."
git add -A
git commit -m "Release ${TAG}

Bump all crate versions to ${VERSION}.

Co-Authored-By: claude-flow <ruv@ruv.net>"

info "Creating tag ${TAG}..."
git tag -a "$TAG" -m "Release ${TAG}"

# ── Push ─────────────────────────────────────────────────────

echo ""
echo -e "${BOLD}Ready to push.${NC} This will:"
echo "  1. Push the commit to origin/${BRANCH}"
echo "  2. Push tag ${TAG} (triggers the GitHub Actions release workflow)"
echo ""
echo -n "Push now? [y/N] "
read -r answer
if [ "$answer" != "y" ] && [ "$answer" != "Y" ]; then
    warn "Tag created locally but not pushed. To push later:"
    echo "  git push origin ${BRANCH} && git push origin ${TAG}"
    exit 0
fi

git push origin "$BRANCH"
git push origin "$TAG"

info "${GREEN}${BOLD}Release ${TAG} pushed!${NC}"
info "GitHub Actions will now build binaries and create the release."
info "Monitor at: https://github.com/trustedautonomy/ta/actions"
