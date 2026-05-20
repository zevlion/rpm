#!/usr/bin/env bash
set -euo pipefail

# ── config ────────────────────────────────────────────────────────────────────

REPO="zevlion/rpm2"
BINARY="rpm2"
TMP_PATH="/tmp/rpm2_bin"
INSTALL_DIR="/usr/local/bin"
INSTALL_PATH="$INSTALL_DIR/$BINARY"
DOWNLOAD_URL="https://github.com/$REPO/releases/download/latest/$BINARY"

# ── helpers ───────────────────────────────────────────────────────────────────

info()    { echo "  $*"; }
success() { echo "✓ $*"; }
error()   { echo "✗ $*" >&2; exit 1; }

need_cmd() {
    command -v "$1" &>/dev/null || error "required command not found: $1"
}

# ── checks ────────────────────────────────────────────────────────────────────

need_cmd curl
need_cmd chmod

# Determine if we need sudo for the install directory
if [ -w "$INSTALL_DIR" ]; then
    SUDO=""
else
    need_cmd sudo
    SUDO="sudo"
fi

# ── detect update vs fresh install ───────────────────────────────────────────

if command -v "$BINARY" &>/dev/null; then
    CURRENT_VERSION=$(rpm2 --version 2>/dev/null || echo "unknown")
    echo "Updating rpm2 ($CURRENT_VERSION → latest)..."
    IS_UPDATE=true
else
    echo "Installing rpm2..."
    IS_UPDATE=false
fi

# ── stop daemon before replacing binary ───────────────────────────────────────

if $IS_UPDATE; then
    info "Stopping rpm2 daemon (if running)..."
    rpm2 kill 2>/dev/null || true
    sleep 0.4
fi

# ── download ──────────────────────────────────────────────────────────────────

info "Downloading from $DOWNLOAD_URL"

# Clean up any previous failed attempt
rm -f "$TMP_PATH"

if ! curl -fsSL --progress-bar "$DOWNLOAD_URL" -o "$TMP_PATH"; then
    error "Download failed. Check your internet connection or visit: https://github.com/$REPO/releases"
fi

# Basic sanity check — file should be non-empty and an ELF binary
if [ ! -s "$TMP_PATH" ]; then
    error "Downloaded file is empty"
fi

if ! head -c 4 "$TMP_PATH" | grep -q $'^\x7fELF'; then
    rm -f "$TMP_PATH"
    error "Downloaded file does not appear to be a valid Linux binary"
fi

chmod +x "$TMP_PATH"

# ── install ───────────────────────────────────────────────────────────────────

info "Installing to $INSTALL_PATH"

if ! $SUDO mv "$TMP_PATH" "$INSTALL_PATH"; then
    rm -f "$TMP_PATH"
    error "Failed to install binary to $INSTALL_PATH"
fi

# ── verify ────────────────────────────────────────────────────────────────────

# Make sure the install dir is actually on PATH
if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
    echo ""
    echo "⚠ $INSTALL_DIR is not in your PATH."
    echo "  Add this to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
    echo ""
    echo '    export PATH="$PATH:'"$INSTALL_DIR"'"'
    echo ""
fi

NEW_VERSION=$("$INSTALL_PATH" --version 2>/dev/null || echo "unknown")

if $IS_UPDATE; then
    success "Updated to $NEW_VERSION"
else
    success "Installed $NEW_VERSION"
fi