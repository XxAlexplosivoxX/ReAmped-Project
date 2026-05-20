#!/bin/bash
# Master build script for all platforms

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR/.."

usage() {
    echo "ReAmped Installer Builder"
    echo ""
    echo "Usage: $0 [--all|--windows|--arch|--debian]"
    echo ""
    echo "Options:"
    echo "  --all       Build installers for all platforms"
    echo "  --windows   Build Windows .msi installer"
    echo "  --arch      Build Arch Linux package"
    echo "  --debian    Build Debian/Ubuntu package"
    echo ""
}

build_all() {
    echo "=========================================="
    echo "Building ReAmped for all platforms"
    echo "=========================================="
    echo ""
    
    echo "1️⃣  Windows (.msi)..."
    bash "$SCRIPT_DIR/build-windows.sh" || echo "⚠️  Windows build skipped (WiX not installed)"
    echo ""
    
    echo "2️⃣  Arch Linux..."
    bash "$SCRIPT_DIR/build-arch.sh"
    echo ""
    
    echo "3️⃣  Debian/Ubuntu..."
    bash "$SCRIPT_DIR/build-debian.sh"
    echo ""
    
    echo "=========================================="
    echo "✅ All builds completed!"
    echo "=========================================="
}

if [ $# -eq 0 ]; then
    usage
    exit 0
fi

case "$1" in
    --all)
        build_all
        ;;
    --windows)
        bash "$SCRIPT_DIR/build-windows.sh"
        ;;
    --arch)
        bash "$SCRIPT_DIR/build-arch.sh"
        ;;
    --debian)
        bash "$SCRIPT_DIR/build-debian.sh"
        ;;
    *)
        usage
        exit 1
        ;;
esac
