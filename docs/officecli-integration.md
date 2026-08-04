# OfficeCLI integration

ShenDesk builds OfficeCLI from an audited source snapshot and bundles it as a
Tauri sidecar. OfficeCLI is infrastructure owned by Rust Core; it is not a WASM
plugin and must never be exposed as an arbitrary command runner to the WebView.

## Pinned supply chain

`third_party/officecli/version.json` is the source of truth for the upstream
repository, tag, full commit, source archive SHA-256, .NET SDK patch version,
project path, per-RID NuGet locks, and license. The saved `LICENSE`, `NOTICE`, and
`THIRD-PARTY-NOTICES.txt` files are copied beside the bundled sidecar notices.

The first baseline is OfficeCLI `v1.0.143` at commit
`fd4adab4dbe3283b62e3edcfad124dd648fa74bc`, built with .NET SDK `10.0.302`.
Upgrades must update the manifest, archive hash, NuGet lock, and saved notices
together after reviewing the upstream diff.

## Build

Install the exact SDK patch from the manifest, then run one of:

```bash
pnpm officecli:build -- --target aarch64-apple-darwin
pnpm officecli:build -- --target x86_64-apple-darwin
pnpm officecli:build -- --target x86_64-pc-windows-msvc
```

The script verifies the tag ref and source hash before extraction, restores
with `dotnet restore --locked-mode`, and publishes a self-contained single-file
binary. It rejects missing or placeholder manifest values and unknown targets.
It never invokes the upstream install scripts, npm packages, `main`, or
`latest`.

Generated files are written to `apps/desktop/src-tauri/binaries/` and are
ignored by Git. Tauri's release and OfficeCLI build configs select only the
current Rust target's sidecar through `bundle.externalBin`; the three license
files are included as bundle resources. Use
`src-tauri/tauri.officecli.conf.json` for non-release installer builds.

macOS release builds use `officecli.entitlements` while Tauri signs the nested
sidecar under hardened runtime. The release workflow verifies the final
`ShenDesk.app/Contents/MacOS/officecli` Developer ID signature and requires
`com.apple.security.cs.allow-jit=true` before accepting the notarized bundle.

## Runtime boundary

Every OfficeCLI child process must set `OFFICECLI_SKIP_UPDATE=1` explicitly.
This invariant applies to production execution, smoke tests, and future helper
commands. Runtime code must resolve the bundled sidecar internally and expose
only typed document operations; callers must not provide executable paths or
arbitrary OfficeCLI subcommands.
