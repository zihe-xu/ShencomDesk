import assert from "node:assert/strict";
import { mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  assertTagCommit,
  resolveTarget,
  validateManifest,
  verifyArchiveHash,
} from "./build-officecli.mjs";

function validManifest(overrides = {}) {
  const commit = "fd4adab4dbe3283b62e3edcfad124dd648fa74bc";
  return {
    repository: "https://github.com/iOfficeAI/OfficeCLI",
    tag: "v1.0.143",
    commit,
    sourceArchive: {
      url: `https://github.com/iOfficeAI/OfficeCLI/archive/${commit}.tar.gz`,
      sha256: "1".repeat(64),
    },
    dotnetSdk: "10.0.302",
    project: "src/officecli/officecli.csproj",
    nugetLockDestination: "src/officecli/packages.lock.json",
    nugetLocks: {
      "aarch64-apple-darwin": "packages.osx-arm64.lock.json",
      "x86_64-apple-darwin": "packages.osx-x64.lock.json",
      "x86_64-pc-windows-msvc": "packages.win-x64.lock.json",
    },
    license: "Apache-2.0",
    licenseFiles: ["LICENSE", "NOTICE", "THIRD-PARTY-NOTICES.txt"],
    ...overrides,
  };
}

test("maps supported Rust targets to .NET RIDs", () => {
  assert.equal(resolveTarget("aarch64-apple-darwin").rid, "osx-arm64");
  assert.equal(resolveTarget("x86_64-apple-darwin").rid, "osx-x64");
  assert.equal(resolveTarget("x86_64-pc-windows-msvc").rid, "win-x64");
});

test("rejects an unknown Rust target", () => {
  assert.throws(() => resolveTarget("aarch64-pc-windows-msvc"), /Unsupported Rust target/);
});

test("rejects missing manifest fields", () => {
  const manifest = validManifest();
  delete manifest.dotnetSdk;
  assert.throws(() => validateManifest(manifest), /missing required field: dotnetSdk/);
});

test("rejects placeholder hashes and archive URLs not pinned to the commit", () => {
  assert.throws(
    () =>
      validateManifest(
        validManifest({ sourceArchive: { url: "https://example.com/latest.tar.gz", sha256: "0".repeat(64) } }),
      ),
    /full pinned commit/,
  );
  assert.throws(
    () =>
      validateManifest(
        validManifest({
          sourceArchive: {
            url: "https://github.com/iOfficeAI/OfficeCLI/archive/fd4adab4dbe3283b62e3edcfad124dd648fa74bc.tar.gz",
            sha256: "0".repeat(64),
          },
        }),
      ),
    /placeholder/,
  );
});

test("rejects a tag that does not resolve to the pinned commit", () => {
  assert.throws(
    () => assertTagCommit("v1.0.143", "a".repeat(40), `${"b".repeat(40)}\trefs/tags/v1.0.143\n`),
    /tag\/commit mismatch/,
  );
});

test("rejects a source archive with the wrong hash", async () => {
  const directory = await mkdtemp(join(tmpdir(), "officecli-hash-test-"));
  const archive = join(directory, "source.tar.gz");
  await writeFile(archive, "tampered archive");
  await assert.rejects(() => verifyArchiveHash(archive, "1".repeat(64)), /SHA-256 mismatch/);
});
