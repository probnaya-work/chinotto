#!/usr/bin/env bash
# Build Chinotto's explicit Mac App Store variant.
#
#   ./scripts/build-mas-testflight.sh --prepare-only
#     Builds an ad-hoc-signed, sandboxed audit archive without Apple credentials.
#
#   ./scripts/build-mas-testflight.sh
#     Embeds an App Store profile, signs the app and installer, and creates the
#     Transporter-ready .pkg. This command never uploads anything.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

MODE="package"
case "${1:-}" in
  "") ;;
  --prepare-only) MODE="prepare" ;;
  *) echo "usage: $0 [--prepare-only]" >&2; exit 2 ;;
esac

if [ -f "$ROOT/scripts/mas-testflight-env.sh" ]; then
  # shellcheck source=/dev/null
  source "$ROOT/scripts/mas-testflight-env.sh"
fi

APP_NAME="Chinotto"
CONFIG_BUNDLE_ID="$(node -e "const j=require('./src-tauri/tauri.conf.json');process.stdout.write(j.identifier)")"
BUNDLE_ID="${BUNDLE_ID:-$CONFIG_BUNDLE_ID}"
if [ "$BUNDLE_ID" != "$CONFIG_BUNDLE_ID" ]; then
  echo "BUNDLE_ID ($BUNDLE_ID) must match tauri.conf.json ($CONFIG_BUNDLE_ID)." >&2
  exit 1
fi

MARKETING_VERSION="$(node -e "process.stdout.write(require('./package.json').version)")"
DEFAULT_BUILD_VERSION="$(node -e "const j=require('./src-tauri/tauri.conf.json');process.stdout.write(j.bundle?.macOS?.bundleVersion || j.version)")"
BUILD_VERSION="${MAS_BUILD_NUMBER:-$DEFAULT_BUILD_VERSION}"
if ! [[ "$BUILD_VERSION" =~ ^[1-9][0-9]{0,3}(\.[0-9]{1,2}){0,2}$ ]]; then
  echo "MAS_BUILD_NUMBER must be a valid CFBundleVersion (for example 3, 3.0, or 3.0.1): $BUILD_VERSION" >&2
  exit 1
fi

DEFAULT_MODEL_CACHE="$HOME/Library/Application Support/app.chinotto/models"
MODEL_CACHE="${MAS_MODEL_CACHE:-$DEFAULT_MODEL_CACHE}"
MODEL_REPOSITORY="$MODEL_CACHE/models--Qdrant--all-MiniLM-L6-v2-onnx"
MODEL_BLOB="$MODEL_REPOSITORY/blobs/bbd7b466f6d58e646fdc2bd5fd67b2f5e93c0b687011bd4548c420f7bd46f0c5"
if [ ! -f "$MODEL_REPOSITORY/refs/main" ] || [ ! -f "$MODEL_BLOB" ]; then
  echo "The MAS build requires a complete Qdrant/all-MiniLM-L6-v2-onnx cache." >&2
  echo "Set MAS_MODEL_CACHE to the fastembed cache containing models--Qdrant--all-MiniLM-L6-v2-onnx." >&2
  exit 1
fi
MODEL_SHA="$(shasum -a 256 "$MODEL_BLOB" | awk '{print $1}')"
if [ "$MODEL_SHA" != "bbd7b466f6d58e646fdc2bd5fd67b2f5e93c0b687011bd4548c420f7bd46f0c5" ]; then
  echo "Bundled model checksum mismatch: $MODEL_SHA" >&2
  exit 1
fi

if [ "$MODE" = "package" ]; then
  : "${MAS_APP_SIGN_IDENTITY:?Set MAS_APP_SIGN_IDENTITY (see scripts/mas-testflight-env.example.sh)}"
  : "${MAS_INSTALLER_SIGN_IDENTITY:?Set MAS_INSTALLER_SIGN_IDENTITY (see scripts/mas-testflight-env.example.sh)}"
  : "${MAS_PROVISIONING_PROFILE:?Set MAS_PROVISIONING_PROFILE to a Mac App Store Connect profile}"
  if [ ! -f "$MAS_PROVISIONING_PROFILE" ]; then
    echo "MAS_PROVISIONING_PROFILE is not a file: $MAS_PROVISIONING_PROFILE" >&2
    exit 1
  fi
fi

