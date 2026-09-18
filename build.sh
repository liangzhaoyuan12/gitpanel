#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
PROJECT_DIR="$(pwd)"
BUILD_DIR="$PROJECT_DIR/build"
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
PKG_NAME="gitpanel"
TARGET_DIR="$PROJECT_DIR/target/release"
BINARY="$TARGET_DIR/$PKG_NAME"
ICONS_SRC="$PROJECT_DIR/data/icons/hicolor"
DESKTOP_SRC="$PROJECT_DIR/data/io.github.liangzhaoyuan12.gitpanel.desktop"
LICENSE_FILE="$PROJECT_DIR/LICENCE"

# ── 架构检测 ──────────────────────────────────────
detect_arch() {
    local native
    native=$(uname -m)
    if [ -n "${CARGO_BUILD_TARGET:-}" ]; then
        case "$CARGO_BUILD_TARGET" in
            x86_64-*)       echo "x86_64" ;;
            aarch64-*)      echo "aarch64" ;;
            armv7-*)        echo "armv7" ;;
            arm-*)          echo "armv6" ;;
            riscv64-*)      echo "riscv64" ;;
            loongarch64-*)  echo "loongarch64" ;;
            *)              echo "$native" ;;
        esac
    else
        echo "$native"
    fi
}

arch_to_deb() {
    case "$1" in
        x86_64)       echo "amd64" ;;
        aarch64)      echo "arm64" ;;
        armv7)        echo "armhf" ;;
        armv6)        echo "armel" ;;
        riscv64)      echo "riscv64" ;;
        loongarch64)  echo "loong64" ;;
        *)            echo "$1" ;;
    esac
}

arch_to_rpm() {
    case "$1" in
        x86_64)       echo "x86_64" ;;
        aarch64)      echo "aarch64" ;;
        armv7)        echo "armv7hl" ;;
        armv6)        echo "armv6hl" ;;
        riscv64)      echo "riscv64" ;;
        loongarch64)  echo "loong64" ;;
        *)            echo "$1" ;;
    esac
}

HOST_ARCH=$(detect_arch)
DEB_ARCH=$(arch_to_deb "$HOST_ARCH")
RPM_ARCH=$(arch_to_rpm "$HOST_ARCH")

echo "=== GitPanel Build Script ==="
echo "Version:  $VERSION"
echo "Arch:     $HOST_ARCH (deb=$DEB_ARCH, rpm=$RPM_ARCH)"

