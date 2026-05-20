#!/bin/bash
# Build script for Debian package

set -e

echo "Building ReAmped for Debian/Ubuntu..."
cd "$(dirname "$0")/.."

# Install build dependencies if needed
echo "Installing build dependencies..."
sudo apt-get install -y debhelper-compat rustc cargo pkg-config libasound2-dev libssl-dev

# Prepare source
VERSION="1.0.0"
SRCDIR="reamped-${VERSION}"
mkdir -p packaging/debian/build
cd packaging/debian/build

# Create source structure
mkdir -p $SRCDIR/debian
cp -r ../../.. $SRCDIR/
cd $SRCDIR

# Copy debian files
cp ../../debian/* debian/

# Build package
echo "Building Debian package..."
debuild -us -uc

cd ..
echo "✅ Debian package created:"
echo "   - reamped_${VERSION}-1_amd64.deb"
echo "   - reamped_${VERSION}-1_amd64.build"
echo "   - reamped_${VERSION}-1.dsc"
echo ""
echo "📦 To install: sudo apt install ./reamped_${VERSION}-1_amd64.deb"
