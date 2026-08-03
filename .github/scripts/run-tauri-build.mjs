import { spawn } from "node:child_process";
import { createWriteStream, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

export function resolveBuildProcess(platform = process.platform, env = process.env) {
  if (platform === "win32") {
    return {
      command: env.ComSpec?.trim() || env.COMSPEC?.trim() || "cmd.exe",
      args: ["/d", "/s", "/c", "pnpm run tauri build"],
    };
  }

  return {
    command: "pnpm",
    args: ["run", "tauri", "build"],
  };
}

function errorMessage(error) {
  return error instanceof Error ? error.message : String(error);
}

async function closeLogStream(logStream) {
  await new Promise((resolveClose, rejectClose) => {
    logStream.once("error", rejectClose);
    logStream.end(resolveClose);
  });
}

export async function runTauriBuild({
  cwd = process.cwd(),
  env = process.env,
  platform = process.platform,
  logPath = resolve(env.TAURI_BUILD_LOG || "tauri-build.log"),
  spawnProcess = spawn,
  stdout = process.stdout,
  stderr = process.stderr,
} = {}) {
  // Create the diagnostics file synchronously so even a synchronous spawn
  // failure can be uploaded by the following workflow step.
  writeFileSync(logPath, "", "utf8");
  const logStream = createWriteStream(logPath, { flags: "a" });

  const writeChunk = (target, chunk) => {
    target.write(chunk);
    logStream.write(chunk);
  };

  const { command, args } = resolveBuildProcess(platform, env);
  let child;
  try {
    child = spawnProcess(command, args, {
      cwd,
      env,
      stdio: ["inherit", "pipe", "pipe"],
      windowsHide: true,
    });
  } catch (error) {
    const message = `Unable to start Tauri build: ${errorMessage(error)}\n`;
    stderr.write(message);
    logStream.write(message);
    await closeLogStream(logStream);
    return 1;
  }

  const exitCode = await new Promise((resolveExitCode) => {
    let settled = false;
    const settle = (code) => {
      if (settled) {
        return;
      }
      settled = true;
      resolveExitCode(code);
    };

    child.stdout.on("data", (chunk) => writeChunk(stdout, chunk));
    child.stderr.on("data", (chunk) => writeChunk(stderr, chunk));

    child.on("error", (error) => {
      const message = `Unable to start Tauri build: ${errorMessage(error)}\n`;
      stderr.write(message);
      logStream.write(message);
      settle(1);
    });

    child.on("close", (code, signal) => {
      if (signal) {
        const message = `Tauri build terminated by signal ${signal}\n`;
        stderr.write(message);
        logStream.write(message);
        settle(1);
        return;
      }

      settle(code ?? 1);
    });
  });

  await closeLogStream(logStream);

  if (exitCode !== 0) {
    stderr.write(`Tauri build failed with exit code ${exitCode}. Diagnostics: ${logPath}\n`);
  }

  return exitCode;
}

const isMainModule =
  process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;

if (isMainModule) {
  try {
    process.exitCode = await runTauriBuild();
  } catch (error) {
    console.error(`Tauri build wrapper failed: ${errorMessage(error)}`);
    process.exitCode = 1;
  }
}