# ── 构建前依赖检查 ────────────────────────────────
# libgit2 由 libgit2-sys 从源码静态编译进二进制，运行时不需要。
# 真正的运行时依赖：GTK4、libadwaita、OpenSSL、zlib（均通过二进制动态链接）
check_build_deps() {
    echo ""
    echo ">>> Checking build dependencies ..."
    local missing=()

    command -v cargo    >/dev/null 2>&1 || missing+=("rust/cargo")
    command -v pkg-config >/dev/null 2>&1 || missing+=("pkg-config")

    if command -v pkg-config >/dev/null 2>&1; then
        if ! pkg-config --atleast-version=4.12 gtk4 2>/dev/null; then
            missing+=("gtk4-devel >= 4.12 (pkg-config: gtk4)")
        fi
        if ! pkg-config --atleast-version=1.5 libadwaita-1 2>/dev/null; then
            missing+=("libadwaita-devel >= 1.5 (pkg-config: libadwaita-1)")
        fi
        if ! pkg-config --atleast-version=3.0 openssl 2>/dev/null; then
            missing+=("openssl-devel >= 3.0 (pkg-config: openssl)")
        fi
        if ! pkg-config --atleast-version=1.2 zlib 2>/dev/null; then
            missing+=("zlib-devel >= 1.2 (pkg-config: zlib)")
        fi
    fi

    command -v cmake >/dev/null 2>&1 || missing+=("cmake")

    if [ ${#missing[@]} -gt 0 ]; then
        echo ""
        echo "ERROR: Missing build dependencies:"
        for dep in "${missing[@]}"; do
            echo "  - $dep"
        done
        echo ""
        echo "Install hints:"
        echo "  Debian/Ubuntu: sudo apt-get install -y build-essential pkg-config cmake libgtk-4-dev libadwaita-1-dev libssl-dev zlib1g-dev"
        echo "  Fedora/RHEL:   sudo dnf install -y gcc pkg-config cmake gtk4-devel libadwaita-devel openssl-devel zlib-devel"
        echo "  Arch Linux:    sudo pacman -S base-devel pkgconf cmake gtk4 libadwaita openssl zlib"
        echo ""
        exit 1
    fi

    echo "    All build dependencies satisfied."
}

check_build_deps

# ── 构建 release ───────────────────────────────────
echo ""
echo ">>> Building release binary ..."
if [ -n "${CARGO_BUILD_TARGET:-}" ]; then
    cargo build --release --target "$CARGO_BUILD_TARGET"
    BINARY="$PROJECT_DIR/target/$CARGO_BUILD_TARGET/release/$PKG_NAME"
else
    cargo build --release
fi

if [ ! -f "$BINARY" ]; then
    echo "ERROR: binary not found at $BINARY"
    exit 1
fi

echo "    Binary: $BINARY ($(stat -c%s "$BINARY") bytes)"

# ── 准备 build 目录 ────────────────────────────────
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"

# ── 通用安装函数 ───────────────────────────────────
# 将 icons/hicolor 整个目录树复制到 $1/usr/share/icons/hicolor
# 这样 apps/ 和 actions/ 下的自定义图标都会被安装
install_icons() {
    local dest="$1"
    mkdir -p "$dest/usr/share/icons"
    cp -r "$ICONS_SRC" "$dest/usr/share/icons/"
}

# ──────────────────────────────────────────────────
# deb (Debian / Ubuntu)
# ──────────────────────────────────────────────────
echo ""
echo ">>> Building .deb ($DEB_ARCH) ..."
DEB_DIR="$BUILD_DIR/deb"
mkdir -p "$DEB_DIR/DEBIAN"
mkdir -p "$DEB_DIR/usr/bin"
mkdir -p "$DEB_DIR/usr/share/applications"
mkdir -p "$DEB_DIR/usr/share/doc/$PKG_NAME"

cat > "$DEB_DIR/DEBIAN/control" << EOF
Package: $PKG_NAME
Version: $VERSION
Section: devel
Priority: optional
Architecture: $DEB_ARCH
Maintainer: liangzhaoyuan12
Description: Browse and manage Git repositories
 A native GTK4 + libadwaita Git repository browser and manager.
 Supports branch operations, staging, commit history, diff viewing,
 file browsing, and syntax highlighting.
Homepage: https://github.com/liangzhaoyuan12/gitpanel
Depends: libgtk-4-1 (>= 4.12), libadwaita-1-0 (>= 1.5), libssl3
Recommends: git
EOF

cp "$BINARY" "$DEB_DIR/usr/bin/$PKG_NAME"
cp "$DESKTOP_SRC" "$DEB_DIR/usr/share/applications/io.github.liangzhaoyuan12.gitpanel.desktop"
install_icons "$DEB_DIR"

cp "$LICENSE_FILE" "$DEB_DIR/usr/share/doc/$PKG_NAME/copyright" 2>/dev/null || true
cat > "$DEB_DIR/usr/share/doc/$PKG_NAME/changelog.Debian" << EOF
gitpanel ($VERSION-1) unstable; urgency=medium

  * Initial release.

 -- liangzhaoyuan12 <liangzhaoyuan12>  $(date -R)
EOF
gzip -9n "$DEB_DIR/usr/share/doc/$PKG_NAME/changelog.Debian"

dpkg-deb --root-owner-group --build "$DEB_DIR" "$BUILD_DIR/${PKG_NAME}_${VERSION}_${DEB_ARCH}.deb"
echo "    -> $BUILD_DIR/${PKG_NAME}_${VERSION}_${DEB_ARCH}.deb"

# ──────────────────────────────────────────────────
# rpm (Fedora / RHEL / openSUSE)
# ──────────────────────────────────────────────────
echo ""
echo ">>> Building .rpm ($RPM_ARCH) ..."
RPM_DIR="$BUILD_DIR/rpm"
mkdir -p "$RPM_DIR"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

SPEC_FILE="$RPM_DIR/SPECS/$PKG_NAME.spec"

cat > "$SPEC_FILE" << EOF
Name:           $PKG_NAME
Version:        $VERSION
Release:        1%{?dist}
Summary:        Browse and manage Git repositories
License:        GPL-3.0-or-later
URL:            https://github.com/liangzhaoyuan12/gitpanel
BuildArch:      $RPM_ARCH

BuildRequires:  gcc
BuildRequires:  cmake
BuildRequires:  pkg-config
BuildRequires:  gtk4-devel >= 4.12
BuildRequires:  libadwaita-devel >= 1.5
BuildRequires:  openssl-devel >= 3.0
BuildRequires:  zlib-devel

Requires:       gtk4
Requires:       libadwaita
Requires:       openssl-libs
Recommends:     git

%description
A native GTK4 + libadwaita Git repository browser and manager.
Supports branch operations, staging, commit history, diff viewing,
file browsing, and syntax highlighting.

%install
mkdir -p %{buildroot}/usr/bin
mkdir -p %{buildroot}/usr/share/applications
mkdir -p %{buildroot}/usr/share/icons/hicolor
install -m 755 $BINARY %{buildroot}/usr/bin/$PKG_NAME
install -m 644 $DESKTOP_SRC %{buildroot}/usr/share/applications/io.github.liangzhaoyuan12.gitpanel.desktop
cp -r $ICONS_SRC/* %{buildroot}/usr/share/icons/hicolor/

%files
/usr/bin/$PKG_NAME
/usr/share/applications/io.github.liangzhaoyuan12.gitpanel.desktop
/usr/share/icons/hicolor/

%changelog
* $(LANG=C date '+%a %b %d %Y') liangzhaoyuan12 <liangzhaoyuan12> - $VERSION-1
- Initial release
EOF

rpmbuild --define "_topdir $RPM_DIR" -bb "$SPEC_FILE"
RPM_FILE=$(find "$RPM_DIR/RPMS" -name "${PKG_NAME}-${VERSION}*.rpm" -print -quit 2>/dev/null || true)
if [ -n "$RPM_FILE" ]; then
    mv "$RPM_FILE" "$BUILD_DIR/${PKG_NAME}_${VERSION}_${RPM_ARCH}.rpm"
fi
echo "    -> $BUILD_DIR/${PKG_NAME}_${VERSION}_${RPM_ARCH}.rpm"

# ──────────────────────────────────────────────────
# pacman (Arch Linux / Manjaro)
# ──────────────────────────────────────────────────
echo ""
echo ">>> Building .pkg.tar.zst ($RPM_ARCH) ..."
PACMAN_DIR="$BUILD_DIR/pacman"
mkdir -p "$PACMAN_DIR/usr/bin"
mkdir -p "$PACMAN_DIR/usr/share/applications"

cp "$BINARY" "$PACMAN_DIR/usr/bin/$PKG_NAME"
cp "$DESKTOP_SRC" "$PACMAN_DIR/usr/share/applications/io.github.liangzhaoyuan12.gitpanel.desktop"
install_icons "$PACMAN_DIR"

BINARY_SIZE=$(stat -c%s "$BINARY")
cat > "$PACMAN_DIR/.PKGINFO" << EOF
pkgname = $PKG_NAME
pkgver = $VERSION-1
pkgdesc = Browse and manage Git repositories
url = https://github.com/liangzhaoyuan12/gitpanel
builddate = $(date +%s)
packager = liangzhaoyuan12
size = $BINARY_SIZE
arch = $RPM_ARCH
license = GPL-3.0-or-later
depend = gtk4
depend = libadwaita
depend = openssl
depend = zlib
makedepends = gcc
makedepends = cmake
makedepends = pkgconf
makepkgopt = !strip
EOF

cd "$PACMAN_DIR"
tar -cf "$BUILD_DIR/${PKG_NAME}_${VERSION}_${RPM_ARCH}.pkg.tar" \
    .PKGINFO usr/
cd "$PROJECT_DIR"
if command -v zstd >/dev/null 2>&1; then
    zstd -q --rm -c "$BUILD_DIR/${PKG_NAME}_${VERSION}_${RPM_ARCH}.pkg.tar" \
        > "$BUILD_DIR/${PKG_NAME}_${VERSION}_${RPM_ARCH}.pkg.tar.zst"
    rm -f "$BUILD_DIR/${PKG_NAME}_${VERSION}_${RPM_ARCH}.pkg.tar"
else
    gzip -9 "$BUILD_DIR/${PKG_NAME}_${VERSION}_${RPM_ARCH}.pkg.tar"
    mv "$BUILD_DIR/${PKG_NAME}_${VERSION}_${RPM_ARCH}.pkg.tar.gz" \
       "$BUILD_DIR/${PKG_NAME}_${VERSION}_${RPM_ARCH}.pkg.tar.zst"
fi
echo "    -> $BUILD_DIR/${PKG_NAME}_${VERSION}_${RPM_ARCH}.pkg.tar.zst"

# ──────────────────────────────────────────────────
# tar.gz（通用 Linux）
# ──────────────────────────────────────────────────
echo ""
echo ">>> Building .tar.gz ($DEB_ARCH) ..."
TARGZ_NAME="${PKG_NAME}-${VERSION}-linux-$DEB_ARCH"
TARGZ_DIR="$BUILD_DIR/$TARGZ_NAME"
mkdir -p "$TARGZ_DIR"

cp "$BINARY" "$TARGZ_DIR/$PKG_NAME"
cp -r "$ICONS_SRC" "$TARGZ_DIR/icons/"
cp "$LICENSE_FILE" "$TARGZ_DIR/" 2>/dev/null || true

cat > "$TARGZ_DIR/README.md" << EOF
# GitPanel v$VERSION

A native GTK4 + libadwaita Git repository browser and manager.

## Features

- Repository browsing with branch/tag/stash support
- Staging area management (stage/unstage individual lines)
- Commit history with diff viewer
- File tree browsing with syntax highlighting
- Branch operations (create, delete, rename, merge)

## Dependencies (runtime)

- **GTK 4** (>= 4.12)
- **libadwaita** (>= 1.5)
- **OpenSSL** (>= 3.0)
- **git** (recommended, for CLI operations)

## Usage

\\\`\\\`\\\`
./$PKG_NAME [path/to/repo]
\\\`\\\`\\\`

If no path is given, opens a repository chooser.

Icons are bundled in the \`icons/\` directory.
To use system-wide, copy them:
\\\`\\\`\\\`
sudo cp -r icons/* /usr/share/icons/
\\\`\\\`\\\`

## Build from Source

### Debian / Ubuntu
\\\`\\\`\\\`
sudo apt-get install -y build-essential pkg-config cmake \\
    libgtk-4-dev libadwaita-1-dev libssl-dev zlib1g-dev
\\\`\\\`\\\`

### Fedora / RHEL
\\\`\\\`\\\`
sudo dnf install -y gcc pkg-config cmake \\
    gtk4-devel libadwaita-devel openssl-devel zlib-devel
\\\`\\\`\\\`

### Arch Linux
\\\`\\\`\\\`
sudo pacman -S base-devel pkgconf cmake gtk4 libadwaita openssl zlib
\\\`\\\`\\\`

Then:
\\\`\\\`\\\`
git clone https://github.com/liangzhaoyuan12/gitpanel.git
cd gitpanel
cargo build --release
\\\`\\\`\\\`

## License

GPL-3.0-or-later
EOF

cd "$BUILD_DIR"
tar -czf "${PKG_NAME}_${VERSION}_${DEB_ARCH}.tar.gz" "$TARGZ_NAME/"
cd "$PROJECT_DIR"
echo "    -> $BUILD_DIR/${PKG_NAME}_${VERSION}_${DEB_ARCH}.tar.gz"

# ── 汇总 ──────────────────────────────────────────
echo ""
echo "=== All packages built successfully ==="
echo ""
ls -lh "$BUILD_DIR"/*.{deb,rpm,pkg.tar.zst,tar.gz} 2>/dev/null || echo "(some packages may not have been generated)"
