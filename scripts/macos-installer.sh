#!/usr/bin/env bash
set -euo pipefail

# ── config ────────────────────────────────────────────────────────────────────

REPO="zevlion/rpm2"
BINARY="rpm2"
TMP_PATH="/tmp/rpm2_bin"
INSTALL_DIR="/usr/local/bin"
INSTALL_PATH="$INSTALL_DIR/$BINARY"

# ── arch detection ────────────────────────────────────────────────────────────

ARCH=$(uname -m)
case "$ARCH" in
    x86_64)  ASSET="rpm2-macos-x86_64" ;;
    arm64)   ASSET="rpm2-macos-arm64"  ;;
    *)       error "Unsupported architecture: $ARCH" ;;
esac

DOWNLOAD_URL="https://github.com/$REPO/releases/download/latest/$ASSET"

# ── helpers ───────────────────────────────────────────────────────────────────

info()    { echo "  $*"; }
success() { echo "✓ $*"; }
warn()    { echo "⚠ $*"; }
error()   { echo "✗ $*" >&2; exit 1; }

need_cmd() {
    command -v "$1" &>/dev/null || error "required command not found: $1"
}

# ── checks ────────────────────────────────────────────────────────────────────

# Must be macOS
[ "$(uname -s)" = "Darwin" ] || error "This installer is for macOS only."

need_cmd curl
need_cmd uname

# Sudo only if install dir isn't writable
if [ -w "$INSTALL_DIR" ]; then
    SUDO=""
else
    need_cmd sudo
    SUDO="sudo"
fi

# ── detect update vs fresh install ───────────────────────────────────────────

IS_UPDATE=false
if command -v "$BINARY" &>/dev/null; then
    CURRENT_VERSION=$(rpm2 --version 2>/dev/null || echo "unknown")
    echo "Updating rpm2 ($CURRENT_VERSION → latest)..."
    IS_UPDATE=true
else
    echo "Installing rpm2..."
fi

info "Detected architecture: $ARCH"

# ── Rosetta check (arm64 Macs running x86_64 binary) ─────────────────────────

if [ "$ARCH" = "arm64" ] && [ -f "/usr/local/bin/rpm2" ]; then
    EXISTING_ARCH=$(file /usr/local/bin/rpm2 2>/dev/null || true)
    if echo "$EXISTING_ARCH" | grep -q "x86_64"; then
        warn "Replacing x86_64 binary with native arm64 build."
    fi
fi

# ── stop daemon before replacing binary ───────────────────────────────────────

if $IS_UPDATE; then
    info "Stopping rpm2 daemon (if running)..."
    rpm2 kill 2>/dev/null || true
    sleep 0.4
fi

# ── download ──────────────────────────────────────────────────────────────────

info "Downloading from $DOWNLOAD_URL"

rm -f "$TMP_PATH"

if ! curl -fsSL --progress-bar "$DOWNLOAD_URL" -o "$TMP_PATH"; then
    error "Download failed. Check your connection or visit: https://github.com/$REPO/releases"
fi

# ── validate ──────────────────────────────────────────────────────────────────

if [ ! -s "$TMP_PATH" ]; then
    error "Downloaded file is empty."
fi

# Check Mach-O magic bytes:
#   0xFEEDFACE = 32-bit Mach-O
#   0xFEEDFACF = 64-bit Mach-O
#   0xCAFEBABE = Universal (fat) binary
MAGIC=$(xxd -p -l 4 "$TMP_PATH" 2>/dev/null || od -A n -t x1 -N 4 "$TMP_PATH" | tr -d ' \n')
case "$MAGIC" in
    feedfacf|cefaedfe|cafebabe|bebafeca)
        info "Binary validation OK (Mach-O)" ;;
    *)
        rm -f "$TMP_PATH"
        error "Downloaded file is not a valid macOS binary (bad Mach-O header). The release may not exist yet." ;;
esac

# Verify the binary matches the expected architecture (skip for fat binaries)
if [ "$MAGIC" != "cafebabe" ] && [ "$MAGIC" != "bebafeca" ]; then
    FILE_INFO=$(file "$TMP_PATH" 2>/dev/null || true)
    if [ "$ARCH" = "arm64" ] && ! echo "$FILE_INFO" | grep -q "arm64"; then
        rm -f "$TMP_PATH"
        error "Downloaded binary is not an arm64 build. Check the release assets."
    fi
    if [ "$ARCH" = "x86_64" ] && ! echo "$FILE_INFO" | grep -q "x86_64"; then
        rm -f "$TMP_PATH"
        error "Downloaded binary is not an x86_64 build. Check the release assets."
    fi
fi

chmod +x "$TMP_PATH"

# ── Gatekeeper / quarantine ───────────────────────────────────────────────────

# macOS quarantines binaries downloaded by browsers/curl.
# xattr -d removes the quarantine flag so Gatekeeper doesn't block execution.
if command -v xattr &>/dev/null; then
    xattr -d com.apple.quarantine "$TMP_PATH" 2>/dev/null || true
fi

# ── install ───────────────────────────────────────────────────────────────────

# Ensure install dir exists (e.g. on fresh macOS /usr/local/bin may not exist)
if [ ! -d "$INSTALL_DIR" ]; then
    info "Creating $INSTALL_DIR"
    $SUDO mkdir -p "$INSTALL_DIR"
fi

info "Installing to $INSTALL_PATH"

if ! $SUDO mv "$TMP_PATH" "$INSTALL_PATH"; then
    rm -f "$TMP_PATH"
    error "Failed to install binary to $INSTALL_PATH"
fi

# ── PATH check ────────────────────────────────────────────────────────────────

if ! echo "$PATH" | grep -qF "$INSTALL_DIR"; then
    echo ""
    warn "$INSTALL_DIR is not in your PATH."

    # Detect shell and suggest the right profile file
    SHELL_NAME=$(basename "$SHELL")
    case "$SHELL_NAME" in
        zsh)  PROFILE="~/.zshrc" ;;
        bash) PROFILE="~/.bash_profile" ;;
        fish) PROFILE="~/.config/fish/config.fish" ;;
        *)    PROFILE="your shell profile" ;;
    esac

    if [ "$SHELL_NAME" = "fish" ]; then
        warn "Add this to $PROFILE:"
        echo ""
        echo "    fish_add_path $INSTALL_DIR"
    else
        warn "Add this to $PROFILE:"
        echo ""
        echo "    export PATH=\"\$PATH:$INSTALL_DIR\""
    fi
    echo ""
fi

# ── verify ────────────────────────────────────────────────────────────────────

NEW_VERSION=$("$INSTALL_PATH" --version 2>/dev/null || echo "unknown")

if $IS_UPDATE; then
    success "Updated to $NEW_VERSION"
else
    success "Installed $NEW_VERSION — run 'rpm2 --help' to get started"
fi