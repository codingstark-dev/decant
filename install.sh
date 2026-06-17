#!/bin/sh
# decant installer
# Usage: curl -fsSL https://raw.githubusercontent.com/codingstark-dev/decant/main/install.sh | sh

set -e

REPO="codingstark-dev/decant"
BINARY="decant"
INSTALL_DIR="${DECANT_INSTALL_DIR:-$HOME/.local/bin}"

# ── Detect platform ────────────────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux*)  OS_NAME="unknown-linux-musl" ;;
  Darwin*) OS_NAME="apple-darwin" ;;
  *)
    echo "✗ Unsupported OS: $OS"
    echo "  Install manually: cargo install decant"
    exit 1
    ;;
esac

case "$ARCH" in
  x86_64)  ARCH_NAME="x86_64" ;;
  arm64|aarch64) ARCH_NAME="aarch64" ;;
  *)
    echo "✗ Unsupported architecture: $ARCH"
    echo "  Install manually: cargo install decant"
    exit 1
    ;;
esac

# ── Fetch latest version from GitHub ──────────────────────────────────────────
VERSION="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
  | grep '"tag_name"' \
  | sed -E 's/.*"v?([^"]+)".*/\1/')"

if [ -z "$VERSION" ]; then
  echo "✗ Could not determine latest version — check your connection"
  exit 1
fi

TARGET="${ARCH_NAME}-${OS_NAME}"
ARCHIVE="decant-v${VERSION}-${TARGET}.tar.gz"
URL="https://github.com/${REPO}/releases/download/v${VERSION}/${ARCHIVE}"

# ── Download and install ───────────────────────────────────────────────────────
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "→ Downloading decant v${VERSION} for ${TARGET}…"
curl -fsSL "$URL" -o "$TMP_DIR/$ARCHIVE"

echo "→ Extracting…"
tar -xzf "$TMP_DIR/$ARCHIVE" -C "$TMP_DIR"

echo "→ Installing to $INSTALL_DIR…"
mkdir -p "$INSTALL_DIR"
cp "$TMP_DIR/$BINARY" "$INSTALL_DIR/$BINARY"
chmod +x "$INSTALL_DIR/$BINARY"

# ── PATH hint ─────────────────────────────────────────────────────────────────
echo ""
echo "  ✓ decant v${VERSION} installed!"
echo ""

if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
  echo "  Add to PATH by running:"
  echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
  echo ""
  echo "  Then add that line to ~/.bashrc or ~/.zshrc to make it permanent."
  echo ""
fi

echo "  Run: decant --help"
echo ""
