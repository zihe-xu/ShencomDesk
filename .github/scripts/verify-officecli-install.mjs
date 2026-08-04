#!/usr/bin/env node

import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(scriptDirectory, "../..");
const manifestPath = join(repositoryRoot, "third_party/officecli/version.json");

export function assertExactVersion(output, expectedVersion) {
  const versions = [
    ...output.matchAll(
      /(?<![0-9A-Za-z.-])v?(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?)(?![0-9A-Za-z.-])/g,
    ),
  ].map((match) => match[1]);
  if (versions.length !== 1 || versions[0] !== expectedVersion) {
    throw new Error(
      `OfficeCLI reported ${JSON.stringify(output.trim())}; expected version ${expectedVersion}`,
    );
  }
}

export function readPeArchitecture(bytes) {
  if (bytes.length < 64 || bytes.toString("ascii", 0, 2) !== "MZ") {
    throw new Error("OfficeCLI is not a Windows PE executable");
  }
  const peOffset = bytes.readUInt32LE(0x3c);
  if (
    peOffset + 6 > bytes.length ||
    bytes.toString("ascii", peOffset, peOffset + 4) !== "PE\0\0"
  ) {
    throw new Error("OfficeCLI has an invalid Windows PE header");
  }
  const machine = bytes.readUInt16LE(peOffset + 4);
  if (machine === 0x8664) return "x86_64";
  if (machine === 0xaa64) return "arm64";
  throw new Error(`OfficeCLI has unsupported PE machine 0x${machine.toString(16)}`);
}

export function runCommand(
  command,
  arguments_,
  { acceptedExitCodes = [0], capture = false, env = process.env } = {},
) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, arguments_, {
      env,
      shell: false,
      stdio: capture ? ["ignore", "pipe", "pipe"] : "inherit",
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
      if (acceptedExitCodes.includes(code)) {
        resolvePromise(stdout);
      } else {
        reject(
          new Error(
            `${basename(command)} exited with ${code}${stderr ? `: ${stderr.trim()}` : ""}`,
          ),
        );
      }
    });
  });
}

export async function verifyDocumentSmoke(sidecarPath, execute = runCommand) {
  const directory = await mkdtemp(join(tmpdir(), "shendesk-officecli-smoke-"));
  const document = join(directory, "bundle-smoke.docx");
  const marker = "ShenDesk bundled OfficeCLI smoke test";
  const env = { ...process.env, OFFICECLI_SKIP_UPDATE: "1" };
  let residentOpen = false;
  try {
    await execute(sidecarPath, ["create", document], { env });
    residentOpen = true;
    await execute(sidecarPath, ["open", document], { env });
    await execute(
      sidecarPath,
      ["add", document, "/body", "--type", "paragraph", "--prop", `text=${marker}`],
      { env },
    );
    const output = await execute(
      sidecarPath,
      ["get", document, "/body/p[last()]", "--json"],
      { capture: true, env },
    );
    if (!output.includes(marker)) {
      throw new Error("OfficeCLI could not read back the smoke-test content");
    }
    await execute(sidecarPath, ["close", document], { env });
    residentOpen = false;
    if ((await stat(document)).size === 0) {
      throw new Error("OfficeCLI did not persist the smoke-test document");
    }
  } finally {
    if (residentOpen) {
      await execute(sidecarPath, ["close", document], { env }).catch(() => {});
    }
    await rm(directory, { recursive: true, force: true });
  }
}

async function verifyArchitecture(sidecarPath, expectedArchitecture, platform, execute) {
  if (platform === "darwin") {
    const output = await execute("lipo", ["-archs", sidecarPath], { capture: true });
    if (output.trim() !== expectedArchitecture) {
      throw new Error(
        `OfficeCLI architecture is ${output.trim()}; expected ${expectedArchitecture}`,
      );
    }
    return;
  }
  if (platform === "win32") {
    const architecture = readPeArchitecture(await readFile(sidecarPath));
    if (architecture !== expectedArchitecture) {
      throw new Error(
        `OfficeCLI architecture is ${architecture}; expected ${expectedArchitecture}`,
      );
    }
    return;
  }
  throw new Error(`Unsupported bundle platform: ${platform}`);
}

export async function verifyOfficeCliInstall({
  root,
  resourceRoot = root,
  expectedArchitecture,
  platform = process.platform,
  execute = runCommand,
}) {
  const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  const sidecarPath = join(root, platform === "win32" ? "officecli.exe" : "officecli");
  await stat(sidecarPath).catch(() => {
    throw new Error(`OfficeCLI sidecar is missing from ${root}`);
  });
  for (const licenseFile of manifest.licenseFiles) {
    await stat(join(resourceRoot, "officecli-licenses", licenseFile)).catch(() => {
      throw new Error(`OfficeCLI legal file is missing: ${licenseFile}`);
    });
  }
  await verifyArchitecture(sidecarPath, expectedArchitecture, platform, execute);
  const versionOutput = await execute(sidecarPath, ["--version"], {
    capture: true,
    env: { ...process.env, OFFICECLI_SKIP_UPDATE: "1" },
  });
  assertExactVersion(versionOutput, manifest.tag.replace(/^v/, ""));
  await verifyDocumentSmoke(sidecarPath, execute);
}

function parseArguments(arguments_) {
  const values = {};
  for (let index = 0; index < arguments_.length; index += 2) {
    const name = arguments_[index];
    const value = arguments_[index + 1];
    if (!name?.startsWith("--") || !value) {
      throw new Error("Expected --root and --expected-arch arguments");
    }
    values[name.slice(2)] = value;
  }
  if (!values.root || !values["expected-arch"]) {
    throw new Error("Expected --root and --expected-arch arguments");
  }
  return values;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const values = parseArguments(process.argv.slice(2));
  verifyOfficeCliInstall({
    root: resolve(values.root),
    resourceRoot: resolve(values.resources ?? values.root),
    expectedArchitecture: values["expected-arch"],
  }).catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
}
