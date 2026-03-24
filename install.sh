#!/bin/sh
# Install frg (fastripgrep) — fast regex search with sparse n-gram indexing
# Usage: curl -fsSL https://raw.githubusercontent.com/qhkm/fastripgrep/main/install.sh | sh

set -e

REPO="qhkm/fastripgrep"
BINARY="frg"
INSTALL_DIR="${FRG_INSTALL_DIR:-/usr/local/bin}"

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)  OS_TAG="unknown-linux-gnu" ;;
    Darwin) OS_TAG="apple-darwin" ;;
    *)      echo "Error: unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
    x86_64|amd64)   ARCH_TAG="x86_64" ;;
    aarch64|arm64)  ARCH_TAG="aarch64" ;;
    *)              echo "Error: unsupported architecture: $ARCH"; exit 1 ;;
esac

TARGET="${ARCH_TAG}-${OS_TAG}"

# Get latest release tag
LATEST=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$LATEST" ]; then
    echo "No releases found. Installing from source with cargo..."
    if command -v cargo >/dev/null 2>&1; then
        cargo install fastripgrep
        echo "Installed frg $(frg --version)"
        exit 0
    else
        echo "Error: no releases available and cargo not found."
        echo "Install Rust first: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        exit 1
    fi
fi

DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST}/frg-${LATEST}-${TARGET}.tar.gz"

echo "Installing frg ${LATEST} for ${TARGET}..."

# Create temp directory
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

# Download and extract
curl -fsSL "$DOWNLOAD_URL" -o "${TMP_DIR}/frg.tar.gz"
tar -xzf "${TMP_DIR}/frg.tar.gz" -C "$TMP_DIR"

# Install binary
if [ -w "$INSTALL_DIR" ]; then
    cp "${TMP_DIR}/frg" "${INSTALL_DIR}/${BINARY}"
    chmod +x "${INSTALL_DIR}/${BINARY}"
else
    echo "Need sudo to install to ${INSTALL_DIR}"
    sudo cp "${TMP_DIR}/frg" "${INSTALL_DIR}/${BINARY}"
    sudo chmod +x "${INSTALL_DIR}/${BINARY}"
fi

echo "Installed frg ${LATEST} to ${INSTALL_DIR}/${BINARY}"
echo ""
echo "Get started:"
echo "  frg index .          # Build index (one-time)"
echo "  frg search 'pattern' # Search"
echo "  frg status           # Check index"
