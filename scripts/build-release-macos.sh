#!/bin/bash
# Build a signed + notarized macOS release bundle.
# Requires env vars: APPLE_SIGNING_IDENTITY, APPLE_API_KEY, APPLE_API_ISSUER, APPLE_API_KEY_PATH
# See docs/release-macos.md for setup.

set -e

MISSING=""
for var in APPLE_SIGNING_IDENTITY APPLE_API_KEY APPLE_API_ISSUER APPLE_API_KEY_PATH; do
  if [ -z "${!var}" ]; then
    MISSING="${MISSING} ${var}"
  fi
done

if [ -n "$MISSING" ]; then
  echo "Missing environment variables:$MISSING"
  echo "See docs/release-macos.md for how to set them."
  exit 1
fi

if [ -n "$APPLE_API_KEY_PATH" ] && [ ! -f "$APPLE_API_KEY_PATH" ]; then
  echo "APPLE_API_KEY_PATH is set but file not found: $APPLE_API_KEY_PATH"
  exit 1
fi

npm run build && CI=false ./scripts/tauri-direct.sh build
