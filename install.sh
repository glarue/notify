#!/bin/bash
set -e

# Detect OS and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

# Map architecture names
case "$ARCH" in
    x86_64|amd64)
        ARCH="x86_64"
        ;;
    aarch64|arm64)
        ARCH="aarch64"
        ;;
    *)
        echo "Error: Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

# Determine platform-specific details
case "$OS" in
    darwin)
        PLATFORM="macos"
        EXT="tar.gz"
        ;;
    linux)
        PLATFORM="linux"
        EXT="tar.gz"
        ;;
    mingw*|msys*|cygwin*)
        PLATFORM="windows"
        EXT="zip"
        ;;
    *)
        echo "Error: Unsupported OS: $OS"
        exit 1
        ;;
esac

ASSET_NAME="notify-${PLATFORM}-${ARCH}"
DOWNLOAD_URL="https://github.com/glarue/notify/releases/latest/download/${ASSET_NAME}.${EXT}"

echo "Downloading notify for ${PLATFORM}-${ARCH}..."
echo "URL: ${DOWNLOAD_URL}"

# Download
if command -v curl >/dev/null 2>&1; then
    curl -fsSL -o "/tmp/${ASSET_NAME}.${EXT}" "${DOWNLOAD_URL}"
elif command -v wget >/dev/null 2>&1; then
    wget -q -O "/tmp/${ASSET_NAME}.${EXT}" "${DOWNLOAD_URL}"
else
    echo "Error: curl or wget is required"
    exit 1
fi

# Extract
cd /tmp
if [ "$EXT" = "tar.gz" ]; then
    tar xzf "${ASSET_NAME}.${EXT}"
elif [ "$EXT" = "zip" ]; then
    unzip -q "${ASSET_NAME}.${EXT}"
fi

# Install
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

if [ -w "$INSTALL_DIR" ]; then
    mv notify "$INSTALL_DIR/"
    echo "✓ Installed notify to $INSTALL_DIR/notify"
else
    echo "Installing to $INSTALL_DIR (requires sudo)..."
    sudo mv notify "$INSTALL_DIR/"
    echo "✓ Installed notify to $INSTALL_DIR/notify"
fi

# Cleanup
rm -f "/tmp/${ASSET_NAME}.${EXT}"

echo ""
echo "notify has been installed successfully!"
echo "Run 'notify --setup-server' to configure SMTP settings."
