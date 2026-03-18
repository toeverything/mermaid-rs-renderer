#!/usr/bin/env bash
set -euo pipefail

cat > aur-mmdr/PKGBUILD <<'PKGBUILD_TEMPLATE'
# Maintainer: Jeremy <jeremyhuang55555@gmail.com>

pkgname=mmdr-bin
pkgver=VERSION_PLACEHOLDER
pkgrel=1
pkgdesc="Fast Mermaid diagram renderer in pure Rust - 23 diagram types, 100-1400x faster than mermaid-cli"
arch=('x86_64')
url="https://github.com/1jehuang/mermaid-rs-renderer"
license=('MIT')
depends=('glibc')
provides=('mmdr')
conflicts=('mmdr')
source=(
  "mmdr-${pkgver}-x86_64-unknown-linux-gnu.tar.gz::https://github.com/1jehuang/mermaid-rs-renderer/releases/download/v${pkgver}/mmdr-x86_64-unknown-linux-gnu.tar.gz"
  "LICENSE::https://raw.githubusercontent.com/1jehuang/mermaid-rs-renderer/v${pkgver}/LICENSE"
)
sha256sums=(
  'SHA_LINUX_PLACEHOLDER'
  'SHA_LICENSE_PLACEHOLDER'
)

package() {
  install -Dm755 mmdr "${pkgdir}/usr/bin/mmdr"
  install -Dm644 "${srcdir}/LICENSE" "${pkgdir}/usr/share/licenses/${pkgname}/LICENSE"
}
PKGBUILD_TEMPLATE

sed -i "s/VERSION_PLACEHOLDER/${VERSION}/" aur-mmdr/PKGBUILD
sed -i "s/SHA_LINUX_PLACEHOLDER/${SHA_LINUX}/" aur-mmdr/PKGBUILD
sed -i "s/SHA_LICENSE_PLACEHOLDER/${SHA_LICENSE}/" aur-mmdr/PKGBUILD

cat > aur-mmdr/.SRCINFO <<EOF
pkgbase = mmdr-bin
	pkgdesc = Fast Mermaid diagram renderer in pure Rust - 23 diagram types, 100-1400x faster than mermaid-cli
	pkgver = ${VERSION}
	pkgrel = 1
	url = https://github.com/1jehuang/mermaid-rs-renderer
	arch = x86_64
	license = MIT
	depends = glibc
	provides = mmdr
	conflicts = mmdr
	source = mmdr-${VERSION}-x86_64-unknown-linux-gnu.tar.gz::https://github.com/1jehuang/mermaid-rs-renderer/releases/download/v${VERSION}/mmdr-x86_64-unknown-linux-gnu.tar.gz
	source = LICENSE::https://raw.githubusercontent.com/1jehuang/mermaid-rs-renderer/v${VERSION}/LICENSE
	sha256sums = ${SHA_LINUX}
	sha256sums = ${SHA_LICENSE}

pkgname = mmdr-bin
EOF

echo "Updated AUR package to v${VERSION}"
