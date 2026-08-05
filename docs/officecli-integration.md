# OfficeCLI integration

ShenDesk builds OfficeCLI from an audited source snapshot and bundles it as a
Tauri sidecar. OfficeCLI is infrastructure owned by Rust Core; it is not a WASM
plugin and must never be exposed as an arbitrary command runner to the WebView.

Product scope and acceptance criteria are documented in
[`officecli-integration-prd.md`](officecli-integration-prd.md).

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
The signing order is: build the unsigned sidecar, let Tauri sign nested code
with the entitlement, sign the outer `ShenDesk.app`, then notarize and staple
the app. The action's build wrapper subsequently runs deep signature,
entitlement, native-architecture, stapler, and Gatekeeper checks against that
final app path.

The same wrapper executes the bundled OfficeCLI on both native macOS runners.
It checks the pinned version, then exercises Word, Excel, and PowerPoint through
`create → open → batch → get → PNG preview → close`, confirming that each
resident persists its document. Every invocation sets
`OFFICECLI_SKIP_UPDATE=1`. The wrapper must succeed before `tauri-action`
creates or uploads Draft release assets, so a signing, notarization, or
smoke-test failure publishes nothing.

## Runtime boundary

Every OfficeCLI child process must set `OFFICECLI_SKIP_UPDATE=1` explicitly.
This invariant applies to production execution, smoke tests, and future helper
commands. Runtime code must resolve the bundled sidecar internally and expose
only typed document operations; callers must not provide executable paths or
arbitrary OfficeCLI subcommands.

`application::office_service::OfficeRuntime` is the Rust Core port. Its surface
is limited to version probing, document lifecycle, create, structured inspect,
four whitelisted batch operations, and PNG screenshot rendering.
`infrastructure::office::OfficeCliRuntime` resolves only
the `officecli` executable bundled beside ShenDesk, compares `--version` with
the pinned manifest using an exact SemVer token match, passes arguments without
shell parsing, and captures both
output streams with fixed limits. Missing or incompatible binaries, timeout,
cancellation, non-zero exit, crash, oversized output, and invalid JSON are
mapped to stable service errors. Logs record operation names, error kinds, exit
codes, and byte counts only; they do not record document paths, document
content, named-pipe names, or raw stderr.

Create and edit never publish a partially written destination. Work happens in
a same-filesystem staging location, every owned resident is closed first, and
the result is committed with no-clobber semantics. Existing documents are
copied and the original remains untouched. Screenshot output is written to a
per-call managed temporary directory, checked for PNG signature and a 16 MiB
limit, returned as a data URL, and deleted before the call completes. The
WebView uses that local resource under the existing image CSP; OfficeCLI `watch` and
HTML output are never invoked.

Startup probing has its own two-second timeout instead of using the 30-second
document-operation timeout. Lifecycle JSON must contain a boolean
`success: true`; valid JSON with a missing or non-boolean success field is
rejected. Once `open` starts, the runtime lets its bounded ownership handshake
finish instead of killing only the transient CLI and orphaning its detached
resident. If cancellation arrived during that handshake, a newly started
resident is closed immediately; a reused resident is left untouched.

`OfficeService` canonicalizes supported existing document paths before launch
and uses one async mutex per canonical path, so two ShenDesk operations cannot
write the same document concurrently. A session is registered as owned only
after `open` succeeds. Application operations use `with_document`, which makes
a best-effort `close` after success, failure, or cancellation when that open
started a new resident. A pre-existing resident may be reused but is never
registered as owned or closed by ShenDesk. Shutdown first
cancels transient children registered by this runtime and then closes only the
owned document sessions; it never enumerates or terminates unrelated OfficeCLI
or user processes. No Office lifecycle details are published on EventBus.
