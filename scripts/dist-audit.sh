#!/usr/bin/env bash
#
# dist-audit.sh — reproducible bundled-binary audit for a built NexTwist AppImage.
#
# Extracts the AppImage and enumerates exactly what native code it ships, producing the
# evidence recorded in DIST-AUDIT.md (DIST-02):
#   * ldd of the shipped binary  -> proves rustls-only TLS, no app-path libssl/libcrypto (V6).
#   * the full usr/lib/*.so* list -> the bundled shared-library inventory.
#   * UnRAR-absence checks        -> proves NO non-free RAR code ships (.rar shells out to a
#                                    system unrar/7z instead).
#   * WebKitGTK-absence check     -> proves the AppImage uses the HOST's WebKitGTK 4.1.
#
# Run on the artifact produced by `release.yml` (tauri-action --bundles appimage):
#   ./scripts/dist-audit.sh [path-to.AppImage]
# Default artifact name matches Tauri's bundle output for version 0.1.0.
#
# Source: docs.appimage.org + standard ldd workflow (05-RESEARCH.md "Bundled-binary audit").

set -euo pipefail

APP="${1:-NexTwist_0.1.0_amd64.AppImage}"

if [[ ! -f "$APP" ]]; then
  echo "error: AppImage not found: $APP" >&2
  echo "usage: $0 [path-to.AppImage]    (build it first via 'cargo tauri build --bundles appimage' or the release.yml run)" >&2
  exit 1
fi

# Resolve to an absolute path before any extraction churn changes cwd expectations.
APP="$(realpath "$APP")"
BIN="squashfs-root/usr/bin/nextwist"

echo "== Extracting $APP =="
# Some CI/container runners lack FUSE; --appimage-extract never needs it.
"$APP" --appimage-extract >/dev/null
echo

echo "== 1. Dynamic deps of $BIN (expect rustls; NO app-path libssl/libcrypto — V6) =="
ldd "$BIN"
echo

echo "== 2. Bundled shared libraries (usr/lib) =="
find squashfs-root/usr/lib -name '*.so*' | sort
echo

echo "== 3. UnRAR / non-free RAR absence (expect NO output below — DIST-02 / T-05-04) =="
find squashfs-root \( -iname '*unrar*' -o -iname '*libunrar*' \) -print
grep -rIl --binary-files=text -e 'UnRAR' "$BIN" || echo "no UnRAR string in $BIN"
echo

echo "== 4. WebKitGTK absence (expect NO output — the AppImage uses the host's WebKitGTK 4.1) =="
find squashfs-root/usr/lib -iname '*webkit*' -print
echo

echo "== dist-audit.sh complete =="
