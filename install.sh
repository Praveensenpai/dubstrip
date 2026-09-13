#!/usr/bin/env bash
# ==============================================================================
#  🗡️ DubStrip - Remote Binary Bootstrapper
#  Usage: curl -fsSL https://raw.githubusercontent.com/Praveensenpai/dubstrip/main/install.sh | bash
# ==============================================================================
set -euo pipefail
IFS=$'\n\t'

REPO="Praveensenpai/dubstrip"
BINARY="dubstrip"
INSTALL_DIR="${HOME}/.local/bin"
mkdir -p "$INSTALL_DIR"

echo "🗡️ ========================================= 🗡️"
echo "           DubStrip Native Audio Preserver      "
echo "🗡️ ========================================= 🗡️"

ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|amd64)   TARGET="x86_64-unknown-linux-gnu" ;;
    aarch64|arm64)  TARGET="aarch64-unknown-linux-gnu" ;;
    *)
        echo "❌ Architecture $ARCH is not supported."
        exit 1
        ;;
esac

echo "==> Fetching latest release..."
TAG=$(curl -4 -sSL \
    -H "Cache-Control: no-cache" \
    -H "Pragma: no-cache" \
    "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null \
    | grep '"tag_name":' \
    | sed -E 's/.*"([^"]+)".*/\1/' \
    || true)

if [ -z "${TAG:-}" ]; then
    TAG="v0.1.0"
fi

DOWNLOAD_URL="https://github.com/$REPO/releases/download/$TAG/dubstrip-${TARGET}.tar.gz"
echo "==> Downloading dubstrip ${TAG} (${TARGET})..."

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

if ! curl -4 -fsSL "$DOWNLOAD_URL" | tar -xz -C "$TMP_DIR" 2>/dev/null; then
    echo "❌ Failed to download release binary from: $DOWNLOAD_URL"
    exit 1
fi

install -m 755 "$TMP_DIR/$BINARY" "$INSTALL_DIR/$BINARY"
echo "✔ Successfully installed dubstrip ${TAG} to $INSTALL_DIR/$BINARY"

case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) export PATH="$INSTALL_DIR:$PATH" ;;
esac

if [ -f "${HOME}/.bashrc" ] && ! grep -q '\.local/bin' "${HOME}/.bashrc"; then
    printf '\n# User local binaries\nexport PATH="%s:$PATH"\n' "$INSTALL_DIR" >> "${HOME}/.bashrc"
fi

echo ""
"$INSTALL_DIR/$BINARY" --help | head -n 8
echo ""
echo "🎉 DubStrip is ready to use!"
