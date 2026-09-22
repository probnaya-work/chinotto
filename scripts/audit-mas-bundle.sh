#!/usr/bin/env bash
# Static release gate for a prepared Mac App Store app and optional installer package.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
APP_PATH="${1:?usage: $0 /path/to/Chinotto.app [/path/to/Chinotto.pkg]}"
PKG_PATH="${2:-}"
EXPECTED_ID="$(node -e "process.stdout.write(require('./src-tauri/tauri.conf.json').identifier)")"
EXPECTED_MARKETING="$(node -e "process.stdout.write(require('./package.json').version)")"
EXPECTED_BUILD="${MAS_EXPECTED_BUILD_VERSION:-$(node -e "const j=require('./src-tauri/tauri.conf.json');process.stdout.write(j.bundle?.macOS?.bundleVersion || j.version)")}"

fail() { echo "MAS audit failed: $*" >&2; exit 1; }
require_file() { [ -f "$1" ] || fail "missing $1"; }
plist_raw() { plutil -extract "$2" raw -o - "$1" 2>/dev/null; }
plist_literal_key_raw() { /usr/libexec/PlistBuddy -c "Print :$2" "$1" 2>/dev/null; }
expect_plist() {
  local file="$1" key="$2" expected="$3" actual
  actual="$(plist_raw "$file" "$key" || true)"
  [ "$actual" = "$expected" ] || fail "$key is '$actual', expected '$expected'"
}

[ -d "$APP_PATH" ] || fail "app bundle not found: $APP_PATH"
INFO="$APP_PATH/Contents/Info.plist"
EXECUTABLE="$APP_PATH/Contents/MacOS/Chinotto"
PRIVACY="$APP_PATH/Contents/Resources/PrivacyInfo.xcprivacy"
NOTICE="$APP_PATH/Contents/Resources/resources/THIRD-PARTY-NOTICES.txt"
MODEL="$APP_PATH/Contents/Resources/models/models--Qdrant--all-MiniLM-L6-v2-onnx/blobs/bbd7b466f6d58e646fdc2bd5fd67b2f5e93c0b687011bd4548c420f7bd46f0c5"

require_file "$INFO"
require_file "$EXECUTABLE"
require_file "$PRIVACY"
require_file "$NOTICE"
require_file "$MODEL"
/usr/bin/grep -q "END OF TERMS AND CONDITIONS" "$NOTICE" \
  || fail "bundled model's Apache-2.0 license text is incomplete"
plutil -lint "$INFO" "$PRIVACY" >/dev/null
expect_plist "$INFO" CFBundleIdentifier "$EXPECTED_ID"
expect_plist "$INFO" CFBundleShortVersionString "$EXPECTED_MARKETING"
expect_plist "$INFO" CFBundleVersion "$EXPECTED_BUILD"
expect_plist "$INFO" LSApplicationCategoryType "public.app-category.productivity"
expect_plist "$INFO" LSMinimumSystemVersion "12.0"
expect_plist "$INFO" ITSAppUsesNonExemptEncryption "false"
expect_plist "$PRIVACY" NSPrivacyTracking "false"

ARCHS="$(lipo -archs "$EXECUTABLE")"
case " $ARCHS " in
  *" arm64 "*) ;;
  *) fail "main executable does not contain arm64: $ARCHS" ;;
esac

MODEL_SHA="$(shasum -a 256 "$MODEL" | awk '{print $1}')"
[ "$MODEL_SHA" = "bbd7b466f6d58e646fdc2bd5fd67b2f5e93c0b687011bd4548c420f7bd46f0c5" ] \
  || fail "bundled meaning model checksum mismatch"

if strings "$EXECUTABLE" | /usr/bin/grep -Eq 'drawsBackground|fullScreenEnabled'; then
  fail "private WKWebView API marker is present"
fi
if strings "$EXECUTABLE" | /usr/bin/grep -q 'chinotto-oauth-bridge'; then
  fail "browser-loopback OAuth server marker is present"
fi

codesign --verify --strict --verbose=2 "$APP_PATH"
ENT_CHECK="$(mktemp -t chinotto-mas-audit-entitlements).plist"
trap 'rm -f "$ENT_CHECK"' EXIT
# `codesign` 27 writes a diagnostic tree when passed a filesystem output path.
# Its stdout plist form remains machine-readable across the supported toolchains.
codesign -d --entitlements :- "$APP_PATH" >"$ENT_CHECK" 2>/dev/null
[ "$(plist_literal_key_raw "$ENT_CHECK" com.apple.security.app-sandbox || true)" = "true" ] \
  || fail "App Sandbox entitlement is missing"
[ "$(plist_literal_key_raw "$ENT_CHECK" com.apple.security.network.client || true)" = "true" ] \
  || fail "outgoing-network entitlement is missing"
[ "$(plist_literal_key_raw "$ENT_CHECK" com.apple.security.files.user-selected.read-write || true)" = "true" ] \
  || fail "user-selected read/write entitlement is missing"
[ "$(plist_literal_key_raw "$ENT_CHECK" com.apple.security.device.audio-input || true)" = "true" ] \
  || fail "audio-input entitlement is missing"
[ "$(plist_literal_key_raw "$ENT_CHECK" com.apple.security.device.microphone || true)" = "true" ] \
  || fail "microphone entitlement is missing"
if plist_literal_key_raw "$ENT_CHECK" com.apple.security.network.server >/dev/null 2>&1; then
  fail "incoming-network entitlement must not be present"
fi
if [ "$(plist_literal_key_raw "$ENT_CHECK" get-task-allow || true)" = "true" ]; then
  fail "get-task-allow must not be enabled"
fi

ICON="$APP_PATH/Contents/Resources/icon.icns"
require_file "$ICON"
[ "$(sips -g pixelWidth "$ICON" 2>/dev/null | awk '/pixelWidth/{print $2}')" = "1024" ] \
  || fail "icon.icns lacks a 1024px representation"

if [ -n "$PKG_PATH" ]; then
  require_file "$PKG_PATH"
  pkgutil --check-signature "$PKG_PATH"
fi

echo "MAS audit passed: $APP_PATH"
