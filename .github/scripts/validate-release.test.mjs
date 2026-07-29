import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  parseCargoPackageVersion,
  validateReleaseConfiguration,
} from "./validate-release.mjs";

function validInput(overrides = {}) {
  return {
    rootPackage: { version: "1.2.3" },
    desktopPackage: { version: "1.2.3" },
    cargoToml:
      '[package]\nname = "shendesk"\nversion = "1.2.3"\n\n[dependencies]\n',
    tauriConfig: { version: "1.2.3", bundle: {} },
    releaseConfig: { bundle: { createUpdaterArtifacts: true } },
    tagName: "v1.2.3",
    publicKey: "public-key",
    privateKeyConfigured: true,
    appleCertificateConfigured: true,
    appleCertificatePasswordConfigured: true,
    appleSigningIdentityConfigured: true,
    appleIdConfigured: true,
    applePasswordConfigured: true,
    appleTeamIdConfigured: true,
    ...overrides,
  };
}

test("extracts the package version without reading dependency versions", () => {
  assert.equal(
    parseCargoPackageVersion(
      '[package]\nname = "shendesk"\nversion = "2.3.4"\n\n[dependencies]\nserde = "1"\n',
    ),
    "2.3.4",
  );
});

test("accepts a matching signed release configuration", () => {
  assert.deepEqual(validateReleaseConfiguration(validInput()), {
    version: "1.2.3",
    tagName: "v1.2.3",
    updaterArtifacts: true,
    signingMaterialConfigured: true,
    appleCodeSigningConfigured: true,
    appleNotarizationConfigured: true,
  });
});

test("rejects version drift before creating a release", () => {
  assert.throws(
    () =>
      validateReleaseConfiguration(
        validInput({ desktopPackage: { version: "1.2.4" } }),
      ),
    /release versions do not match/,
  );
});

test("rejects a tag that does not exactly match the application version", () => {
  assert.throws(
    () => validateReleaseConfiguration(validInput({ tagName: "v1.2.4" })),
    /release tag must be exactly v1\.2\.3/,
  );
});

test("fails closed when signing material is missing", () => {
  assert.throws(
    () => validateReleaseConfiguration(validInput({ publicKey: "" })),
    /SHENDESK_UPDATER_PUBLIC_KEY is not configured/,
  );
  assert.throws(
    () =>
      validateReleaseConfiguration(
        validInput({ privateKeyConfigured: false }),
      ),
    /TAURI_SIGNING_PRIVATE_KEY is not configured/,
  );

  for (const [field, environmentVariable] of [
    ["appleCertificateConfigured", "APPLE_CERTIFICATE"],
    ["appleCertificatePasswordConfigured", "APPLE_CERTIFICATE_PASSWORD"],
    ["appleSigningIdentityConfigured", "APPLE_SIGNING_IDENTITY"],
    ["appleIdConfigured", "APPLE_ID"],
    ["applePasswordConfigured", "APPLE_PASSWORD"],
    ["appleTeamIdConfigured", "APPLE_TEAM_ID"],
  ]) {
    assert.throws(
      () =>
        validateReleaseConfiguration(validInput({ [field]: false })),
      new RegExp(`${environmentVariable} is not configured`),
    );
  }
});

test("keeps updater artifact generation release-only", () => {
  assert.throws(
    () =>
      validateReleaseConfiguration(
        validInput({
          tauriConfig: {
            version: "1.2.3",
            bundle: { createUpdaterArtifacts: true },
          },
        }),
      ),
    /base Tauri config must not require updater artifacts/,
  );
  assert.throws(
    () =>
      validateReleaseConfiguration(
        validInput({ releaseConfig: { bundle: {} } }),
      ),
    /release Tauri config must enable/,
  );
});

