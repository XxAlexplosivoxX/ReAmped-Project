#!/usr/bin/env bash
set -euo pipefail

APP_NAME="ReAmped"
BINARY_NAME="reamped"
REPO="XxAlexplosivoxX/ReAmped-Project"
RAW_BASE="https://raw.githubusercontent.com/$REPO/main"

# ── Detect install scope ─────────────────────────────────────────────────
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

MODE="download"
SRC_DIR=""

# ── Parse flags ──────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
    case "$1" in
        --build)        MODE="build"; shift ;;
        --system)       INSTALL_SCOPE="system"; shift ;;
        --user)         INSTALL_SCOPE="user"; shift ;;
        --help|-h)
            echo "Usage: $0 [--build] [--system|--user]"
            echo "  --build      Build from source instead of downloading binary"
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

echo "=== $APP_NAME Installer ==="
echo "Scope:     $INSTALL_SCOPE"
echo "Mode:      $MODE"
echo "Binary:    $BIN_DIR/$BINARY_NAME"
echo ""

# ── Build or Download ────────────────────────────────────────────────────
if [[ "$MODE" == "build" ]]; then
    SRC_DIR="$(cd "$(dirname "$0")" && pwd)"
    BUILD_DIR="$SRC_DIR/desktop/target/release"
    BINARY_PATH="$BUILD_DIR/$APP_NAME"

    echo ">> Building $APP_NAME (cargo build --release)..."
    cd "$SRC_DIR/desktop"
    cargo build --release
    cd "$SRC_DIR"

    echo ">> Installing binary..."
    mkdir -p "$BIN_DIR"
    install -m 755 "$BINARY_PATH" "$BIN_DIR/$BINARY_NAME"
    echo "   Installed: $BIN_DIR/$BINARY_NAME"
else
    # ── Download from latest GitHub release ──────────────────────────────
    RELEASE_URL="https://api.github.com/repos/$REPO/releases/latest"
    TAG=$(curl -sL "$RELEASE_URL" | grep '"tag_name":' | sed 's/.*"tag_name": "\(.*\)",/\1/')
    if [[ -z "$TAG" ]]; then
        echo "ERROR: Could not determine latest release tag"
        exit 1
    fi
    echo ">> Latest release: $TAG"

    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$TAG/$APP_NAME"
    echo ">> Downloading $APP_NAME binary..."
    mkdir -p "$BIN_DIR"
    curl -#L "$DOWNLOAD_URL" -o "$BIN_DIR/$BINARY_NAME"
    chmod +x "$BIN_DIR/$BINARY_NAME"
    echo "   Installed: $BIN_DIR/$BINARY_NAME"
fi

# ── Install icon ─────────────────────────────────────────────────────────
echo ">> Installing icon..."
mkdir -p "$DATA_DIR"
ICON_DST="$DATA_DIR/reamped.png"

if [[ "$MODE" == "build" ]]; then
    ICO_SRC="$SRC_DIR/assets/ReAmped.ico"
    if [[ -f "$ICO_SRC" ]]; then
        if command -v magick &>/dev/null; then
            magick "$ICO_SRC[0]" "$ICON_DST" 2>/dev/null
        elif command -v convert &>/dev/null; then
            convert "$ICO_SRC[0]" "$ICON_DST" 2>/dev/null
        else
            echo "   WARNING: ImageMagick not found, skipping icon install"
            ICON_DST=""
        fi
    else
        echo "   WARNING: $ICO_SRC not found"
        ICON_DST=""
    fi
else
    ICO_TMP=$(mktemp)
    if curl -sL -o "$ICO_TMP" "$RAW_BASE/assets/ReAmped.ico" 2>/dev/null; then
        if command -v magick &>/dev/null; then
            magick "$ICO_TMP[0]" "$ICON_DST" 2>/dev/null && echo "   Converted icon from .ico"
        elif command -v convert &>/dev/null; then
            convert "$ICO_TMP[0]" "$ICON_DST" 2>/dev/null && echo "   Converted icon from .ico"
        else
            echo "   WARNING: ImageMagick not found, skipping icon install"
            ICON_DST=""
        fi
    else
        echo "   WARNING: Could not download icon, skipping"
        ICON_DST=""
    fi
    rm -f "$ICO_TMP"
fi

if [[ -f "$ICON_DST" ]]; then
    echo "   Installed icon: $ICON_DST"
fi

# ── Install desktop entry ────────────────────────────────────────────────
echo ">> Installing desktop entry..."
mkdir -p "$DESKTOP_DIR"

DESKTOP_TMP=$(mktemp)
if [[ "$MODE" == "build" ]] && [[ -f "$SRC_DIR/assets/ReAmped.desktop" ]]; then
    cp "$SRC_DIR/assets/ReAmped.desktop" "$DESKTOP_TMP"
elif curl -sL -o "$DESKTOP_TMP" "$RAW_BASE/assets/ReAmped.desktop" 2>/dev/null; then
    :
else
    echo "   WARNING: Could not download desktop entry"
    rm -f "$DESKTOP_TMP"
    DESKTOP_TMP=""
fi

if [[ -n "${DESKTOP_TMP:-}" ]] && [[ -s "$DESKTOP_TMP" ]]; then
    if [[ -n "${ICON_DST:-}" ]] && [[ -f "$ICON_DST" ]]; then
        sed "s|^Icon=.*|Icon=$ICON_DST|" "$DESKTOP_TMP" > "$DESKTOP_DIR/reamped.desktop"
    else
        install -m 644 "$DESKTOP_TMP" "$DESKTOP_DIR/reamped.desktop"
    fi
    echo "   Installed: $DESKTOP_DIR/reamped.desktop"
fi
rm -f "${DESKTOP_TMP:-}"

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
