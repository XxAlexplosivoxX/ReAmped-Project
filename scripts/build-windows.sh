#!/bin/bash
# Build script for Windows .msi installer (or portable .zip on Linux)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR/.."

echo "Building ReAmped for Windows..."
cd "$PROJECT_DIR"

# Create WiX output directory
mkdir -p wix/build

# Detect OS
if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "win32" ]] || command -v wmic &> /dev/null; then
    # Windows
    echo "🪟 Windows detected - building .msi..."
    
    # Build release binary for Windows
    echo "Compiling for Windows..."
    cargo build --release
    
    # Check if WiX Toolset is installed
    if ! command -v candle.exe &> /dev/null; then
        echo "❌ WiX Toolset not found. Install from: https://wixtoolset.org/"
        exit 1
    fi
    
    echo "Generating WiX installer..."
    cd wix
    
    # Create obj directory
    mkdir -p obj
    
    # Compile WiX source
    candle.exe -d PlatformVersion=v100 -arch x64 ReAmped.wxs -o obj/
    light.exe -ext WixUIExtension -cultures:en-us obj/ReAmped.wixobj -o build/ReAmped-1.0.0.msi
    
    echo "✅ Windows installer created: wix/build/ReAmped-1.0.0.msi"
else
    # Linux / macOS - create portable .zip instead
    echo "🐧 Linux/macOS detected - building portable .zip..."
    
    echo "Compiling for current platform..."
    cargo build --release
    
    # Get binary path based on platform
    if [[ "$OSTYPE" == "linux"* ]]; then
        BINARY="$PROJECT_DIR/target/release/ReAmped"
        ZIP_NAME="ReAmped-1.0.0-linux-x64.zip"
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        BINARY="$PROJECT_DIR/target/release/ReAmped"
        ZIP_NAME="ReAmped-1.0.0-macos.zip"
    fi
    
    if [ ! -f "$BINARY" ]; then
        echo "❌ Binary not found at $BINARY"
        exit 1
    fi
    
    # Create portable zip
    cd wix/build
    mkdir -p ReAmped-portable
    cp "$BINARY" ReAmped-portable/
    cp "$PROJECT_DIR/LICENSE" ReAmped-portable/
    cp "$PROJECT_DIR/README.md" ReAmped-portable/
    cp "$PROJECT_DIR/assets/fonts/"* ReAmped-portable/ 2>/dev/null || true
    
    zip -r "$ZIP_NAME" ReAmped-portable/
    rm -rf ReAmped-portable
    
    echo "✅ Portable archive created: wix/build/$ZIP_NAME"
    echo "📦 To run: unzip and ./ReAmped"
fi
