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
smoke_directory="$(mktemp -d "${RUNNER_TEMP:-/tmp}/officecli-smoke.XXXXXX")"
smoke_document="$smoke_directory/signed-release-smoke.docx"
resident_open=false

cleanup() {
  if [[ "$resident_open" == "true" ]]; then
    OFFICECLI_SKIP_UPDATE=1 "$officecli_path" close "$smoke_document" >/dev/null 2>&1 || true
  fi
  rm -f "$entitlements_path"
  rm -rf "$smoke_directory"
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

officecli_archs="$(lipo -archs "$officecli_path")"
if [[ "$officecli_archs" != "$OFFICECLI_EXPECTED_ARCH" ]]; then
  echo "::error::OfficeCLI architecture is $officecli_archs; expected $OFFICECLI_EXPECTED_ARCH"
  exit 1
fi

expected_version="$(node -e 'const manifest = JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8")); process.stdout.write(manifest.tag.replace(/^v/, ""));' "$GITHUB_WORKSPACE/third_party/officecli/version.json")"
version_output="$(OFFICECLI_SKIP_UPDATE=1 "$officecli_path" --version)"
if [[ "$version_output" != *"$expected_version"* ]]; then
  echo "::error::OfficeCLI version output does not contain pinned version $expected_version"
  exit 1
fi

smoke_marker="ShenDesk signed OfficeCLI smoke test"
OFFICECLI_SKIP_UPDATE=1 "$officecli_path" create "$smoke_document"
OFFICECLI_SKIP_UPDATE=1 "$officecli_path" open "$smoke_document"
resident_open=true
OFFICECLI_SKIP_UPDATE=1 "$officecli_path" add "$smoke_document" /body --type paragraph --prop "text=$smoke_marker"
read_output="$(OFFICECLI_SKIP_UPDATE=1 "$officecli_path" get "$smoke_document" '/body/p[last()]' --json)"
if [[ "$read_output" != *"$smoke_marker"* ]]; then
  echo "::error::OfficeCLI could not read back the smoke-test content"
  exit 1
fi
OFFICECLI_SKIP_UPDATE=1 "$officecli_path" close "$smoke_document"
resident_open=false
if [[ ! -s "$smoke_document" ]]; then
  echo "::error::OfficeCLI did not persist the smoke-test document"
  exit 1
fi

xcrun stapler validate "$MACOS_APP_PATH"
spctl --assess --type execute --verbose=2 "$MACOS_APP_PATH"
