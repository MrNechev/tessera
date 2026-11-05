#!/bin/sh
# Stage libheif's HEVC decoder plugin (Ubuntu/Debian: libheif-plugin-libde265)
# so the AppImage can open HEIC photos. Used with tauri.appimage.conf.json:
#   packaging/appimage/stage-heif-plugin.sh
#   npm run tauri build -- --bundles appimage --config packaging/appimage/tauri.appimage.conf.json
set -eu
out="$(dirname "$0")/../../target/appimage-extra"
plugin=$(find /usr/lib /usr/lib64 -path '*libheif/plugins/libheif-libde265.so' 2>/dev/null | head -n1)
lib=$(find /usr/lib /usr/lib64 -name 'libde265.so.0' 2>/dev/null | head -n1)
if [ -z "$plugin" ] || [ -z "$lib" ]; then
  echo "libheif-libde265 plugin or libde265 not found (install libheif-plugin-libde265)" >&2
  exit 1
fi
mkdir -p "$out"
cp -L "$plugin" "$out/libheif-libde265.so"
cp -L "$lib" "$out/libde265.so.0"
# The plugin lives in usr/lib/libheif/plugins; libde265 two levels up in usr/lib.
patchelf --set-rpath '$ORIGIN/../..' "$out/libheif-libde265.so"
echo "Staged $(basename "$plugin") and $(basename "$lib") in $out"
