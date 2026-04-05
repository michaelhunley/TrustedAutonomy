#!/usr/bin/env bash
# staging-status.sh — show every staging dir with its goal state, size, and age.
# Safe to delete: Applied, Completed, Denied, PrReady (if not under active review)
# Keep:           Running, Configured (if agent still working)
#
# Usage:
#   ./scripts/staging-status.sh              # all staging dirs
#   ./scripts/staging-status.sh --stale      # only safe-to-delete
#   ./scripts/staging-status.sh --delete-stale  # delete stale dirs (prompts first)

set -euo pipefail

TA_DIR="$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel 2>/dev/null || echo "$HOME/development/TrustedAutonomy")/.ta"
STAGING_DIR="$TA_DIR/staging"
GOALS_DIR="$TA_DIR/goals"

STALE_STATES=("Applied" "Completed" "Denied")
FILTER="${1:-}"

if [ ! -d "$STAGING_DIR" ]; then
  echo "No staging directory found at $STAGING_DIR"
  exit 0
fi

mapfile -t DIRS < <(ls "$STAGING_DIR" 2>/dev/null)

if [ ${#DIRS[@]} -eq 0 ]; then
  echo "No staging directories."
  exit 0
fi

# Header
printf "\n%-8s  %-10s  %-6s  %-20s  %s\n" "SIZE" "STATE" "AGE" "GOAL ID" "TITLE"
printf "%s\n" "$(printf '%.0s-' {1..80})"

STALE_DIRS=()

for dir in "${DIRS[@]}"; do
  full_path="$STAGING_DIR/$dir"
  goal_file="$GOALS_DIR/$dir.json"

  # Size — use find to count top-level size estimate (fast, non-recursive)
  size=$(du -sh --max-depth=0 "$full_path" 2>/dev/null | cut -f1 || \
         du -sh -d 0 "$full_path" 2>/dev/null | cut -f1 || echo "?")

  # Age (days since last modification)
  if [[ "$OSTYPE" == "darwin"* ]]; then
    mtime=$(stat -f %m "$full_path" 2>/dev/null || echo 0)
  else
    mtime=$(stat -c %Y "$full_path" 2>/dev/null || echo 0)
  fi
  now=$(date +%s)
  age_days=$(( (now - mtime) / 86400 ))
  age_hours=$(( (now - mtime) / 3600 ))
  if [ "$age_days" -ge 1 ]; then
    age="${age_days}d"
  else
    age="${age_hours}h"
  fi

  # Goal state + title from JSON
  if [ -f "$goal_file" ]; then
    state=$(python3 -c "import json; d=json.load(open('$goal_file')); print(d.get('state','?'))" 2>/dev/null || echo "?")
    title=$(python3 -c "import json; d=json.load(open('$goal_file')); print(d.get('title','?')[:50])" 2>/dev/null || echo "?")
  else
    state="NO_RECORD"
    title="(goal record missing)"
  fi

  short_id="${dir:0:8}"

  # Determine if stale
  is_stale=false
  for s in "${STALE_STATES[@]}"; do
    if [[ "$state" == "$s" ]]; then
      is_stale=true
      break
    fi
  done
  [[ "$state" == "NO_RECORD" ]] && is_stale=true

  # Color coding
  if $is_stale; then
    color="\033[0;33m"  # yellow = safe to delete
    STALE_DIRS+=("$full_path")
  elif [[ "$state" == "Running" ]]; then
    color="\033[0;32m"  # green = active
  else
    color="\033[0;36m"  # cyan = review/pending
  fi
  reset="\033[0m"

  if [[ "$FILTER" == "--stale" ]] && ! $is_stale; then
    continue
  fi

  printf "${color}%-8s  %-10s  %-6s  %-20s  %s${reset}\n" \
    "$size" "$state" "$age" "$short_id..." "$title"
done

echo ""

if [[ "$FILTER" == "--stale" || "$FILTER" == "--delete-stale" ]]; then
  total=${#STALE_DIRS[@]}
  if [ "$total" -eq 0 ]; then
    echo "No stale staging directories found."
    exit 0
  fi

  total_size=$(du -sh "${STALE_DIRS[@]}" 2>/dev/null | tail -1 | cut -f1 || echo "?")
  echo "Stale dirs: $total  (total: ~$total_size)"

  if [[ "$FILTER" == "--delete-stale" ]]; then
    echo ""
    read -r -p "Delete all $total stale staging directories? [y/N] " confirm
    if [[ "$confirm" =~ ^[Yy]$ ]]; then
      for d in "${STALE_DIRS[@]}"; do
        echo "  Removing: $d"
        rm -rf "$d"
      done
      echo "Done."
    else
      echo "Aborted."
    fi
  fi
else
  echo "Legend: \033[0;32mgreen\033[0m=Running  \033[0;36mcyan\033[0m=PrReady/other  \033[0;33myellow\033[0m=safe to delete"
  echo ""
  echo "  ./scripts/staging-status.sh --stale          # show only deletable"
  echo "  ./scripts/staging-status.sh --delete-stale   # delete with confirmation"
fi
