#!/usr/bin/env bash
# Generate all Tauri icon variants from icon.png (expected 512x512 or larger)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SOURCE="$SCRIPT_DIR/icon.png"

if [ ! -f "$SOURCE" ]; then
  echo "Error: icon.png not found in $SCRIPT_DIR"
  exit 1
fi

echo "Generating icons from $SOURCE..."

# Standard PNG sizes
magick "$SOURCE" -resize 32x32   "$SCRIPT_DIR/32x32.png"
magick "$SOURCE" -resize 128x128 "$SCRIPT_DIR/128x128.png"
magick "$SOURCE" -resize 256x256 "$SCRIPT_DIR/128x128@2x.png"

# Windows Store logos
magick "$SOURCE" -resize 30x30   "$SCRIPT_DIR/Square30x30Logo.png"
magick "$SOURCE" -resize 44x44   "$SCRIPT_DIR/Square44x44Logo.png"
magick "$SOURCE" -resize 71x71   "$SCRIPT_DIR/Square71x71Logo.png"
magick "$SOURCE" -resize 89x89   "$SCRIPT_DIR/Square89x89Logo.png"
magick "$SOURCE" -resize 107x107 "$SCRIPT_DIR/Square107x107Logo.png"
magick "$SOURCE" -resize 142x142 "$SCRIPT_DIR/Square142x142Logo.png"
magick "$SOURCE" -resize 150x150 "$SCRIPT_DIR/Square150x150Logo.png"
magick "$SOURCE" -resize 284x284 "$SCRIPT_DIR/Square284x284Logo.png"
magick "$SOURCE" -resize 310x310 "$SCRIPT_DIR/Square310x310Logo.png"
magick "$SOURCE" -resize 50x50   "$SCRIPT_DIR/StoreLogo.png"

# Windows .ico (multi-size)
magick "$SOURCE" \
  \( -clone 0 -resize 16x16 \) \
  \( -clone 0 -resize 24x24 \) \
  \( -clone 0 -resize 32x32 \) \
  \( -clone 0 -resize 48x48 \) \
  \( -clone 0 -resize 64x64 \) \
  \( -clone 0 -resize 256x256 \) \
  -delete 0 "$SCRIPT_DIR/icon.ico"

# macOS .icns (iconset → icns via ImageMagick)
ICONSET=$(mktemp -d)/icon.iconset
mkdir -p "$ICONSET"
magick "$SOURCE" -resize 16x16     "$ICONSET/icon_16x16.png"
magick "$SOURCE" -resize 32x32     "$ICONSET/icon_16x16@2x.png"
magick "$SOURCE" -resize 32x32     "$ICONSET/icon_32x32.png"
magick "$SOURCE" -resize 64x64     "$ICONSET/icon_32x32@2x.png"
magick "$SOURCE" -resize 128x128   "$ICONSET/icon_128x128.png"
magick "$SOURCE" -resize 256x256   "$ICONSET/icon_128x128@2x.png"
magick "$SOURCE" -resize 256x256   "$ICONSET/icon_256x256.png"
magick "$SOURCE" -resize 512x512   "$ICONSET/icon_256x256@2x.png"
magick "$SOURCE" -resize 512x512   "$ICONSET/icon_512x512.png"

# Try iconutil (macOS) first, fall back to png2icns, then ImageMagick
if command -v iconutil &>/dev/null; then
  iconutil -c icns "$ICONSET" -o "$SCRIPT_DIR/icon.icns"
elif command -v png2icns &>/dev/null; then
  png2icns "$SCRIPT_DIR/icon.icns" "$ICONSET"/icon_*.png
else
  # ImageMagick can write icns directly
  magick "$SOURCE" -resize 512x512 "$SCRIPT_DIR/icon.icns"
fi

rm -rf "$(dirname "$ICONSET")"

# Welcome screen logo (public/logo.png) — copy source as-is (already 512x512)
cp "$SOURCE" "$SCRIPT_DIR/../../public/logo.png"

echo "Done! Generated all icons."
