#!/usr/bin/env bash
# Baut die aktuelle native LuxiFer-Version und paketiert sie als AppImage.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGE="luxifer-native"
BINARY="luxifer-native"
APP_NAME="LuxiFer"
VERSION="$(awk -F '"' '/^version = "/ { print $2; exit }' \
  "$ROOT_DIR/Cargo.toml")"

case "$(uname -m)" in
  x86_64) APPIMAGE_ARCH="x86_64" ;;
  aarch64|arm64) APPIMAGE_ARCH="aarch64" ;;
  *)
    echo "Nicht unterstützte Architektur: $(uname -m)" >&2
    exit 1
    ;;
esac

BUILD_DIR="$ROOT_DIR/target/release"
DIST_DIR="$ROOT_DIR/dist"
TOOLS_DIR="$ROOT_DIR/.tools"
APPDIR="$DIST_DIR/AppDir"
OUTPUT="$DIST_DIR/${APP_NAME}-${VERSION}-${APPIMAGE_ARCH}.AppImage"

find_linuxdeploy() {
  if [[ -n "${LINUXDEPLOY:-}" ]]; then
    printf '%s\n' "$LINUXDEPLOY"
  elif command -v linuxdeploy >/dev/null 2>&1; then
    command -v linuxdeploy
  else
    printf '%s\n' "$TOOLS_DIR/linuxdeploy-${APPIMAGE_ARCH}.AppImage"
  fi
}

download_linuxdeploy() {
  local target="$1"
  local url="https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-${APPIMAGE_ARCH}.AppImage"

  if [[ -x "$target" ]]; then
    return
  fi
  if [[ "${NO_DOWNLOAD:-0}" == "1" ]]; then
    echo "linuxdeploy fehlt. Installiere es oder setze LINUXDEPLOY=/pfad/linuxdeploy." >&2
    exit 1
  fi

  mkdir -p "$TOOLS_DIR"
  echo "» Lade linuxdeploy für ${APPIMAGE_ARCH} …"
  if command -v curl >/dev/null 2>&1; then
    curl --fail --location --output "$target" "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget --output-document="$target" "$url"
  else
    echo "Für den linuxdeploy-Download wird curl oder wget benötigt." >&2
    exit 1
  fi
  chmod +x "$target"
}

echo "» Baue ${PACKAGE} ${VERSION} im Release-Profil …"
cargo build --manifest-path "$ROOT_DIR/Cargo.toml" --package "$PACKAGE" --release --locked

LINUXDEPLOY_BIN="$(find_linuxdeploy)"
if [[ ! -x "$LINUXDEPLOY_BIN" ]]; then
  download_linuxdeploy "$LINUXDEPLOY_BIN"
fi

mkdir -p "$DIST_DIR"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin"

install -Dm755 "$BUILD_DIR/$BINARY" "$APPDIR/usr/bin/$BINARY"
# Größenspezifische Icon-Fassungen (16–32 minimal, ab 48 mit Flügeln).
for size in 16 24 32 48 64 128 256 512; do
  install -Dm644 "$ROOT_DIR/luxifer/native/assets/icon/luxifer-$size.png" \
    "$APPDIR/usr/share/icons/hicolor/${size}x${size}/apps/luxifer.png"
done

mkdir -p "$APPDIR/usr/share/applications"
cat >"$APPDIR/usr/share/applications/luxifer.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=LuxiFer
Comment=Nativer Editor für Laserprojekte
Exec=luxifer-native
Icon=luxifer
Terminal=false
Categories=Graphics;Engineering;
StartupWMClass=luxifer
EOF

rm -f "$OUTPUT"
echo "» Erzeuge $(basename "$OUTPUT") …"
LDAI_OUTPUT="$OUTPUT" APPIMAGE_EXTRACT_AND_RUN=1 "$LINUXDEPLOY_BIN" \
  --appdir "$APPDIR" \
  --executable "$APPDIR/usr/bin/$BINARY" \
  --desktop-file "$APPDIR/usr/share/applications/luxifer.desktop" \
  --icon-file "$APPDIR/usr/share/icons/hicolor/512x512/apps/luxifer.png" \
  --output appimage

echo "» Fertig: $OUTPUT"
