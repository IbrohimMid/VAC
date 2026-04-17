#!/bin/sh
set -e

# vac installation script
# https://github.com/your-org/vac

echo "Installing vac..."

OS="$(uname -s)"
ARCH="$(uname -m)"

if [ "$OS" = "Linux" ]; then
    if [ "$ARCH" = "x86_64" ]; then
        TARGET="x86_64-unknown-linux-gnu"
    elif [ "$ARCH" = "aarch64" ]; then
        TARGET="aarch64-unknown-linux-gnu"
    else
        echo "Unsupported architecture: $ARCH"
        exit 1
    fi
elif [ "$OS" = "Darwin" ]; then
    if [ "$ARCH" = "x86_64" ]; then
        TARGET="x86_64-apple-darwin"
    elif [ "$ARCH" = "arm64" ]; then
        TARGET="aarch64-apple-darwin"
    else
        echo "Unsupported architecture: $ARCH"
        exit 1
    fi
else
    echo "Unsupported OS: $OS"
    exit 1
fi

VERSION="latest"
if [ -n "$1" ]; then
    VERSION="$1"
fi

if [ "$VERSION" = "latest" ]; then
    # Fetch latest release tag
    # TAG=$(curl -s https://api.github.com/repos/your-org/vac/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
    TAG="v0.1.0" # Placeholder
else
    TAG="$VERSION"
fi

URL="https://github.com/your-org/vac/releases/download/${TAG}/vac-${TARGET}.tar.gz"

echo "Downloading $URL"
curl -sL "$URL" -o vac.tar.gz
tar -xzf vac.tar.gz

echo "Installing to /usr/local/bin"
sudo mv vac /usr/local/bin/vac
rm vac.tar.gz

echo "vac installed successfully!"
vac --version
