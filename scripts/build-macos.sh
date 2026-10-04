#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --release --locked
# Stage outside synced folders: file providers may re-add Finder metadata while signing.
stage_root=$(mktemp -d /tmp/pixelify-build.XXXXXX)
stage_app="$stage_root/Pixelify.app"
stage_icons="$stage_root/Pixelify.iconset"
mkdir -p "$stage_app/Contents/MacOS" "$stage_app/Contents/Resources" "$stage_icons" dist
rm -rf dist/Pixelify.app dist/Pixelify-macOS.zip
cp target/release/pixelify "$stage_app/Contents/MacOS/pixelify"
cp packaging/Info.plist "$stage_app/Contents/Info.plist"
cargo run --release --locked --example app_icon
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" target/app-icon.png --out "$stage_icons/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" target/app-icon.png --out "$stage_icons/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$stage_icons" -o "$stage_app/Contents/Resources/Pixelify.icns"
xattr -cr "$stage_app"
codesign --force --sign - "$stage_app"
codesign --verify --deep --strict "$stage_app"
# `dist` can be a File Provider-backed folder. Archive the clean, signed
# staging app before Finder/File Provider metadata can be added to its copy.
ditto --norsrc -c -k --keepParent "$stage_app" dist/Pixelify-macOS.zip
ditto --norsrc "$stage_app" dist/Pixelify.app
