#!/usr/bin/env bash
# install_local.sh — Build TA from source and add it to your PATH.
#
# Usage:
#   ./install_local.sh          # Build release binary and install
#   ./install_local.sh --debug  # Build debug binary (faster compile)
#
# After running, either:
#   1. Restart your shell, or
#   2. Run: export PATH="$HOME/.local/bin:$PATH"

set -euo pipefail

INSTALL_DIR="${HOME}/.local/bin"
PROFILE="${CARGO_BUILD_PROFILE:-release}"

if [[ "${1:-}" == "--debug" ]]; then
    PROFILE="dev"
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "Building ta-cli (profile: ${PROFILE})..."

# Detect build environment: Nix devShell or system Rust.
if command -v nix &>/dev/null && [[ -f flake.nix ]]; then
    echo "  Using Nix devShell..."
    export PATH="/nix/var/nix/profiles/default/bin:$HOME/.nix-profile/bin:$PATH"
    if [[ "$PROFILE" == "dev" ]]; then
        nix develop --command bash -c "cargo build -p ta-cli"
        BINARY="target/debug/ta"
    else
        nix develop --command bash -c "cargo build --release -p ta-cli"
        BINARY="target/release/ta"
    fi
elif command -v cargo &>/dev/null; then
    echo "  Using system Rust toolchain..."
    if [[ "$PROFILE" == "dev" ]]; then
        cargo build -p ta-cli
        BINARY="target/debug/ta"
    else
        cargo build --release -p ta-cli
        BINARY="target/release/ta"
    fi
else
    echo "Error: Neither Nix nor Cargo found. Install Rust or Nix first."
    echo "  Rust: https://rustup.rs"
    echo "  Nix:  https://nixos.org/download"
    exit 1
fi

if [[ ! -f "$BINARY" ]]; then
    echo "Error: Build succeeded but binary not found at $BINARY"
    exit 1
fi

# Install to ~/.local/bin.
mkdir -p "$INSTALL_DIR"
cp "$BINARY" "$INSTALL_DIR/ta"
chmod +x "$INSTALL_DIR/ta"

echo ""
echo "Installed: $INSTALL_DIR/ta"
"$INSTALL_DIR/ta" --version
echo ""

# Check if ~/.local/bin is in PATH.
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo "Add to your PATH by adding this to your shell profile:"
    echo ""

    # Detect shell and suggest the right file.
    SHELL_NAME="$(basename "${SHELL:-bash}")"
    case "$SHELL_NAME" in
        zsh)  PROFILE_FILE="~/.zshrc" ;;
        bash) PROFILE_FILE="~/.bashrc" ;;
        fish) PROFILE_FILE="~/.config/fish/config.fish" ;;
        *)    PROFILE_FILE="~/.profile" ;;
    esac

    if [[ "$SHELL_NAME" == "fish" ]]; then
        echo "  fish_add_path $INSTALL_DIR"
    else
        echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    fi
    echo ""
    echo "  (add to $PROFILE_FILE for persistence)"
    echo ""
    echo "Or for this session only:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
else
    echo "~/.local/bin is already in your PATH. You're all set."
fi
