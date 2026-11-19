#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
project=$(pwd)
cargo build --locked --release
metadata=$(cargo metadata --locked --no-deps --format-version 1)
version=$(echo "$metadata" | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["version"])')
target=$(echo "$metadata" | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
arch=$(dpkg --print-architecture)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
package="$work/yourdiary"
install -Dm755 "$target/release/YourDiary" "$package/usr/lib/yourdiary/YourDiary"
strip --strip-unneeded "$package/usr/lib/yourdiary/YourDiary"
install -Dm755 packaging/YourDiary "$package/usr/bin/YourDiary"
install -Dm644 resources/com.orlando.yourdiary.desktop "$package/usr/share/applications/com.orlando.yourdiary.desktop"
install -Dm644 resources/YourDiary.png "$package/usr/share/pixmaps/com.orlando.yourdiary.png"
install -Dm644 LICENSE "$package/usr/share/doc/yourdiary/copyright"
install -Dm644 README.md "$package/usr/share/doc/yourdiary/README.md"
mkdir -p "$work/debian" "$package/DEBIAN" dist
cat > "$work/debian/control" <<'CONTROL'
Source: yourdiary
Section: misc
Priority: optional
Maintainer: Orlando Vale <ljc2481534548@163.com>

Package: yourdiary
Architecture: any
Description: YourDiary GTK desktop client
CONTROL
cd "$work"
dpkg-shlibdeps -Odebian/substvars "$package/usr/lib/yourdiary/YourDiary"
depends=$(sed -n 's/^shlibs:Depends=//p' debian/substvars)
test -n "$depends"
cat > "$package/DEBIAN/control" <<CONTROL
Package: yourdiary
Version: $version
Section: misc
Priority: optional
Architecture: $arch
Maintainer: Orlando Vale <ljc2481534548@163.com>
Depends: $depends, ca-certificates
Description: YourDiary desktop client for diaries and Treehole
CONTROL
artifact="yourdiary_${version}_${arch}.deb"
cd "$project/dist"
dpkg-deb --root-owner-group --build "$package" "$artifact"
sha256sum "$artifact" > "$artifact.sha256"
