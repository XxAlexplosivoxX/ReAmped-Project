#!/bin/bash
# Build script for Arch Linux package

set -e

echo "Building ReAmped for Arch Linux..."
cd "$(dirname "$0")/.."

# Prepare source tarball
VERSION="1.0.0"
PKGNAME="reamped-${VERSION}"
mkdir -p /tmp/reamped-build
tar --exclude=target --exclude=.git -czf /tmp/reamped-build/${PKGNAME}.tar.gz .

# Create build directory
mkdir -p packaging/arch/build
cd packaging/arch/build

# Extract and prepare
tar -xzf /tmp/reamped-build/${PKGNAME}.tar.gz
cd $PKGNAME

# Build
echo "Compiling..."
cargo build --release --all

# Create package directory structure
mkdir -p pkg/usr/bin
mkdir -p pkg/usr/share/applications
mkdir -p pkg/usr/share/licenses/reamped

cp target/release/ReAmped pkg/usr/bin/reamped
cp ~/.local/share/applications/ReAmped.desktop pkg/usr/share/applications/reamped.desktop
cp LICENSE pkg/usr/share/licenses/reamped/

# Create .tar.zst package (Arch standard)
cd pkg
tar -czf ../reamped-${VERSION}-1-x86_64.pkg.tar.zst *

echo "✅ Arch package created: packaging/arch/build/reamped-${VERSION}-1-x86_64.pkg.tar.zst"
echo "📦 To install: sudo pacman -U reamped-${VERSION}-1-x86_64.pkg.tar.zst"
