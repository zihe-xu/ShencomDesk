#!/bin/bash

set -euo pipefail

if [[ "${1:-}" != "build" ]]; then
  echo "::error::Expected the Tauri build command"
  exit 1
fi

pnpm run tauri "$@"

: "${MACOS_APP_PATH:?MACOS_APP_PATH is required}"
: "${OFFICECLI_EXPECTED_ARCH:?OFFICECLI_EXPECTED_ARCH is required}"
: "${GITHUB_WORKSPACE:?GITHUB_WORKSPACE is required}"

if [[ ! -d "$MACOS_APP_PATH" ]]; then
  echo "::error::Expected macOS application bundle was not generated"
  exit 1
fi

officecli_path="$MACOS_APP_PATH/Contents/MacOS/officecli"
if [[ ! -f "$officecli_path" ]]; then
  echo "::error::Expected OfficeCLI sidecar was not bundled"
  exit 1
fi

entitlements_path="$(mktemp "${RUNNER_TEMP:-/tmp}/officecli-entitlements.XXXXXX")"

cleanup() {
  rm -f "$entitlements_path"
}
trap cleanup EXIT

codesign --verify --deep --strict --verbose=2 "$MACOS_APP_PATH"
signature_details="$(codesign -dvvv "$MACOS_APP_PATH" 2>&1)"
if [[ "$signature_details" != *"Authority=Developer ID Application:"* ]]; then
  echo "::error::Application is not signed with a Developer ID Application certificate"
  exit 1
fi

codesign --verify --strict --verbose=2 "$officecli_path"
officecli_signature_details="$(codesign -dvvv "$officecli_path" 2>&1)"
if [[ "$officecli_signature_details" != *"Authority=Developer ID Application:"* ]]; then
  echo "::error::OfficeCLI is not signed with a Developer ID Application certificate"
  exit 1
fi
codesign -d --entitlements - --xml "$officecli_path" > "$entitlements_path"
allow_jit="$(/usr/libexec/PlistBuddy -c 'Print :com.apple.security.cs.allow-jit' "$entitlements_path")"
if [[ "$allow_jit" != "true" ]]; then
  echo "::error::OfficeCLI sidecar is missing com.apple.security.cs.allow-jit"
  exit 1
fi

node "$GITHUB_WORKSPACE/.github/scripts/verify-officecli-install.mjs" \
  --root "$MACOS_APP_PATH/Contents/MacOS" \
  --resources "$MACOS_APP_PATH/Contents/Resources" \
  --expected-arch "$OFFICECLI_EXPECTED_ARCH"

xcrun stapler validate "$MACOS_APP_PATH"
spctl --assess --type execute --verbose=2 "$MACOS_APP_PATH"
