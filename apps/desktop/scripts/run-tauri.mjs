import { spawn } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { loadEnv } from "vite";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(scriptDirectory, "../../..");
const supportedModes = new Set(["development", "production", "test"]);

export function normalizeArgs(args) {
  return args[0] === "--" ? args.slice(1) : args;
}

export function resolveMode(args, environment = process.env) {
  const override = environment.SHENDESK_AUTH_ENVIRONMENT?.trim().toLowerCase();
  const mode = override || (args[0] === "build" ? "production" : "development");

  if (!supportedModes.has(mode)) {
    throw new Error(`Unsupported authentication environment: ${mode}`);
  }

  return mode;
}

export function selectAuthEnvironment(mode, loadedEnvironment) {
  const host = loadedEnvironment.HOST?.trim();
  const scid = loadedEnvironment.SCID?.trim();
  if (!host) {
    throw new Error(`HOST is missing from .env.${mode}`);
  }
  if (!scid) {
    throw new Error(`SCID is missing from .env.${mode}`);
  }

  return {
    SHENDESK_AUTH_ENVIRONMENT: mode,
    SHENDESK_AUTH_HOST: host,
    SHENDESK_AUTH_SCID: scid,
  };
}

export async function runTauri(args = process.argv.slice(2)) {
  args = normalizeArgs(args);
  const mode = resolveMode(args);
  const authEnvironment = selectAuthEnvironment(
    mode,
    loadEnv(mode, repositoryRoot, ""),
  );
  const command = process.platform === "win32" ? "pnpm.cmd" : "pnpm";
  const child = spawn(command, ["run", "tauri:cli", ...args], {
    cwd: resolve(repositoryRoot, "apps/desktop"),
    env: { ...process.env, ...authEnvironment },
    stdio: "inherit",
    windowsHide: true,
  });

  return await new Promise((resolveExit, reject) => {
    child.on("error", reject);
    child.on("exit", (code, signal) => {
      if (signal) {
        reject(new Error(`Tauri terminated by signal ${signal}`));
        return;
      }
      resolveExit(code ?? 1);
    });
  });
}

const isMainModule =
  process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;

if (isMainModule) {
  runTauri()
    .then((code) => {
      process.exitCode = code;
    })
    .catch((error) => {
      console.error(error instanceof Error ? error.message : error);
      process.exitCode = 1;
    });
}
