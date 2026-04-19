#!/usr/bin/env bash
set -euo pipefail

VAC_VERSION="${VAC_VERSION:-latest}"
VAC_INSTALL_DIR="${VAC_INSTALL_DIR:-$HOME/.local/bin}"
REPO="IbrohimMid/VAC"

# Detect platform
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$ARCH" in
    x86_64)  ARCH="x86_64" ;;
    arm64|aarch64) ARCH="aarch64" ;;
    *) echo "Error: unsupported architecture: $ARCH"; exit 1 ;;
esac
case "$OS" in
    linux)  TARGET="${ARCH}-unknown-linux-musl" ;;
    darwin) TARGET="${ARCH}-apple-darwin" ;;
    *) echo "Error: unsupported OS: $OS"; exit 1 ;;
esac

# Resolve version
if [ "$VAC_VERSION" = "latest" ]; then
    VAC_VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name"' | sed 's/.*"v\([^"]*\)".*/\1/')
    if [ -z "$VAC_VERSION" ]; then
        echo "Error: could not determine latest version"; exit 1
    fi
fi

TARBALL="vac-v${VAC_VERSION}-${TARGET}.tar.gz"
URL="https://github.com/${REPO}/releases/download/v${VAC_VERSION}/${TARBALL}"

echo "Downloading VAC v${VAC_VERSION} for ${TARGET}..."

# Download + verify
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
curl -fsSL "$URL" -o "$TMP/$TARBALL"

# SHA256 verification (mandatory)
if curl -fsSL "https://github.com/${REPO}/releases/download/v${VAC_VERSION}/SHA256SUMS.txt" -o "$TMP/SHA256SUMS.txt"; then
    (cd "$TMP" && grep " ${TARBALL}$" SHA256SUMS.txt | sha256sum -c -) || {
        echo "Error: checksum verification failed";
        exit 1;
    }
else
    echo "Error: SHA256SUMS.txt not available, verification failed."
    exit 1
fi

# Install
mkdir -p "$VAC_INSTALL_DIR"
tar -xzf "$TMP/$TARBALL" -C "$TMP"
install -m 755 "$TMP/vac" "$VAC_INSTALL_DIR/vac"

echo ""
echo "VAC installed to ${VAC_INSTALL_DIR}/vac"

# Check PATH
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$VAC_INSTALL_DIR"; then
    echo ""
    echo "Add ${VAC_INSTALL_DIR} to your PATH:"
    echo '  export PATH="'"${VAC_INSTALL_DIR}"':$PATH"'
fi
