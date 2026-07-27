import { spawn } from "node:child_process";
import { createWriteStream } from "node:fs";
import { resolve } from "node:path";

const logPath = resolve(process.env.TAURI_BUILD_LOG || "tauri-build.log");
const npmCommand = process.platform === "win32" ? "npm.cmd" : "npm";
const logStream = createWriteStream(logPath, { flags: "w" });

function writeChunk(target, chunk) {
  target.write(chunk);
  logStream.write(chunk);
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

  const child = spawn(npmCommand, ["run", "tauri", "--", "build"], {
    cwd: process.cwd(),
    env: process.env,
    stdio: ["inherit", "pipe", "pipe"],
    windowsHide: true,
  });

  child.stdout.on("data", (chunk) => writeChunk(process.stdout, chunk));
  child.stderr.on("data", (chunk) => writeChunk(process.stderr, chunk));

  child.on("error", (error) => {
    const message = `Unable to start Tauri build: ${error.message}\n`;
    process.stderr.write(message);
    logStream.write(message);
    settle(1);
  });

  child.on("close", (code, signal) => {
    if (signal) {
      const message = `Tauri build terminated by signal ${signal}\n`;
      process.stderr.write(message);
      logStream.write(message);
      settle(1);
      return;
    }

    settle(code ?? 1);
  });
});

await new Promise((resolveClose, rejectClose) => {
  logStream.once("error", rejectClose);
  logStream.end(resolveClose);
});

if (exitCode !== 0) {
  console.error(`Tauri build failed with exit code ${exitCode}. Diagnostics: ${logPath}`);
}

process.exitCode = exitCode;
