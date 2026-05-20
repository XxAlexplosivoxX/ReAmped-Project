# ReAmped Installer & Packaging Guide

This directory contains build configurations for creating ReAmped installers and packages for Windows, Arch Linux, and Debian/Ubuntu.

## Quick Start

### Build all installers:
```bash
./scripts/build-installers.sh --all
```

### Build specific platform:
```bash
./scripts/build-installers.sh --windows  # Windows .msi
./scripts/build-installers.sh --arch     # Arch Linux .pkg.tar.zst
./scripts/build-installers.sh --debian   # Debian .deb
```

---

## Windows (.msi)

**Prerequisites:**
- WiX Toolset 4.0+ (https://wixtoolset.org/)
- Windows Build Tools

**Build:**
```bash
./scripts/build-windows.sh
```

**Output:**
- `wix/build/ReAmped-1.0.0.msi`

**Features:**
- Start Menu shortcut
- Desktop shortcut
- Uninstaller support
- Per-machine installation

---

## Arch Linux

**Prerequisites:**
- `rustc`, `cargo`, `pkg-config`
- `base-devel` group (gcc, make, etc.)
- `alsa-lib`

**Build:**
```bash
./scripts/build-arch.sh
```

**Output:**
- `packaging/arch/build/reamped-1.0.0-1-x86_64.pkg.tar.zst`

**Install:**
```bash
sudo pacman -U packaging/arch/build/reamped-1.0.0-1-x86_64.pkg.tar.zst
```

**AUR Submission:**
1. Update `PKGBUILD` with correct `sha256sums`
2. Test locally: `makepkg -si`
3. Submit to AUR at https://aur.archlinux.org/

---

## Debian/Ubuntu

**Prerequisites:**
- Build tools: `sudo apt install build-essential debhelper cargo rustc`
- Dev libs: `libasound2-dev libssl-dev pkg-config`

**Build:**
```bash
./scripts/build-debian.sh
```

**Output:**
- `packaging/debian/build/reamped_1.0.0-1_amd64.deb`

**Install:**
```bash
sudo apt install ./packaging/debian/build/reamped_1.0.0-1_amd64.deb
```

**Supported distros:**
- Debian 11+
- Ubuntu 20.04+
- Linux Mint 20+

---

## Version Updates

To bump version across all packages:

1. Update `desktop/Cargo.toml`:
   ```toml
   [package]
   version = "1.1.0"
   ```

2. Update `packaging/arch/PKGBUILD`:
   ```bash
   pkgver=1.1.0
   ```

3. Update `packaging/debian/changelog`:
   ```
   reamped (1.1.0-1) unstable; urgency=medium
   ```

4. Update `wix/ReAmped.wxs`:
   ```xml
   <Product ... Version="1.1.0.0" ...>
   ```

---

## Build Details

### Windows WiX (.wxs)
- **File:** `wix/ReAmped.wxs`
- **Standard:** WiX 4.0
- Generates per-machine MSI installer with GUI
- Auto-detects Program Files directory

### Arch PKGBUILD
- **File:** `packaging/arch/PKGBUILD`
- **Format:** Standard Arch packaging
- Pulls from release tarball
- Supports x86_64 and aarch64

### Debian debian/control
- **Files:** 
  - `debian/control` — metadata
  - `debian/rules` — build rules
  - `debian/changelog` — history
  - `debian/source/format` — source format

---

## Troubleshooting

**Windows: "WiX Toolset not found"**
- Install WiX: https://wixtoolset.org/
- Add to PATH

**Arch: "cargo not found"**
- Install: `sudo pacman -S rustup`
- Run: `rustup toolchain install stable`

**Debian: Build fails**
- Update deps: `sudo apt update && sudo apt upgrade`
- Reinstall build-essential: `sudo apt reinstall build-essential`

---

## CI/CD Integration

These scripts can be integrated into GitHub Actions:

```yaml
- name: Build installers
  run: |
    chmod +x scripts/build-installers.sh
    ./scripts/build-installers.sh --all
    
- name: Upload artifacts
  uses: actions/upload-artifact@v3
  with:
    name: installers
    path: |
      wix/build/*.msi
      packaging/arch/build/*.pkg.tar.zst
      packaging/debian/build/*.deb
```

---

## Release Checklist

- [ ] Bump version in all packaging files
- [ ] Build all installers locally: `./scripts/build-installers.sh --all`
- [ ] Test Windows .msi on Windows 10/11
- [ ] Test Arch package: `sudo pacman -U ...`
- [ ] Test Debian package: `sudo apt install ...`
- [ ] Create GitHub release with all artifacts
- [ ] Submit to AUR (Arch Linux)
- [ ] Update package repos (if available)
