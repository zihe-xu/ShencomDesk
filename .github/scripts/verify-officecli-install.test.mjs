import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  assertExactVersion,
  readPeArchitecture,
  runCommand,
  verifyDocumentSmoke,
  verifyOfficeCliInstall,
} from "./verify-officecli-install.mjs";
import { findSingleMsi } from "./verify-officecli-windows-msi.mjs";

test("compares the complete OfficeCLI version token", () => {
  assert.doesNotThrow(() => assertExactVersion("officecli 1.0.143\n", "1.0.143"));
  assert.throws(
    () => assertExactVersion("officecli 11.0.143\n", "1.0.143"),
    /expected version 1\.0\.143/,
  );
  assert.throws(
    () => assertExactVersion("officecli 1.0.143-beta\n", "1.0.143"),
    /expected version 1\.0\.143/,
  );
  assert.throws(
    () => assertExactVersion("officecli 1.0.143.1\n", "1.0.143"),
    /expected version 1\.0\.143/,
  );
  assert.throws(() => assertExactVersion("{}", "1.0.143"), /expected version/);
});

test("reads the architecture from a PE header", () => {
  const executable = Buffer.alloc(256);
  executable.write("MZ", 0, "ascii");
  executable.writeUInt32LE(128, 0x3c);
  executable.write("PE\0\0", 128, "ascii");
  executable.writeUInt16LE(0x8664, 132);
  assert.equal(readPeArchitecture(executable), "x86_64");
  executable.writeUInt16LE(0xaa64, 132);
  assert.equal(readPeArchitecture(executable), "arm64");
  executable.write("NO", 0, "ascii");
  assert.throws(() => readPeArchitecture(executable), /not a Windows PE/);
});

test("allows documented successful installer exit codes", async () => {
  await runCommand(process.execPath, ["-e", "process.exit(3)"], {
    acceptedExitCodes: [0, 3],
    capture: true,
  });
  await assert.rejects(
    () => runCommand(process.execPath, ["-e", "process.exit(3)"], { capture: true }),
    /exited with 3/,
  );
});

test("document smoke failures close an opened resident", async () => {
  const calls = [];
  await assert.rejects(
    () =>
      verifyDocumentSmoke("officecli", async (_command, arguments_) => {
        calls.push(arguments_[0]);
        if (arguments_[0] === "get") return "missing marker";
        return "";
      }),
    /could not read back/,
  );
  assert.deepEqual(calls, ["create", "open", "add", "get", "close"]);
});

test("requires exactly one Windows installer", async () => {
  const directory = await mkdtemp(join(tmpdir(), "shendesk-msi-test-"));
  await assert.rejects(() => findSingleMsi(directory), /found 0/);
  await writeFile(join(directory, "ShenDesk.msi"), "fixture");
  assert.equal(await findSingleMsi(directory), join(directory, "ShenDesk.msi"));
  await mkdir(join(directory, "ignored"));
  await writeFile(join(directory, "second.msi"), "fixture");
  await assert.rejects(() => findSingleMsi(directory), /found 2/);
  await rm(directory, { recursive: true, force: true });
});

test("requires the sidecar and every legal file before smoke testing", async () => {
  const root = await mkdtemp(join(tmpdir(), "shendesk-officecli-install-test-"));
  const executable = Buffer.alloc(256);
  executable.write("MZ", 0, "ascii");
  executable.writeUInt32LE(128, 0x3c);
  executable.write("PE\0\0", 128, "ascii");
  executable.writeUInt16LE(0x8664, 132);
  await writeFile(join(root, "officecli.exe"), executable);
  await mkdir(join(root, "officecli-licenses"));
  for (const file of ["LICENSE", "NOTICE", "THIRD-PARTY-NOTICES.txt"]) {
    await writeFile(join(root, "officecli-licenses", file), "fixture");
  }

  const execute = async (_command, arguments_) => {
    if (arguments_[0] === "--version") return "officecli 1.0.143\n";
    if (arguments_[0] === "create") await writeFile(arguments_[1], "document");
    if (arguments_[0] === "get") return "ShenDesk bundled OfficeCLI smoke test";
    return "";
  };
  await verifyOfficeCliInstall({
    root,
    expectedArchitecture: "x86_64",
    platform: "win32",
    execute,
  });

  await rm(join(root, "officecli-licenses", "NOTICE"));
  await assert.rejects(
    () =>
      verifyOfficeCliInstall({
        root,
        expectedArchitecture: "x86_64",
        platform: "win32",
        execute,
      }),
    /legal file is missing: NOTICE/,
  );
  await rm(join(root, "officecli.exe"));
  await assert.rejects(
    () =>
      verifyOfficeCliInstall({
        root,
        expectedArchitecture: "x86_64",
        platform: "win32",
        execute,
      }),
    /sidecar is missing/,
  );
  await rm(root, { recursive: true, force: true });
});
