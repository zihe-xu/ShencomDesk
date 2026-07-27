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
    cargoToml: '[package]\nname = "shendesk"\nversion = "1.2.3"\n\n[dependencies]\n',
    tauriConfig: { version: "1.2.3", bundle: {} },
    releaseConfig: { bundle: { createUpdaterArtifacts: true } },
    tagName: "v1.2.3",
    publicKey: "public-key",
    privateKey: "private-key",
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
    () => validateReleaseConfiguration(validInput({ privateKey: "  " })),
    /TAURI_SIGNING_PRIVATE_KEY is not configured/,
  );
});

test("keeps updater artifact generation release-only", () => {
  assert.throws(
    () =>
      validateReleaseConfiguration(
        validInput({
          tauriConfig: { version: "1.2.3", bundle: { createUpdaterArtifacts: true } },
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

test("release workflow is tag-only and receives keys without printing them", async () => {
  const workflow = await readFile(
    new URL("../workflows/release.yml", import.meta.url),
    "utf8",
  );

  assert.match(workflow, /tags:\s*\n\s*- "v\*"/);
  assert.doesNotMatch(workflow, /pull_request:/);
  assert.match(
    workflow,
    /SHENDESK_UPDATER_PUBLIC_KEY: \$\{\{ vars\.SHENDESK_UPDATER_PUBLIC_KEY \}\}/,
  );
  assert.match(
    workflow,
    /TAURI_SIGNING_PRIVATE_KEY: \$\{\{ secrets\.TAURI_SIGNING_PRIVATE_KEY \}\}/,
  );
  assert.match(
    workflow,
    /tauri-apps\/tauri-action@84b9d35b5fc46c1e45415bdb6144030364f7ebc5/,
  );
  assert.match(workflow, /tauri\.release\.conf\.json/);
  assert.match(workflow, /includeUpdaterJson: true/);
  assert.match(workflow, /max-parallel: 1/);
  assert.match(workflow, /releaseDraft: true/);
});
