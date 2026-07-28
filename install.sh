#!/usr/bin/env bash
set -euo pipefail

APP_NAME="ReAmped"
BINARY_NAME="reamped"
VERSION="1.2.0"

SRC_DIR="$(cd "$(dirname "$0")" && pwd)"
BUILD_DIR="$SRC_DIR/desktop/target/release"
BINARY_PATH="$BUILD_DIR/$APP_NAME"

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
REBUILD=true
while [[ $# -gt 0 ]]; do
    case "$1" in
        --no-build) REBUILD=false; shift ;;
        --system)   INSTALL_SCOPE="system"; shift ;;
        --user)     INSTALL_SCOPE="user"; shift ;;
        --help|-h)
            echo "Usage: $0 [--no-build] [--system|--user]"
            echo "  --no-build   Skip cargo build (use existing binary)"
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

echo "=== $APP_NAME v$VERSION Installer ==="
echo "Scope:     $INSTALL_SCOPE"
echo "Binary:    $BIN_DIR/$BINARY_NAME"
echo "Data:      $DATA_DIR"
echo "Icon:      $DATA_DIR/reamped.png"
echo "Desktop:   $DESKTOP_DIR/reamped.desktop"
echo ""

# ── Build ────────────────────────────────────────────────────────────────
if $REBUILD; then
    echo ">> Building $APP_NAME (cargo build --release)..."
    cd "$SRC_DIR/desktop"
    cargo build --release
    cd "$SRC_DIR"
else
    if [[ ! -f "$BINARY_PATH" ]]; then
        echo "ERROR: Binary not found at $BINARY_PATH. Use without --no-build or build first."
        exit 1
    fi
fi

# ── Install binary ───────────────────────────────────────────────────────
echo ">> Installing binary..."
mkdir -p "$BIN_DIR"
install -m 755 "$BINARY_PATH" "$BIN_DIR/$BINARY_NAME"
echo "   Installed: $BIN_DIR/$BINARY_NAME"

# ── Install data assets ──────────────────────────────────────────────────
echo ">> Installing data assets..."
mkdir -p "$DATA_DIR"
if [[ -d "$SRC_DIR/assets" ]]; then
    cp -r "$SRC_DIR/assets"/* "$DATA_DIR/"
    echo "   Copied assets to $DATA_DIR"
fi

# ── Install icon ─────────────────────────────────────────────────────────
echo ">> Installing icon..."
ICO_SRC="$SRC_DIR/assets/ReAmped.ico"
if [[ -f "$ICO_SRC" ]]; then
    ICON_DST="$DATA_DIR/reamped.png"
    if command -v magick &>/dev/null; then
        magick "$ICO_SRC[0]" "$ICON_DST" 2>/dev/null
    elif command -v convert &>/dev/null; then
        convert "$ICO_SRC[0]" "$ICON_DST" 2>/dev/null
    else
        # No ImageMagick: just symlink from /usr/share/icons if possible
        echo "   WARNING: ImageMagick not found, skipping icon install"
        ICON_DST=""
    fi
    if [[ -f "$ICON_DST" ]]; then
        echo "   Installed icon: $ICON_DST"
    fi
else
    echo "   WARNING: $ICO_SRC not found"
fi

# ── Install desktop entry ────────────────────────────────────────────────
echo ">> Installing desktop entry..."
mkdir -p "$DESKTOP_DIR"
DESKTOP_SRC="$SRC_DIR/assets/ReAmped.desktop"
if [[ -f "$DESKTOP_SRC" ]]; then
    # Use absolute icon path if we installed one
    if [[ -n "${ICON_DST:-}" ]] && [[ -f "$ICON_DST" ]]; then
        sed "s|^Icon=.*|Icon=$ICON_DST|" "$DESKTOP_SRC" > "$DESKTOP_DIR/reamped.desktop"
    else
        install -m 644 "$DESKTOP_SRC" "$DESKTOP_DIR/reamped.desktop"
    fi
    echo "   Installed: $DESKTOP_DIR/reamped.desktop"
fi

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
            # Only add if not already present
            if ! grep -sqF "$BIN_DIR" "$RC_FILE" 2>/dev/null; then
                echo "" >> "$RC_FILE"
                echo "# Added by ReAmped installer" >> "$RC_FILE"
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
