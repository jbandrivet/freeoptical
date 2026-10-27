#!/bin/sh
# Construit freeoptical_<version>_amd64.deb à la racine du dépôt.
set -eu
cd "$(dirname "$0")/.."

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
DEB="freeoptical_${VERSION}_amd64.deb"
ROOT=target/deb/freeoptical

cargo build --release
rm -rf "$ROOT"
install -Dm755 target/release/freeoptical "$ROOT/usr/bin/freeoptical"
strip "$ROOT/usr/bin/freeoptical" 2>/dev/null || true
install -Dm644 packaging/freeoptical.desktop "$ROOT/usr/share/applications/freeoptical.desktop"
install -Dm644 README.md "$ROOT/usr/share/doc/freeoptical/README.md"
install -Dm644 LICENSE-MIT "$ROOT/usr/share/doc/freeoptical/LICENSE-MIT"
install -Dm644 LICENSE-APACHE "$ROOT/usr/share/doc/freeoptical/LICENSE-APACHE"
cat > "$ROOT/usr/share/doc/freeoptical/copyright" <<COPYRIGHT
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: freeoptical
Source: https://github.com/jbandrivet/freeoptical

Files: *
Copyright: 2026 Jean-Baptiste Andrivet
License: MIT or Apache-2.0
 Voir /usr/share/doc/freeoptical/LICENSE-MIT et
 /usr/share/common-licenses/Apache-2.0.
COPYRIGHT

mkdir -p "$ROOT/DEBIAN"
cat > "$ROOT/DEBIAN/control" <<CONTROL
Package: freeoptical
Version: $VERSION
Section: science
Priority: optional
Architecture: amd64
Installed-Size: $(du -sk "$ROOT/usr" | cut -f1)
Depends: libc6 (>= 2.34)
Maintainer: Jean-Baptiste Andrivet <jeanbaptisteandrivet@gmail.com>
Homepage: https://github.com/jbandrivet/freeoptical
Description: Logiciel open-source de conception et de simulation optique séquentielle
 FreeOptical est un logiciel de conception optique séquentielle développé en Rust.
 Il intègre un moteur de lancer de rayons 3D exact (Snell-Descartes, surfaces
 sphériques, coniques et asphériques, Spot Diagram, aberration sphérique,
 optimisation Damped Least Squares), une interface graphique egui, une interface
 TUI rétro style MS-DOS en français (freeoptical --tui) ainsi qu'un serveur
 Model Context Protocol (MCP) natif (freeoptical --mcp).
CONTROL

dpkg-deb --root-owner-group -b "$ROOT" "$DEB"
echo "$DEB"
