#!/usr/bin/env bash
# Build MarkdownDocs from this checkout and install it for the current user, no root needed:
#
#   $PREFIX/bin/markdowndocs, $PREFIX/bin/markdowndocs-cli
#   $PREFIX/share/{applications,mime/packages,icons/hicolor}   launcher entry, file types, icons
#
# Usage: packaging/linux/install-local.sh [--skip-build] [--uninstall]
#
# PREFIX defaults to ~/.local (its bin/ must be on PATH). Rerun after every change to the code.
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/linux"
APP_ID=io.github.michaelmuclaude.markdowndocs
PREFIX="${PREFIX:-$HOME/.local}"

SKIP_BUILD=0
UNINSTALL=0
while [ $# -gt 0 ]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    --uninstall) UNINSTALL=1; shift ;;
    -h | --help) sed -n '2,9p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

refresh_caches() {
  update-mime-database "$PREFIX/share/mime" >/dev/null 2>&1 || true
  update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
  gtk-update-icon-cache -q "$PREFIX/share/icons/hicolor" 2>/dev/null || true
}

if [ "$UNINSTALL" = 1 ]; then
  rm -f "$PREFIX/bin/markdowndocs" "$PREFIX/bin/markdowndocs-cli" \
    "$PREFIX/share/applications/$APP_ID.desktop" "$PREFIX/share/mime/packages/$APP_ID.xml"
  rm -f "$PREFIX"/share/icons/hicolor/*/apps/"$APP_ID".*
  refresh_caches
  echo "==> removed MarkdownDocs from $PREFIX"
  exit 0
fi

if [ "$SKIP_BUILD" = 0 ]; then
  (cd "$ROOT" && cargo build --release --locked -p wordcraft -p wordcraft-cli)
fi
BIN="$CARGO_TARGET_DIR/release"

install -Dm755 "$BIN/wordcraft" "$PREFIX/bin/markdowndocs"
install -Dm755 "$BIN/wordcraft-cli" "$PREFIX/bin/markdowndocs-cli"
strip "$PREFIX/bin/markdowndocs" "$PREFIX/bin/markdowndocs-cli" 2>/dev/null || true
install -Dm644 "$HERE/$APP_ID.desktop" "$PREFIX/share/applications/$APP_ID.desktop"
install -Dm644 "$HERE/$APP_ID.mime.xml" "$PREFIX/share/mime/packages/$APP_ID.xml"
for icon in "$ROOT"/assets/app-icon/hicolor/*/apps/"$APP_ID".*; do
  install -Dm644 "$icon" "$PREFIX/share/icons/hicolor/${icon#"$ROOT/assets/app-icon/hicolor/"}"
done
refresh_caches

echo "==> installed MarkdownDocs $VERSION into $PREFIX"
