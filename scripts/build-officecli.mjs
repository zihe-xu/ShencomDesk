import { createHash } from "node:crypto";
import { createReadStream, createWriteStream } from "node:fs";
import {
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  stat,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { pipeline } from "node:stream/promises";
import { spawn } from "node:child_process";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(scriptDirectory, "..");
const manifestPath = join(repositoryRoot, "third_party/officecli/version.json");
const vendorDirectory = dirname(manifestPath);
const outputDirectory = join(
  repositoryRoot,
  "apps/desktop/src-tauri/binaries",
);

export const TARGETS = Object.freeze({
  "aarch64-apple-darwin": { rid: "osx-arm64", extension: "" },
  "x86_64-apple-darwin": { rid: "osx-x64", extension: "" },
  "x86_64-pc-windows-msvc": { rid: "win-x64", extension: ".exe" },
});

const REQUIRED_FIELDS = [
  "repository",
  "tag",
  "commit",
  "sourceArchive",
  "dotnetSdk",
  "project",
  "nugetLockDestination",
  "nugetLocks",
  "license",
  "licenseFiles",
];

export function validateManifest(manifest) {
  for (const field of REQUIRED_FIELDS) {
    if (manifest[field] === undefined || manifest[field] === null) {
      throw new Error(`OfficeCLI manifest is missing required field: ${field}`);
    }
  }

  if (!/^https:\/\/github\.com\/[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(manifest.repository)) {
    throw new Error("OfficeCLI repository must be a full GitHub HTTPS URL");
  }
  if (!/^v\d+\.\d+\.\d+$/.test(manifest.tag)) {
    throw new Error("OfficeCLI tag must be an exact semantic version tag");
  }
  if (!/^[0-9a-f]{40}$/.test(manifest.commit)) {
    throw new Error("OfficeCLI commit must be a full lowercase commit SHA");
  }
  if (!/^\d+\.\d+\.\d+$/.test(manifest.dotnetSdk)) {
    throw new Error("OfficeCLI dotnetSdk must be a fixed SDK patch version");
  }
  if (!Array.isArray(manifest.licenseFiles) || manifest.licenseFiles.length === 0) {
    throw new Error("OfficeCLI licenseFiles must not be empty");
  }
  for (const target of Object.keys(TARGETS)) {
    if (typeof manifest.nugetLocks[target] !== "string") {
      throw new Error(`OfficeCLI manifest is missing NuGet lock for ${target}`);
    }
  }

  const expectedArchiveUrl = `${manifest.repository}/archive/${manifest.commit}.tar.gz`;
  if (manifest.sourceArchive?.url !== expectedArchiveUrl) {
    throw new Error("OfficeCLI source archive URL must contain the full pinned commit");
  }
  const hash = manifest.sourceArchive?.sha256;
  if (!/^[0-9a-f]{64}$/.test(hash ?? "") || /^0{64}$/.test(hash)) {
    throw new Error("OfficeCLI source archive SHA-256 is missing or a placeholder");
  }

  return manifest;
}

export function resolveTarget(target) {
  const configuration = TARGETS[target];
  if (!configuration) {
    throw new Error(`Unsupported Rust target: ${target ?? "(missing)"}`);
  }
  return configuration;
}

export function assertTagCommit(tag, expectedCommit, lsRemoteOutput) {
  const [actualCommit, ref] = lsRemoteOutput.trim().split(/\s+/);
  if (ref !== `refs/tags/${tag}` || actualCommit !== expectedCommit) {
    throw new Error(
      `OfficeCLI tag/commit mismatch: ${tag} resolved to ${actualCommit || "nothing"}`,
    );
  }
}

export function assertProjectVersion(tag, projectText) {
  const expectedVersion = tag.slice(1);
  const actualVersion = projectText.match(/<Version>([^<]+)<\/Version>/)?.[1];
  if (actualVersion !== expectedVersion) {
    throw new Error(
      `OfficeCLI project version ${actualVersion ?? "is missing"}; expected ${expectedVersion}`,
    );
  }
}

export async function sha256File(file) {
  const hash = createHash("sha256");
  await pipeline(createReadStream(file), hash);
  return hash.digest("hex");
}

export async function verifyArchiveHash(file, expectedHash) {
  const actualHash = await sha256File(file);
  if (actualHash !== expectedHash) {
    throw new Error(
      `OfficeCLI source archive SHA-256 mismatch: expected ${expectedHash}, got ${actualHash}`,
    );
  }
}

function parseTarget(arguments_) {
  const index = arguments_.indexOf("--target");
  const target = index === -1 ? process.env.TAURI_ENV_TARGET_TRIPLE : arguments_[index + 1];
  if (index !== -1 && !target) {
    throw new Error("--target requires a Rust target triple");
  }
  return target;
}

function run(command, arguments_, options = {}) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, arguments_, {
      cwd: options.cwd ?? repositoryRoot,
      env: options.env ?? process.env,
      stdio: options.capture ? ["ignore", "pipe", "pipe"] : "inherit",
      shell: false,
    });
    let stdout = "";
    let stderr = "";
    child.stdout?.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr?.on("data", (chunk) => {
      stderr += chunk;
    });
    child.on("error", reject);
    child.on("exit", (code) => {
      if (code === 0) {
        resolvePromise(stdout);
      } else {
        reject(
          new Error(`${command} exited with ${code}${stderr ? `: ${stderr.trim()}` : ""}`),
        );
      }
    });
  });
}

