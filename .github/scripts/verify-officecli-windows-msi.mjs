#!/usr/bin/env node

import { readdir, rm, mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { runCommand, verifyOfficeCliInstall } from "./verify-officecli-install.mjs";

export async function findSingleMsi(directory) {
  const files = (await readdir(directory)).filter((file) => file.endsWith(".msi"));
  if (files.length !== 1) {
    throw new Error(`Expected exactly one MSI in ${directory}; found ${files.length}`);
  }
  return join(directory, files[0]);
}

export async function verifyWindowsMsi(msiPath, execute = runCommand) {
  if (process.platform !== "win32") {
    throw new Error("MSI verification must run on Windows");
  }
  const temporaryDirectory = await mkdtemp(join(tmpdir(), "shendesk-msi-smoke-"));
  const installDirectory = join(temporaryDirectory, "ShenDesk");
  let installed = false;
  try {
    await execute(
      "msiexec.exe",
      ["/i", resolve(msiPath), "/qn", "/norestart", `INSTALLDIR=${installDirectory}`],
      { acceptedExitCodes: [0, 3010] },
    );
    installed = true;
    await verifyOfficeCliInstall({
      root: installDirectory,
      expectedArchitecture: "x86_64",
      platform: "win32",
      execute,
    });
  } finally {
    if (installed) {
      await execute(
        "msiexec.exe",
        ["/x", resolve(msiPath), "/qn", "/norestart"],
        { acceptedExitCodes: [0, 3010] },
      ).catch(() => {});
    }
    await rm(temporaryDirectory, { recursive: true, force: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const msiDirectory = process.argv[2];
  if (!msiDirectory) {
    console.error("Usage: verify-officecli-windows-msi.mjs <msi-directory>");
    process.exitCode = 1;
  } else {
    findSingleMsi(resolve(msiDirectory))
      .then((msiPath) => verifyWindowsMsi(msiPath))
      .catch((error) => {
        console.error(error instanceof Error ? error.message : error);
        process.exitCode = 1;
      });
  }
}