SOURCE_APP="$ROOT/src-tauri/target/release/bundle/macos/${APP_NAME}.app"
OUT_DIR="$ROOT/builds/mas"
STAGED_APP="$OUT_DIR/${APP_NAME}-${MARKETING_VERSION}-${BUILD_VERSION}-mas.app"
ARCHIVE_PATH="$OUT_DIR/${APP_NAME}-${MARKETING_VERSION}-${BUILD_VERSION}-mas.zip"
PKG_PATH="$OUT_DIR/${APP_NAME}-${MARKETING_VERSION}-${BUILD_VERSION}-mas.pkg"
ENTITLEMENTS="$ROOT/src-tauri/Chinotto.mas.entitlements"
BUILD_CONFIG="$(mktemp -t chinotto-mas-config).json"
MERGED_ENT="$(mktemp -t chinotto-mas-entitlements).plist"
trap 'rm -f "$BUILD_CONFIG" "$MERGED_ENT"' EXIT

node -e '
  const fs = require("fs");
  fs.writeFileSync(process.argv[1], JSON.stringify({ bundle: { macOS: { bundleVersion: process.argv[2] } } }));
' "$BUILD_CONFIG" "$BUILD_VERSION"

mkdir -p "$OUT_DIR"

echo "==> Build MAS variant (no private macOS API, no updater artifacts)"
(
  cd "$ROOT/src-tauri"
  env -u CARGO_TARGET_DIR \
    VITE_DISTRIBUTION_CHANNEL=mas \
    CI=false \
    npx tauri build \
      --bundles app \
      --features mas \
      --config tauri.mas-build.json \
      --config "$BUILD_CONFIG" \
      -- \
      --no-default-features
)

if [ ! -d "$SOURCE_APP" ]; then
  echo "Expected app not found: $SOURCE_APP" >&2
  exit 1
fi

if [ -e "$STAGED_APP" ]; then
  rm -rf "$STAGED_APP"
fi
/usr/bin/ditto "$SOURCE_APP" "$STAGED_APP"

echo "==> Embed the fixed, licensed meaning model as app data"
MODEL_DEST="$STAGED_APP/Contents/Resources/models"
mkdir -p "$MODEL_DEST"
/usr/bin/ditto "$MODEL_CACHE" "$MODEL_DEST"
find "$MODEL_DEST" -type f -name '*.lock' -delete

echo "==> Strip extended attributes"
xattr -cr "$STAGED_APP"

if [ "$MODE" = "package" ]; then
  echo "==> Embed and validate Mac App Store provisioning profile"
  if cp -X "$MAS_PROVISIONING_PROFILE" "$STAGED_APP/Contents/embedded.provisionprofile" 2>/dev/null; then
    :
  else
    cp "$MAS_PROVISIONING_PROFILE" "$STAGED_APP/Contents/embedded.provisionprofile"
  fi
  export BUNDLE_ID
  python3 "$ROOT/scripts/mas-merge-signing-entitlements.py" \
    "$ENTITLEMENTS" \
    "$MAS_PROVISIONING_PROFILE" \
    "$MERGED_ENT"

  echo "==> Sign app for Mac App Store distribution"
  codesign --force --options runtime --timestamp \
    --sign "$MAS_APP_SIGN_IDENTITY" \
    --entitlements "$MERGED_ENT" \
    "$STAGED_APP"
else
  echo "==> Ad-hoc sign sandboxed app for local archive inspection"
  codesign --force --options runtime \
    --sign - \
    --entitlements "$ENTITLEMENTS" \
    "$STAGED_APP"
fi

echo "==> Audit app bundle"
MAS_EXPECTED_BUILD_VERSION="$BUILD_VERSION" \
  "$ROOT/scripts/audit-mas-bundle.sh" "$STAGED_APP"

rm -f "$ARCHIVE_PATH"
/usr/bin/ditto -c -k --keepParent "$STAGED_APP" "$ARCHIVE_PATH"

if [ "$MODE" = "package" ]; then
  echo "==> Build signed installer package"
  rm -f "$PKG_PATH"
  productbuild --component "$STAGED_APP" /Applications \
    --identifier "${BUNDLE_ID}.pkg" \
    --version "$BUILD_VERSION" \
    --sign "$MAS_INSTALLER_SIGN_IDENTITY" \
    "$PKG_PATH"
  xattr -cr "$PKG_PATH" 2>/dev/null || true
  MAS_EXPECTED_BUILD_VERSION="$BUILD_VERSION" \
    "$ROOT/scripts/audit-mas-bundle.sh" "$STAGED_APP" "$PKG_PATH"
  echo "Done. Transporter-ready package (not uploaded):"
  echo "  $PKG_PATH"
else
  echo "Done. Credential-free MAS audit archive:"
  echo "  $ARCHIVE_PATH"
  echo "A Transporter-ready package still requires Apple Distribution, installer, and profile credentials."
fi
