import { appendFile, readFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

const SEMVER_PATTERN =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;

export function parseCargoPackageVersion(cargoToml) {
  if (typeof cargoToml !== "string") {
    throw new TypeError("Cargo.toml content must be a string");
  }

  let inPackageSection = false;
  for (const line of cargoToml.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (trimmed.startsWith("[") && trimmed.endsWith("]")) {
      inPackageSection = trimmed === "[package]";
      continue;
    }

    if (inPackageSection) {
      const versionMatch = line.match(/^version\s*=\s*"([^"]+)"\s*$/);
      if (versionMatch) {
        return versionMatch[1];
      }
    }
  }

  throw new Error("Cargo.toml [package] is missing a version");
}

export function validateReleaseConfiguration({
  rootPackage,
  desktopPackage,
  cargoToml,
  tauriConfig,
  releaseConfig,
  tagName,
  publicKey,
  privateKeyConfigured,
  appleCertificateConfigured,
  appleCertificatePasswordConfigured,
  appleSigningIdentityConfigured,
  appleIdConfigured,
  applePasswordConfigured,
  appleTeamIdConfigured,
}) {
  const versions = {
    rootPackage: rootPackage?.version,
    desktopPackage: desktopPackage?.version,
    cargoPackage: parseCargoPackageVersion(cargoToml),
    tauriConfig: tauriConfig?.version,
  };

  for (const [source, version] of Object.entries(versions)) {
    if (typeof version !== "string" || !SEMVER_PATTERN.test(version)) {
      throw new Error(`${source} has an invalid semantic version`);
    }
  }

  const uniqueVersions = new Set(Object.values(versions));
  if (uniqueVersions.size !== 1) {
    throw new Error(
      `release versions do not match: ${Object.entries(versions)
        .map(([source, version]) => `${source}=${version}`)
        .join(", ")}`,
    );
  }

  const [version] = uniqueVersions;
  const expectedTag = `v${version}`;
  if (tagName !== expectedTag) {
    throw new Error(`release tag must be exactly ${expectedTag}`);
  }

  if (tauriConfig?.bundle?.createUpdaterArtifacts === true) {
    throw new Error(
      "base Tauri config must not require updater artifacts for ordinary builds",
    );
  }
  if (releaseConfig?.bundle?.createUpdaterArtifacts !== true) {
    throw new Error(
      "release Tauri config must enable bundle.createUpdaterArtifacts",
    );
  }

  if (typeof publicKey !== "string" || publicKey.trim().length === 0) {
    throw new Error("SHENDESK_UPDATER_PUBLIC_KEY is not configured");
  }
  if (privateKeyConfigured !== true) {
    throw new Error("TAURI_SIGNING_PRIVATE_KEY is not configured");
  }

  const appleSigningRequirements = {
    APPLE_CERTIFICATE: appleCertificateConfigured,
    APPLE_CERTIFICATE_PASSWORD: appleCertificatePasswordConfigured,
    APPLE_SIGNING_IDENTITY: appleSigningIdentityConfigured,
    APPLE_ID: appleIdConfigured,
    APPLE_PASSWORD: applePasswordConfigured,
    APPLE_TEAM_ID: appleTeamIdConfigured,
  };
  for (const [name, configured] of Object.entries(
    appleSigningRequirements,
  )) {
    if (configured !== true) {
      throw new Error(`${name} is not configured`);
    }
  }

  return {
    version,
    tagName,
    updaterArtifacts: true,
    signingMaterialConfigured: true,
    appleCodeSigningConfigured: true,
    appleNotarizationConfigured: true,
  };
}

async function loadJson(filePath) {
  return JSON.parse(await readFile(filePath, "utf8"));
}

async function runCli() {
  const repositoryRoot = process.cwd();
  const [
    rootPackage,
    desktopPackage,
    cargoToml,
    tauriConfig,
    releaseConfig,
  ] = await Promise.all([
    loadJson(path.join(repositoryRoot, "package.json")),
    loadJson(path.join(repositoryRoot, "apps/desktop/package.json")),
    readFile(
      path.join(repositoryRoot, "apps/desktop/src-tauri/Cargo.toml"),
      "utf8",
    ),
    loadJson(
      path.join(repositoryRoot, "apps/desktop/src-tauri/tauri.conf.json"),
    ),
    loadJson(
      path.join(
        repositoryRoot,
        "apps/desktop/src-tauri/tauri.release.conf.json",
      ),
    ),
  ]);

  const result = validateReleaseConfiguration({
    rootPackage,
    desktopPackage,
    cargoToml,
    tauriConfig,
    releaseConfig,
    tagName: process.env.GITHUB_REF_NAME ?? "",
    publicKey: process.env.SHENDESK_UPDATER_PUBLIC_KEY ?? "",
    privateKeyConfigured:
      process.env.TAURI_SIGNING_PRIVATE_KEY_CONFIGURED === "true",
    appleCertificateConfigured:
      process.env.APPLE_CERTIFICATE_CONFIGURED === "true",
    appleCertificatePasswordConfigured:
      process.env.APPLE_CERTIFICATE_PASSWORD_CONFIGURED === "true",
    appleSigningIdentityConfigured:
      process.env.APPLE_SIGNING_IDENTITY_CONFIGURED === "true",
    appleIdConfigured: process.env.APPLE_ID_CONFIGURED === "true",
    applePasswordConfigured:
      process.env.APPLE_PASSWORD_CONFIGURED === "true",
    appleTeamIdConfigured:
      process.env.APPLE_TEAM_ID_CONFIGURED === "true",
  });

  if (process.env.GITHUB_OUTPUT) {
    await appendFile(
      process.env.GITHUB_OUTPUT,
      `version=${result.version}\n`,
      "utf8",
    );
  }
  if (process.env.GITHUB_STEP_SUMMARY) {
    await appendFile(
      process.env.GITHUB_STEP_SUMMARY,
      [
        "## Signed release preflight",
        "",
        `- Version: \`${result.version}\``,
        `- Tag: \`${result.tagName}\``,
        "- Updater artifacts: enabled by release-only config",
        "- Updater signing material: configured",
        "- Apple Developer ID signing material: configured",
        "- Apple notarization material: configured",
        "- Signing secret contents: not exposed to preflight",
        "",
      ].join("\n"),
      "utf8",
    );
  }

  console.log(`release preflight passed for ${result.tagName}`);
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href
) {
  runCli().catch((error) => {
    console.error(`release preflight failed: ${error.message}`);
    process.exitCode = 1;
  });
}
