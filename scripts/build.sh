#!/usr/bin/env bash
# HAAL Installer — cross-platform build helper
# Usage: ./scripts/build.sh --target <windows|macos|linux|all>
set -euo pipefail

TARGET="all"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) TARGET="$2"; shift 2 ;;
    -h|--help)
      echo "Usage: $0 --target <windows|macos|linux|all>"
      echo ""
      echo "Targets:"
      echo "  windows  - Build .msi (WiX) and .nsis installers"
      echo "  macos    - Build universal .dmg (Intel + Apple Silicon)"
      echo "  linux    - Build .deb and .AppImage"
      echo "  all      - Build for the current host platform (default)"
      exit 0
      ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

cd "$(dirname "$0")/.."

echo "==> Installing frontend dependencies"
npm ci

build_windows() {
  echo "==> Building Windows targets (.msi, .nsis)"
  npm run tauri build -- --bundles msi,nsis
}

build_macos() {
  echo "==> Building macOS universal binary (.dmg)"
  npm run tauri build -- --target universal-apple-darwin --bundles dmg
}

build_linux() {
  echo "==> Building Linux targets (.deb, .appimage)"
  npm run tauri build -- --bundles deb,appimage
}

case "$TARGET" in
  windows) build_windows ;;
  macos)   build_macos   ;;
  linux)   build_linux   ;;
  all)     npm run tauri build ;;
  *)       echo "Invalid target: $TARGET"; exit 1 ;;
esac

echo "==> Build complete. Artifacts are in src-tauri/target/release/bundle/"
