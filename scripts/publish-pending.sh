#!/usr/bin/env bash
# scripts/publish-pending.sh - publish only the crates not yet on crates.io, one per 10m5s.
#
# Usage:
#   CARGO_REGISTRY_TOKEN=<token> ./scripts/publish-pending.sh
#
# Skips already-published crates instantly (skips do NOT count against rate limit).
# After each successful new publish waits 10m5s.
# On 429, reads the Retry-After time from cargo output and waits exactly that long.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

if [[ -z "${CARGO_REGISTRY_TOKEN:-}" ]]; then
  echo "ERROR: CARGO_REGISTRY_TOKEN is not set."
  echo "  Export it before running:"
  echo "    CARGO_REGISTRY_TOKEN=<token> ./scripts/publish-pending.sh"
  exit 1
fi

VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
echo "Workspace version: $VERSION"
echo ""

# 13 remaining crates in dependency order (tiers 2-6 not yet published).
# Edit this list if some get published in the meantime — the API check will skip them anyway.
declare -A CRATE_PATH
CRATE_PATH["ta-workflow"]="crates/ta-workflow"
CRATE_PATH["ta-connector-slack"]="crates/ta-connectors/slack"
CRATE_PATH["ta-connector-discord"]="crates/ta-connectors/discord"
CRATE_PATH["ta-connector-email"]="crates/ta-connectors/email"
CRATE_PATH["ta-connector-unreal"]="crates/ta-connectors/unreal"
CRATE_PATH["ta-connector-comfyui"]="crates/ta-connectors/comfyui"
CRATE_PATH["ta-db-proxy-sqlite"]="crates/ta-db-proxy-sqlite"
CRATE_PATH["ta-connector-fs"]="crates/ta-connectors/fs"
CRATE_PATH["ta-submit"]="crates/ta-submit"
CRATE_PATH["ta-mediation"]="crates/ta-mediation"
CRATE_PATH["ta-mcp-gateway"]="crates/ta-mcp-gateway"
CRATE_PATH["ta-daemon"]="crates/ta-daemon"
CRATE_PATH["ta-cli"]="apps/ta-cli"

ORDERED=(
  ta-workflow
  ta-connector-slack
  ta-connector-discord
  ta-connector-email
  ta-connector-unreal
  ta-connector-comfyui
  ta-db-proxy-sqlite
  ta-connector-fs
  ta-submit
  ta-mediation
  ta-mcp-gateway
  ta-daemon
  ta-cli
)

is_published() {
  local crate="$1" ver="$2"
  local result
  result=$(curl -sf "https://crates.io/api/v1/crates/${crate}/${ver}" 2>/dev/null || true)
  echo "$result" | grep -q '"num"' && return 0 || return 1
}

TOTAL=${#ORDERED[@]}
PUBLISHED=0
SKIPPED=0

for CRATE in "${ORDERED[@]}"; do
  PATH_REL="${CRATE_PATH[$CRATE]}"

  echo "--------------------------------------------"
  echo "Crate: $CRATE  ($PATH_REL)"

  if is_published "$CRATE" "$VERSION"; then
    echo "  Already on crates.io -- skipping"
    SKIPPED=$(( SKIPPED + 1 ))
    continue
  fi

  echo "  Publishing $CRATE@$VERSION ..."

  while true; do
    EXIT=0
    OUT=$(cargo publish -p "$CRATE" --token "$CARGO_REGISTRY_TOKEN" 2>&1) || EXIT=$?

    if [ $EXIT -eq 0 ]; then
      echo "  Published $CRATE@$VERSION"
      PUBLISHED=$(( PUBLISHED + 1 ))
      echo "  Waiting 605s before next publish (index propagation + rate-limit headroom)..."
      sleep 605
      break

    elif echo "$OUT" | grep -q "already exists"; then
      echo "  Already published (cargo confirmed) -- skipping"
      SKIPPED=$(( SKIPPED + 1 ))
      break

    elif echo "$OUT" | grep -q "429 Too Many Requests"; then
      RETRY_AFTER=$(echo "$OUT" | grep -oP 'after \K[^"]+(?= GMT)' | head -1 || true)
      if [[ -n "$RETRY_AFTER" ]]; then
        # Try GNU date first (Linux), fall back to BSD date (macOS)
        RETRY_EPOCH=$(date -d "$RETRY_AFTER GMT" +%s 2>/dev/null \
          || date -j -f "%a, %d %b %Y %T" "$RETRY_AFTER" +%s 2>/dev/null \
          || echo "")
        NOW_EPOCH=$(date +%s)
        if [[ -n "$RETRY_EPOCH" && "$RETRY_EPOCH" -gt "$NOW_EPOCH" ]]; then
          WAIT=$(( RETRY_EPOCH - NOW_EPOCH + 10 ))
          echo "  Rate limited. Retry after: $RETRY_AFTER GMT"
          echo "  Sleeping ${WAIT}s ..."
          sleep "$WAIT"
        else
          echo "  Rate limited (could not parse retry time). Sleeping 615s ..."
          sleep 615
        fi
      else
        echo "  Rate limited (no retry-after found). Sleeping 615s ..."
        sleep 615
      fi
      echo "  Retrying $CRATE ..."
      # loop continues to retry this same crate

    else
      echo "  FAILED to publish $CRATE:"
      echo "$OUT"
      exit 1
    fi
  done
done

echo ""
echo "============================================"
echo "Done.  Published: $PUBLISHED  Skipped: $SKIPPED  Total: $TOTAL"
if [ "$PUBLISHED" -gt 0 ]; then
  echo "  cargo install ta-cli  # should now work in a clean environment"
fi