async function download(url, destination) {
  const response = await fetch(url, {
    headers: { "User-Agent": "ShenDesk-OfficeCLI-builder" },
    redirect: "follow",
  });
  if (!response.ok || !response.body) {
    throw new Error(`Failed to download OfficeCLI source: HTTP ${response.status}`);
  }
  await pipeline(response.body, createWriteStream(destination));
}

async function checkDotnetVersion(expectedVersion) {
  const actualVersion = (await run("dotnet", ["--version"], { capture: true })).trim();
  if (actualVersion !== expectedVersion) {
    throw new Error(
      `OfficeCLI requires .NET SDK ${expectedVersion}; found ${actualVersion}`,
    );
  }
}

async function verifyLicenses(sourceRoot, licenseFiles) {
  for (const licenseFile of licenseFiles) {
    const [sourceText, vendoredText] = await Promise.all([
      readFile(join(sourceRoot, licenseFile)),
      readFile(join(vendorDirectory, licenseFile)),
    ]);
    if (!sourceText.equals(vendoredText)) {
      throw new Error(`Vendored OfficeCLI ${licenseFile} does not match pinned source`);
    }
  }
}

async function build() {
  const manifest = validateManifest(JSON.parse(await readFile(manifestPath, "utf8")));
  const target = parseTarget(process.argv.slice(2));
  const { rid, extension } = resolveTarget(target);

  const tagRef = await run(
    "git",
    ["ls-remote", "--refs", `${manifest.repository}.git`, `refs/tags/${manifest.tag}`],
    { capture: true },
  );
  assertTagCommit(manifest.tag, manifest.commit, tagRef);
  await checkDotnetVersion(manifest.dotnetSdk);

  const temporaryDirectory = await mkdtemp(join(tmpdir(), "shendesk-officecli-"));
  try {
    const archive = join(temporaryDirectory, "source.tar.gz");
    const extracted = join(temporaryDirectory, "source");
    const publish = join(temporaryDirectory, "publish");
    await mkdir(extracted);
    await download(manifest.sourceArchive.url, archive);
    await verifyArchiveHash(archive, manifest.sourceArchive.sha256);
    await run("tar", ["-xzf", archive, "-C", extracted]);

    const entries = await import("node:fs/promises").then(({ readdir }) => readdir(extracted));
    if (entries.length !== 1) {
      throw new Error("OfficeCLI archive must contain exactly one root directory");
    }
    const sourceRoot = join(extracted, entries[0]);
    const project = join(sourceRoot, manifest.project);
    assertProjectVersion(manifest.tag, await readFile(project, "utf8"));
    await verifyLicenses(sourceRoot, manifest.licenseFiles);

    const lockDestination = join(sourceRoot, manifest.nugetLockDestination);
    await copyFile(join(vendorDirectory, manifest.nugetLocks[target]), lockDestination);
    await run(
      "dotnet",
      ["restore", project, "--locked-mode", "--runtime", rid, "--nologo"],
      { cwd: sourceRoot },
    );
    await run(
      "dotnet",
      [
        "publish",
        project,
        "--configuration",
        "Release",
        "--runtime",
        rid,
        "--self-contained",
        "true",
        "--no-restore",
        "--output",
        publish,
        "--nologo",
      ],
      { cwd: sourceRoot },
    );

    await rm(outputDirectory, { recursive: true, force: true });
    const legalOutput = join(outputDirectory, "officecli-licenses");
    await mkdir(legalOutput, { recursive: true });
    const executableName = `officecli-${target}${extension}`;
    const publishedExecutable = join(publish, `officecli${extension}`);
    await stat(publishedExecutable);
    await copyFile(publishedExecutable, join(outputDirectory, executableName));
    for (const licenseFile of manifest.licenseFiles) {
      await copyFile(join(vendorDirectory, licenseFile), join(legalOutput, licenseFile));
    }
    console.log(`Built ${basename(publishedExecutable)} for ${target} -> ${executableName}`);
  } finally {
    await rm(temporaryDirectory, { recursive: true, force: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  build().catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
}