test("release workflow is tag-only and minimizes signing-key exposure", async () => {
  const workflow = await readFile(
    new URL("../workflows/release.yml", import.meta.url),
    "utf8",
  );

  assert.match(workflow, /tags:\s*\n\s*- "v\*"/);
  assert.doesNotMatch(workflow, /pull_request:/);
  assert.match(workflow, /icon app-icons\/logo-macos\.svg/);
  assert.match(workflow, /icon app-icons\/logo-windows\.svg/);
  assert.match(workflow, /permissions:\s*\n\s*contents: read/);
  assert.match(
    workflow,
    /release:[\s\S]*?permissions:\s*\n\s*contents: write/,
  );
  assert.match(
    workflow,
    /SHENDESK_UPDATER_PUBLIC_KEY: \$\{\{ vars\.SHENDESK_UPDATER_PUBLIC_KEY \}\}/,
  );
  assert.match(
    workflow,
    /TAURI_SIGNING_PRIVATE_KEY_CONFIGURED: \$\{\{ secrets\.TAURI_SIGNING_PRIVATE_KEY != '' \}\}/,
  );
  for (const configuredVariable of [
    "APPLE_CERTIFICATE_CONFIGURED",
    "APPLE_CERTIFICATE_PASSWORD_CONFIGURED",
    "APPLE_SIGNING_IDENTITY_CONFIGURED",
    "APPLE_ID_CONFIGURED",
    "APPLE_PASSWORD_CONFIGURED",
    "APPLE_TEAM_ID_CONFIGURED",
  ]) {
    assert.match(workflow, new RegExp(`${configuredVariable}: \\$\\{\\{`));
  }
  assert.equal(
    [...workflow.matchAll(/^\s*TAURI_SIGNING_PRIVATE_KEY:\s/gm)].length,
    2,
  );
  assert.match(
    workflow,
    /tauri-apps\/tauri-action@84b9d35b5fc46c1e45415bdb6144030364f7ebc5/,
  );
  assert.match(workflow, /tauri\.release\.conf\.json/);
  assert.match(workflow, /includeUpdaterJson: true/);
  assert.match(workflow, /max-parallel: 1/);
  assert.match(
    workflow,
    /runner: macos-26\s*\n\s*target: aarch64-apple-darwin/,
  );
  assert.match(
    workflow,
    /runner: macos-26-intel\s*\n\s*target: x86_64-apple-darwin/,
  );
  assert.match(
    workflow,
    /target: aarch64-apple-darwin[\s\S]*?args: --target aarch64-apple-darwin --bundles app,dmg/,
  );
  assert.match(
    workflow,
    /target: x86_64-apple-darwin[\s\S]*?args: --target x86_64-apple-darwin --bundles app,dmg/,
  );
  assert.doesNotMatch(
    workflow,
    /target: (?:aarch64|x86_64)-apple-darwin --bundles dmg(?:\s|$)/,
  );
  const macosBuildStep = workflow.match(
    /- name: Build, sign, notarize, and upload macOS release assets[\s\S]*?(?=\n\s+- name: Verify macOS)/,
  )?.[0];
  const windowsBuildStep = workflow.match(
    /- name: Build, sign, and upload Windows updater release assets[\s\S]*$/,
  )?.[0];
  assert.ok(macosBuildStep);
  assert.ok(windowsBuildStep);
  for (const environmentVariable of [
    "APPLE_CERTIFICATE",
    "APPLE_CERTIFICATE_PASSWORD",
    "APPLE_SIGNING_IDENTITY",
    "APPLE_ID",
    "APPLE_PASSWORD",
    "APPLE_TEAM_ID",
  ]) {
    assert.match(macosBuildStep, new RegExp(`${environmentVariable}: \\$\\{\\{`));
    assert.doesNotMatch(windowsBuildStep, new RegExp(environmentVariable));
  }
  assert.match(workflow, /codesign --verify --deep --strict/);
  assert.match(
    workflow,
    /Authority=Developer ID Application:/,
  );
  assert.match(workflow, /xcrun stapler validate/);
  assert.match(workflow, /spctl --assess --type execute/);
  assert.match(workflow, /releaseDraft: true/);
});

test("runtime updater boundary remains signed, HTTPS-only, and least-privilege", async () => {
  const [capabilityText, baseConfigText, releaseConfigText, updaterSource, buildScript] =
    await Promise.all([
      readFile(
        new URL(
          "../../apps/desktop/src-tauri/capabilities/default.json",
          import.meta.url,
        ),
        "utf8",
      ),
      readFile(
        new URL(
          "../../apps/desktop/src-tauri/tauri.conf.json",
          import.meta.url,
        ),
        "utf8",
      ),
      readFile(
        new URL(
          "../../apps/desktop/src-tauri/tauri.release.conf.json",
          import.meta.url,
        ),
        "utf8",
      ),
      readFile(
        new URL(
          "../../apps/desktop/src-tauri/src/infrastructure/updater/mod.rs",
          import.meta.url,
        ),
        "utf8",
      ),
      readFile(
        new URL("../../apps/desktop/src-tauri/build.rs", import.meta.url),
        "utf8",
      ),
    ]);

  const capability = JSON.parse(capabilityText);
  const baseConfig = JSON.parse(baseConfigText);
  const releaseConfig = JSON.parse(releaseConfigText);

  assert.ok(capability.permissions.includes("allow-check-for-updates"));
  assert.ok(capability.permissions.includes("allow-install-update"));
  assert.equal(
    capability.permissions.some((permission) => permission.startsWith("updater:")),
    false,
  );
  assert.equal(baseConfig.bundle.createUpdaterArtifacts, undefined);
  assert.deepEqual(baseConfig.plugins.updater, { pubkey: "" });
  assert.equal(releaseConfig.bundle.createUpdaterArtifacts, true);
  assert.match(
    updaterSource,
    /https:\/\/github\.com\/zihe-xu\/ShencomDesk\/releases\/latest\/download\/latest\.json/,
  );
  assert.match(
    updaterSource,
    /option_env!\("SHENDESK_UPDATER_PUBLIC_KEY"\)/,
  );
  assert.doesNotMatch(updaterSource, /dangerous_(?:insecure|accept)/);
  assert.match(
    buildScript,
    /cargo:rerun-if-env-changed=SHENDESK_UPDATER_PUBLIC_KEY/,
  );
});
