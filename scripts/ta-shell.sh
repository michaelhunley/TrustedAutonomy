#!/usr/bin/env bash
# ta-shell.sh -- Start the TA daemon (if needed) and launch the interactive shell.
#
# Usage:
#   ./scripts/ta-shell.sh [--port 7700] [--project-root .] [shell args...]
#
# The script checks whether the daemon is already listening. If not, it starts
# one in the background and waits for it to become healthy before opening the
# shell. On exit, the daemon keeps running (use `kill` or stop it manually).

set -euo pipefail

PORT="${TA_DAEMON_PORT:-7700}"
BIND="${TA_DAEMON_BIND:-127.0.0.1}"
PROJECT_ROOT="."
SHELL_ARGS=()

# Parse arguments.
while [[ $# -gt 0 ]]; do
  case "$1" in
    --port)       PORT="$2"; shift 2 ;;
    --project-root) PROJECT_ROOT="$2"; shift 2 ;;
    *)            SHELL_ARGS+=("$1"); shift ;;
  esac
done

DAEMON_URL="http://${BIND}:${PORT}"

# Locate binaries. Prefer siblings of this script, then PATH.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
find_bin() {
  local name="$1"
  if [[ -x "${SCRIPT_DIR}/../target/release/${name}" ]]; then
    echo "${SCRIPT_DIR}/../target/release/${name}"
  elif [[ -x "${SCRIPT_DIR}/../target/debug/${name}" ]]; then
    echo "${SCRIPT_DIR}/../target/debug/${name}"
  elif command -v "$name" &>/dev/null; then
    command -v "$name"
  else
    echo "$name"
  fi
}

TA_BIN="$(find_bin ta)"
DAEMON_BIN="$(find_bin ta-daemon)"

# Check if the daemon is already running.
daemon_healthy() {
  curl -sf "${DAEMON_URL}/api/status" >/dev/null 2>&1
}

if daemon_healthy; then
  echo "Daemon already running at ${DAEMON_URL}"
else
  echo "Starting daemon at ${DAEMON_URL} ..."
  "$DAEMON_BIN" --api --project-root "$PROJECT_ROOT" &
  DAEMON_PID=$!

  # Wait up to 10 seconds for the daemon to become healthy.
  for i in $(seq 1 20); do
    if daemon_healthy; then
      echo "Daemon ready (pid ${DAEMON_PID})"
      break
    fi
    if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
      echo "Error: daemon exited unexpectedly" >&2
      exit 1
    fi
    sleep 0.5
  done

  if ! daemon_healthy; then
    echo "Error: daemon did not become healthy within 10 seconds" >&2
    kill "$DAEMON_PID" 2>/dev/null || true
    exit 1
  fi
fi

# Launch the shell.
exec "$TA_BIN" shell --url "$DAEMON_URL" "${SHELL_ARGS[@]}"
