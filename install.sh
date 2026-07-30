#!/usr/bin/env bash
set -euo pipefail

APP_NAME="ReAmped"
BINARY_NAME="reamped"
REPO="XxAlexplosivoxX/ReAmped-Project"

# ── Detect install mode ──────────────────────────────────────────────────
if [[ $EUID -eq 0 ]]; then
    INSTALL_SCOPE="system"
    BIN_DIR="/usr/local/bin"
    DATA_DIR="/usr/local/share/$APP_NAME"
    DESKTOP_DIR="/usr/local/share/applications"
else
    INSTALL_SCOPE="user"
    BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
    DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/$APP_NAME"
    DESKTOP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
fi

# ── Parse flags ──────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
    case "$1" in
        --system)   INSTALL_SCOPE="system"; shift ;;
        --user)     INSTALL_SCOPE="user"; shift ;;
        --help|-h)
            echo "Usage: $0 [--system|--user]"
            echo "  --system     Install system-wide (/usr/local)"
            echo "  --user       Install per-user (~/.local)"
            exit 0 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

# Recalculate paths after flag overrides
if [[ "$INSTALL_SCOPE" == "system" ]]; then
    BIN_DIR="/usr/local/bin"
    DATA_DIR="/usr/local/share/$APP_NAME"
    DESKTOP_DIR="/usr/local/share/applications"
else
    BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
    DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/$APP_NAME"
    DESKTOP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
fi

echo "=== $APP_NAME Installer (latest release) ==="
echo "Scope:     $INSTALL_SCOPE"
echo "Binary:    $BIN_DIR/$BINARY_NAME"
echo ""

# ── Download binary from latest GitHub release ──────────────────────────
RELEASE_URL="https://api.github.com/repos/$REPO/releases/latest"
echo ">> Fetching latest release info..."
TAG=$(curl -sL "$RELEASE_URL" | grep '"tag_name":' | sed 's/.*"tag_name": "\(.*\)",/\1/')
if [[ -z "$TAG" ]]; then
    echo "ERROR: Could not determine latest release tag"
    exit 1
fi
echo "   Latest release: $TAG"

DOWNLOAD_URL="https://github.com/$REPO/releases/download/$TAG/$APP_NAME"
echo ">> Downloading $APP_NAME binary..."
mkdir -p "$BIN_DIR"
curl -#L "$DOWNLOAD_URL" -o "$BIN_DIR/$BINARY_NAME"
chmod +x "$BIN_DIR/$BINARY_NAME"
echo "   Installed: $BIN_DIR/$BINARY_NAME"

# ── Add ~/.local/bin to PATH if not present ──────────────────────────────
if [[ "$INSTALL_SCOPE" == "user" ]]; then
    if ! echo "$PATH" | tr ':' '\n' | grep -qx "$BIN_DIR"; then
        LINE=""
        RC_FILE=""
        FISH_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/fish"
        FISH_CONF="$FISH_DIR/config.fish"

        if [[ "$SHELL" == *fish ]] || [[ -f "$FISH_CONF" ]]; then
            mkdir -p "$FISH_DIR"
            RC_FILE="$FISH_CONF"
            LINE="fish_add_path $BIN_DIR"
        elif [[ -f "$HOME/.zshrc" ]]; then
            RC_FILE="$HOME/.zshrc"
            LINE="export PATH=\"\$PATH:$BIN_DIR\""
        elif [[ -f "$HOME/.bashrc" ]]; then
            RC_FILE="$HOME/.bashrc"
            LINE="export PATH=\"\$PATH:$BIN_DIR\""
        fi

        if [[ -n "$RC_FILE" ]]; then
            if ! grep -sqF "$BIN_DIR" "$RC_FILE" 2>/dev/null; then
                echo "" >> "$RC_FILE"
                echo "# Added by $APP_NAME installer" >> "$RC_FILE"
                echo "$LINE" >> "$RC_FILE"
                echo "   Added '$BIN_DIR' to PATH in $RC_FILE"
            fi
        else
            echo "   NOTICE: $BIN_DIR is not in PATH."
            echo "   Add this to your shell config:"
            echo "     export PATH=\"\$PATH:$BIN_DIR\""
        fi
    fi
fi

echo ""
echo "=== Installation complete! ==="
echo "Run 'reamped' to start the player."
