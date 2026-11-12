#!/bin/sh
# Usage: APPIMAGETOOL=/absolute/appimagetool ./scripts/package-linux.sh /outside/repo /absolute/binary
set -eu
cd "$(dirname "$0")/.."
repo=$(pwd -P)
out=${1:?Specify an output directory outside the checkout}
binary=${2:?Specify the Debian 12 baseline release executable}
mkdir -p "$out"
out=$(realpath "$out")
case "$out/" in "$repo/"*) echo "Release artifacts must be outside the checkout" >&2; exit 2;; esac
test "$(uname -s)" = Linux
test "$(uname -m)" = x86_64
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)
test "$("$binary" --version)" = "linux-task-manager $version"
baseline=$(objdump -T "$binary" | sed -n 's/.*(GLIBC_\([0-9.]*\)).*/\1/p' | sort -Vu | tail -1)
if test "$(printf '%s\n' "$baseline" 2.36 | sort -V | tail -1)" != 2.36; then
  echo "Binary needs glibc $baseline; build with packaging/Dockerfile (baseline 2.36)" >&2
  exit 2
fi
tool=${APPIMAGETOOL:?Set APPIMAGETOOL to the verified appimagetool 1.9.1 executable}
runtime=${APPIMAGE_RUNTIME:?Set APPIMAGE_RUNTIME to the verified type2 runtime (see docs/releasing.md)}
printf '%s  %s\n' 617aa77314d57c8b8e4fae9c3f0b35af7b30d5529cb69e98f64f92510ed98a46 "$runtime" | sha256sum -c -
stage=$(mktemp -d "$out/.packaging.XXXXXX")
trap 'rm -rf "$stage"' EXIT HUP INT TERM
name=linux-task-manager-$version-linux-x86_64
portable=$stage/$name
mkdir -p "$portable"
install -m755 "$binary" "$portable/linux-task-manager"
cp LICENSE README.md CHANGELOG.md ATTRIBUTION.md THIRD-PARTY-LICENSES.txt "$portable/"
cp -R docs "$portable/"
cp assets/linux-task-manager.svg packaging/linux-task-manager.desktop "$portable/"
tar -czf "$out/$name.tar.gz" -C "$stage" "$name"

deb=$stage/deb
mkdir -p "$deb/DEBIAN" "$deb/usr/bin" "$deb/usr/share/applications" "$deb/usr/share/icons/hicolor/scalable/apps" "$deb/usr/share/doc/linux-task-manager"
install -m755 "$binary" "$deb/usr/bin/linux-task-manager"
cp packaging/linux-task-manager.desktop "$deb/usr/share/applications/"
cp assets/linux-task-manager.svg "$deb/usr/share/icons/hicolor/scalable/apps/"
cp LICENSE "$deb/usr/share/doc/linux-task-manager/copyright"
cp README.md CHANGELOG.md ATTRIBUTION.md THIRD-PARTY-LICENSES.txt "$deb/usr/share/doc/linux-task-manager/"
size=$(du -sk "$deb/usr" | cut -f1)
cat > "$deb/DEBIAN/control" <<EOF
Package: linux-task-manager
Version: $version
Section: utils
Priority: optional
Architecture: amd64
Maintainer: damgru <damgru@users.noreply.github.com>
Homepage: https://github.com/damgru/linux-task-manager
Installed-Size: $size
Depends: libc6 (>= 2.36), libgcc-s1, libx11-6, libxcursor1, libxrandr2, libxi6, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libwayland-cursor0, libwayland-egl1, libegl1, libgl1
Recommends: pkexec, polkitd
Description: Native Linux task manager with a Windows-inspired interface
 Monitor processes and resources, manage tasks and systemd services.
 English and Polish interfaces; runs as an ordinary user.
EOF
dpkg-deb --root-owner-group --build "$deb" "$out/$name.deb"

appdir=$stage/AppDir
mkdir -p "$appdir/usr/bin" "$appdir/usr/share/doc/linux-task-manager" "$appdir/usr/share/metainfo" "$appdir/usr/share/applications"
install -m755 "$binary" "$appdir/usr/bin/linux-task-manager"
install -m755 packaging/AppRun "$appdir/AppRun"
cp packaging/linux-task-manager.desktop "$appdir/com.github.damgru.linux-task-manager.desktop"
cp packaging/linux-task-manager.desktop "$appdir/usr/share/applications/com.github.damgru.linux-task-manager.desktop"
cp assets/linux-task-manager.svg "$appdir/"
cp LICENSE ATTRIBUTION.md THIRD-PARTY-LICENSES.txt "$appdir/usr/share/doc/linux-task-manager/"
cp packaging/APPIMAGE-RUNTIME-LICENSE.txt "$appdir/usr/share/doc/linux-task-manager/"
cp packaging/linux-task-manager.metainfo.xml "$appdir/usr/share/metainfo/com.github.damgru.linux-task-manager.appdata.xml"
# Do not bundle the host's glibc or GPU drivers. Runtime libraries come from Linux.
ARCH=x86_64 "$tool" --appimage-extract-and-run --runtime-file "$runtime" "$appdir" "$out/$name.AppImage"
chmod +x "$out/$name.AppImage"
(cd "$out" && sha256sum "$name.tar.gz" "$name.deb" "$name.AppImage" > SHA256SUMS.txt)
printf 'Created Linux x86_64 packages in %s\n' "$out"
