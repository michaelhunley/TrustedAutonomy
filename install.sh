#!/bin/bash
# Trusted Autonomy installer script
# Usage: curl -fsSL https://raw.githubusercontent.com/trustedautonomy/ta/main/install.sh | sh

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

REPO="trustedautonomy/ta"
BINARY_NAME="ta"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# Detect OS and architecture
detect_platform() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"

    case "$OS" in
        Linux*)
            OS_TYPE="linux"
            ;;
        Darwin*)
            OS_TYPE="darwin"
            ;;
        *)
            echo -e "${RED}Error: Unsupported operating system: $OS${NC}"
            exit 1
            ;;
    esac

    case "$ARCH" in
        x86_64)
            ARCH_TYPE="x86_64"
            ;;
        arm64|aarch64)
            ARCH_TYPE="aarch64"
            ;;
        *)
            echo -e "${RED}Error: Unsupported architecture: $ARCH${NC}"
            exit 1
            ;;
    esac

    # Construct target triple
    if [ "$OS_TYPE" = "linux" ]; then
        TARGET="${ARCH_TYPE}-unknown-linux-musl"
    else
        TARGET="${ARCH_TYPE}-apple-darwin"
    fi

    echo -e "${GREEN}Detected platform:${NC} $OS_TYPE $ARCH_TYPE"
    echo -e "${GREEN}Target:${NC} $TARGET"
}

# Get latest release version
get_latest_version() {
    echo -e "${GREEN}Fetching latest release...${NC}"

    # Try to get latest release from GitHub API
    if command -v curl > /dev/null; then
        VERSION=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')
    else
        echo -e "${RED}Error: curl is required but not installed${NC}"
        exit 1
    fi

    if [ -z "$VERSION" ]; then
        echo -e "${RED}Error: Could not determine latest version${NC}"
        exit 1
    fi

    echo -e "${GREEN}Latest version:${NC} $VERSION"
}

# Download and install binary
install_binary() {
    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/${BINARY_NAME}-${VERSION}-${TARGET}.tar.gz"

    echo -e "${GREEN}Downloading from:${NC} $DOWNLOAD_URL"

    # Create temporary directory
    TMP_DIR=$(mktemp -d)
    trap "rm -rf $TMP_DIR" EXIT

    # Download archive
    if ! curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/${BINARY_NAME}.tar.gz"; then
        echo -e "${RED}Error: Failed to download binary${NC}"
        echo -e "${YELLOW}URL: $DOWNLOAD_URL${NC}"
        exit 1
    fi

    # Download checksum file
    CHECKSUM_URL="https://github.com/$REPO/releases/download/$VERSION/${BINARY_NAME}-${VERSION}-${TARGET}.tar.gz.sha256"
    echo -e "${GREEN}Downloading checksum...${NC}"
    if ! curl -fsSL "$CHECKSUM_URL" -o "$TMP_DIR/${BINARY_NAME}.tar.gz.sha256"; then
        echo -e "${YELLOW}Warning: Could not download checksum file${NC}"
        echo -e "${YELLOW}Skipping checksum verification (not recommended)${NC}"
    else
        # Verify checksum
        echo -e "${GREEN}Verifying checksum...${NC}"
        cd "$TMP_DIR"

        # Different SHA256 command formats for different platforms
        if command -v sha256sum > /dev/null; then
            # Linux
            if ! sha256sum -c "${BINARY_NAME}.tar.gz.sha256" 2>/dev/null; then
                echo -e "${RED}Error: Checksum verification failed${NC}"
                echo -e "${RED}The downloaded file may be corrupted or tampered with${NC}"
                exit 1
            fi
        elif command -v shasum > /dev/null; then
            # macOS
            if ! shasum -a 256 -c "${BINARY_NAME}.tar.gz.sha256" 2>/dev/null; then
                echo -e "${RED}Error: Checksum verification failed${NC}"
                echo -e "${RED}The downloaded file may be corrupted or tampered with${NC}"
                exit 1
            fi
        else
            echo -e "${YELLOW}Warning: No SHA256 tool found (sha256sum or shasum)${NC}"
            echo -e "${YELLOW}Skipping checksum verification (not recommended)${NC}"
        fi

        cd - > /dev/null
        echo -e "${GREEN}✓ Checksum verified${NC}"
    fi

    # Extract archive
    echo -e "${GREEN}Extracting binary...${NC}"
    tar xzf "$TMP_DIR/${BINARY_NAME}.tar.gz" -C "$TMP_DIR"

    # Create install directory if it doesn't exist
    mkdir -p "$INSTALL_DIR"

    # Install binary
    echo -e "${GREEN}Installing to:${NC} $INSTALL_DIR/$BINARY_NAME"
    mv "$TMP_DIR/$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"
    chmod +x "$INSTALL_DIR/$BINARY_NAME"

    echo -e "${GREEN}✓ Installation complete!${NC}"
}

# Verify installation
verify_installation() {
    if [ -x "$INSTALL_DIR/$BINARY_NAME" ]; then
        VERSION_OUTPUT=$("$INSTALL_DIR/$BINARY_NAME" --version 2>&1 || true)
        echo -e "${GREEN}Verification:${NC}"
        echo "  $VERSION_OUTPUT"
    else
        echo -e "${RED}Warning: Binary was installed but is not executable${NC}"
        exit 1
    fi
}

# Check if install directory is in PATH
check_path() {
    case ":$PATH:" in
        *":$INSTALL_DIR:"*)
            echo -e "${GREEN}✓ $INSTALL_DIR is in your PATH${NC}"
            ;;
        *)
            echo -e "${YELLOW}Warning: $INSTALL_DIR is not in your PATH${NC}"
            echo -e "${YELLOW}Add the following to your shell profile (~/.bashrc, ~/.zshrc, etc.):${NC}"
            echo ""
            echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
            echo ""
            ;;
    esac
}

# Print post-install instructions
print_instructions() {
    echo ""
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${GREEN}Getting Started with Trusted Autonomy${NC}"
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
    echo "1. Configure an agent adapter (e.g., Claude Code):"
    echo "   ${BINARY_NAME} adapter install claude-code"
    echo ""
    echo "2. Start your first mediated goal:"
    echo "   ${BINARY_NAME} run \"Add a README\" --source ."
    echo ""
    echo "3. Review and apply changes:"
    echo "   ${BINARY_NAME} pr view <id>"
    echo "   ${BINARY_NAME} pr approve <id>"
    echo "   ${BINARY_NAME} pr apply <id> --git-commit"
    echo ""
    echo "For help: ${BINARY_NAME} --help"
    echo "Documentation: https://github.com/$REPO"
    echo ""
}

# Main execution
main() {
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${GREEN}Trusted Autonomy Installer${NC}"
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""

    detect_platform
    get_latest_version
    install_binary
    verify_installation
    check_path
    print_instructions
}

main
