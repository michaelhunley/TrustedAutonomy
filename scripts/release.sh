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
#   2. Collects commits since last tag
#   3. Generates release notes (via TA agent if available, else from commit log)
#   4. Updates version in all Cargo.toml files + DISCLAIMER.md
#   5. Runs the full verification suite (build, test, clippy, fmt)
#   6. Commits the version bump + release notes
#   7. Creates a git tag with release notes
#   8. Pushes the tag (which triggers the GitHub Actions release workflow)
#
# Prerequisites:
#   - Clean working tree (no uncommitted changes)
#   - Nix devShell available (./dev script)
#   - ta binary available for agent-generated release notes (optional)

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

# ── Collect commits since last release ───────────────────────

LAST_TAG="$(git describe --tags --abbrev=0 2>/dev/null || echo "")"
if [ -n "$LAST_TAG" ]; then
    info "Collecting commits since ${LAST_TAG}..."
    COMMIT_LOG="$(git log "${LAST_TAG}..HEAD" --pretty=format:"- %s (%h)" --no-merges)"
else
    info "No previous tag found. Collecting all commits..."
    COMMIT_LOG="$(git log --pretty=format:"- %s (%h)" --no-merges)"
fi

COMMIT_COUNT="$(echo "$COMMIT_LOG" | wc -l | tr -d ' ')"
info "Found ${COMMIT_COUNT} commits to include."

# ── Generate release notes ───────────────────────────────────

RELEASE_NOTES_FILE="${REPO_ROOT}/RELEASE_NOTES.md"

generate_notes_from_commits() {
    cat > "$RELEASE_NOTES_FILE" <<NOTES_EOF
# Release ${TAG}

## Changes since ${LAST_TAG:-"initial release"}

${COMMIT_LOG}

---

Full changelog: https://github.com/trustedautonomy/ta/compare/${LAST_TAG:-"main"}...${TAG}
NOTES_EOF
}

# Try agent-generated release notes via TA, fall back to commit log
if command -v ta >/dev/null 2>&1; then
    info "Generating release notes via TA agent framework..."

    OBJECTIVE="Synthesize user-facing release notes for version ${TAG} of Trusted Autonomy.

Here are the commits since the last release (${LAST_TAG:-"initial"}):

${COMMIT_LOG}

Write concise, user-facing release notes in Markdown. Group changes into
sections like \"New Features\", \"Improvements\", \"Bug Fixes\" as appropriate.
Do NOT include commit hashes or internal details. Focus on what matters
to users. Keep it brief — a few bullet points per section.

Write the release notes to: ${RELEASE_NOTES_FILE}"

    # Launch a TA goal for the agent to write release notes
    ta run "Release notes for ${TAG}" \
        --agent claude-code \
        --source "$REPO_ROOT" \
        --objective "$OBJECTIVE" 2>/dev/null || true

    # If the agent didn't produce notes, fall back
    if [ ! -f "$RELEASE_NOTES_FILE" ]; then
        info "Agent did not produce notes — falling back to commit log."
        generate_notes_from_commits
    fi
else
    info "ta not found — generating release notes from commit log."
    generate_notes_from_commits
fi

info "Release notes written to ${RELEASE_NOTES_FILE}"
echo ""
echo -e "${BOLD}── Release Notes ──${NC}"
cat "$RELEASE_NOTES_FILE"
echo ""

echo -n "Edit release notes before continuing? [y/N] "
read -r answer
if [ "$answer" = "y" ] || [ "$answer" = "Y" ]; then
    "${EDITOR:-vi}" "$RELEASE_NOTES_FILE"
fi

# ── Version bump ─────────────────────────────────────────────

info "Bumping version to ${VERSION}..."

# Workspace root Cargo.toml
sed -i.bak "s/^version = \".*\"/version = \"${VERSION}\"/" Cargo.toml
rm -f Cargo.toml.bak

# All member crate Cargo.toml files
for cargo_toml in crates/*/Cargo.toml apps/*/Cargo.toml; do
    if [ -f "$cargo_toml" ]; then
        sed -i.bak "/^\[package\]/,/^\[/{s/^version = \".*\"/version = \"${VERSION}\"/}" "$cargo_toml"
        rm -f "${cargo_toml}.bak"
    fi
done

# Update DISCLAIMER.md version
sed -i.bak "s/^\*\*Version\*\*: .*/\*\*Version\*\*: ${VERSION}/" DISCLAIMER.md
rm -f DISCLAIMER.md.bak

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

RELEASE_NOTES_BODY="$(cat "$RELEASE_NOTES_FILE")"

info "Committing version bump..."
git add -A
git commit -m "Release ${TAG}

Bump all crate versions to ${VERSION}.

Co-Authored-By: claude-flow <ruv@ruv.net>"

info "Creating tag ${TAG} with release notes..."
git tag -a "$TAG" -m "Release ${TAG}

${RELEASE_NOTES_BODY}"

# Clean up temp file
rm -f "$RELEASE_NOTES_FILE"

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
