# 桌面自动化构建与发布

## Workflow 职责

仓库将质量验证、合并后诊断构建和正式签名发布拆成三个独立流程：

- `CI`：Pull Request 和 `main` 的前端/Rust fake-runtime 测试、构建脚本测试，以及固定 macOS ARM64 目标的 OfficeCLI 源码编译。
- `Post-merge desktop build`：代码进入 `main` 后生成 macOS ARM64、macOS x64、Windows x64 安装包，从最终 App/MSI 验证 OfficeCLI 后上传短期 Artifact。
- `Signed desktop release`：版本标签触发，从最终安装产物完成验收，再使用受保护密钥生成 Tauri Updater 签名、`latest.json` 和 Draft GitHub Release。

普通构建不读取发布密钥；只有标签发布的 release Job 具有 `contents: write` 和签名 Secret。

## 合并后构建触发规则

| 场景 | 判定 | 结果 |
| --- | --- | --- |
| PR 合并到 `main`，没有 `skip-build` | 构建 | 构建合并提交并上传三个目标的 Artifact |
| PR 合并到 `main`，带 `skip-build` | 跳过 | 不执行构建步骤，不上传 Artifact |
| PR 关闭但未合并 | 跳过 | 不执行构建步骤，不上传 Artifact |
| `workflow_dispatch` | 构建 | 构建手动触发时选择的提交 |

`skip-build` 必须添加到将要合并的 Pull Request；给 Issue 添加同名标签不会影响 Workflow。

## 合并后构建目标与产物

`.github/workflows/build.yml` 使用原生 GitHub-hosted Runner：

- macOS Apple Silicon：固定 `macos-26` ARM64 Runner，生成 `.app` 与 `.dmg`。
- macOS Intel：固定 `macos-26-intel` x64 Runner，生成 `.app` 与 `.dmg`。
- Windows：固定 `windows-2022` 与 Visual Studio 2022，生成 `.msi` 与 NSIS `.exe`。

Windows Runner 不使用滚动的 `windows-latest` 标签，避免 GitHub 切换默认 Windows / Visual Studio 镜像时未经评估地改变生产构建环境。升级 Runner 必须通过独立 PR 和真实打包验证。

Artifact 名称包含平台和目标提交的前 12 位 SHA：

```text
shendesk-macos-arm64-<short-sha>
shendesk-macos-x64-<short-sha>
shendesk-windows-<short-sha>
```

Artifact 保留 14 天。找不到预期安装包时上传步骤失败，避免“Workflow 成功但没有产物”的假成功。

这些 Artifact 是当前内部交付渠道，只供有仓库访问权限的内部成员下载。它们不包含 Developer ID、公证或 Tauri Updater 发布元数据：macOS 用户首次启动时通过 Finder 对应用按住 Control 点击并选择“打开”，或在“系统设置 → 隐私与安全性”中单次允许；不得全局关闭 Gatekeeper。Windows 可能显示未知发布者提示，内部用户核对来源和构建提交后再安装。

内部安装步骤：

1. 在仓库 Actions 页面打开目标提交对应且三平台 Job 全绿的 `Post-merge desktop build`。
2. 在运行页面底部下载本机架构对应的 Artifact，并核对名称中的短 SHA。
3. macOS 解压后打开 DMG、将 ShenDesk 拖入“应用程序”，首次启动按上述方式单次允许。
4. Windows 解压后运行 MSI，确认来源为内部仓库构建后接受未知发布者提示。

Artifact 到期后通过 `workflow_dispatch` 对目标提交重新构建，不把旧产物复制到公开下载位置。

三个 Runner 都安装清单固定的 .NET SDK `10.0.302`，从固定 commit、SHA-256 与 NuGet lock 源码构建对应原生 sidecar。macOS 最终 `.app` 和 Windows MSI 静默安装目录必须通过以下门槛后才能上传：

- sidecar 存在且架构分别为 macOS ARM64、macOS x64、Windows x64；
- `--version` 中唯一 SemVer token 与 `third_party/officecli/version.json` 完全相等；
- DOCX、XLSX、PPTX 均完成 `create → open → batch → get → PNG preview → close` round trip；
- `LICENSE`、`NOTICE`、`THIRD-PARTY-NOTICES.txt` 位于最终包的 `officecli-licenses` 资源目录。

所有 smoke 子进程都设置 `OFFICECLI_SKIP_UPDATE=1`。任一检查失败时不上传正常构建 Artifact；原有 `tauri-build.log` 仍作为失败诊断 Artifact 保留 7 天。

## 构建诊断

`.github/scripts/run-tauri-build.mjs` 以跨平台方式运行 Tauri CLI，同时将标准输出和标准错误实时写入控制台与 `tauri-build.log`。

平台构建失败时上传：

```text
shendesk-<platform>-<short-sha>-diagnostics
```

诊断 Artifact 保留 7 天；成功构建不重复保存正常日志。

## 合并判定

`.github/scripts/resolve-post-merge-build.mjs` 读取 GitHub 事件载荷并输出：

- 是否执行构建；
- 要检出的合并提交 SHA；
- PR 编号；
- 触发方式；
- 构建或跳过原因。

合并事件必须提供 `merge_commit_sha`。缺失时安全失败，不回退到 PR Head 或默认分支提交。判定结果、平台结果、Artifact 和诊断状态写入 Job Summary。

## 签名发布

签名发布保留给未来面向外部用户的正式分发，不是 OfficeCLI 内部一期的关闭门槛。没有 Apple Developer Program 时不创建版本 Tag，内部用户继续使用已通过下述 smoke 的合并后 Artifact。

`.github/workflows/release.yml` 只响应 `v*` 标签，不响应 Pull Request。预检脚本 `.github/scripts/validate-release.mjs` 在运行任何跨平台打包前验证版本、标签、release-only Tauri config、Updater 签名材料、Apple Developer ID 证书和公证材料配置状态；Secret 正文不会进入预检进程。

发布矩阵使用固定、原生架构 Runner：

| 目标 | Runner | Bundle | Updater artifact |
|---|---|---|---|
| macOS Apple Silicon | `macos-26` | Developer ID 签名并公证的 DMG | `.app.tar.gz` + `.sig` |
| macOS Intel | `macos-26-intel` | Developer ID 签名并公证的 DMG | `.app.tar.gz` + `.sig` |
| Windows x64 | `windows-2022` | MSI | `.msi` + `.sig` |

`tauri-apps/tauri-action` 将平台资产和 `latest.json` 上传到同一 Draft Release。macOS ARM64/x64 最终 App 在上传前验证原生架构、精确版本、法律文件和文档 round trip，并使用 `codesign` 检查 App/sidecar Developer ID 签名及 sidecar `allow-jit` entitlement，再通过 `stapler` 和 Gatekeeper `spctl` 验证公证票据和系统信任。Windows x64 MSI 在上传前静默安装到隔离目录，并对安装后的 sidecar 执行相同版本、法律文件和文档检查。任一步失败时 `tauri-action` 尚未上传该平台资产，因此失败资产不会进入可发布 Draft。Draft 必须在人工核验后发布，发布后客户端固定的 `/releases/latest/download/latest.json` 才能发现它。

Updater 签名使用：

- Repository Variable：`SHENDESK_UPDATER_PUBLIC_KEY`
- Secret：`TAURI_SIGNING_PRIVATE_KEY`
- 可选 Secret：`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

macOS Developer ID 签名与 Apple 公证使用：

- Repository Variable：`APPLE_SIGNING_IDENTITY`
- Repository Variable：`APPLE_TEAM_ID`
- Secret：`APPLE_CERTIFICATE`（Developer ID Application `.p12` 的 Base64 内容）
- Secret：`APPLE_CERTIFICATE_PASSWORD`
- Secret：`APPLE_ID`
- Secret：`APPLE_PASSWORD`（Apple ID 的 App 专用密码）

`tauri.release.conf.json` 仅在发布 Workflow 叠加，启用 `bundle.createUpdaterArtifacts`。基础 `tauri.conf.json` 保持关闭，保证普通 CI 和本地构建不依赖发布私钥。

完整密钥生成、版本发布和升级验证流程见 [`auto-update.md`](auto-update.md)。

## 安全边界

### 合并后构建

- Workflow 仅授予 `contents: read` 权限。
- 检出合并后的 `merge_commit_sha`，不直接构建未受信任的 PR Head。
- Checkout 不保留 GitHub 凭据。
- 不读取代码签名、发布或更新签名密钥。

### 签名发布

- 仅受保护版本标签触发，不在 PR 代码上下文暴露 Secret。
- Workflow 默认 `contents: read`；只有 release Job 提升为 `contents: write`。
- 预检只接收 Secret 或 Variable 是否已配置的布尔值，不接收或输出 Secret 正文。
- Updater 私钥只注入对应平台的最终 Tauri 构建步骤；Apple 证书和公证 Secret 只注入 macOS Tauri 构建步骤，Windows Job 不接收 Apple 材料。
- 持有私钥的 `tauri-action` 使用完整提交 SHA 固定版本，避免可变标签改变执行代码。
- Release 默认为 Draft，避免未审核清单立即成为 `latest`。
- 更新签名和 OS 平台代码签名是不同信任层；macOS 正式发布强制 Developer ID 签名与 Apple 公证，Windows Authenticode 仍需独立配置。

## 应用图标

`apps/desktop/app-icons/` 集中存放品牌源文件 `logo.svg`，以及带平台安全区的 `logo-macos.svg` 与 `logo-windows.svg`。构建 Runner 使用对应平台的矢量源，通过 Tauri CLI 在打包前生成 `.icns`、`.ico` 与 PNG 图标；`src-tauri/icons/icon.png` 则作为开发运行和窗口图标的默认 PNG 资源。

## 验证

根目录测试命令同时覆盖合并判定、Workflow 约束与签名发布预检：

```bash
pnpm test
```

也可以分别运行：

```bash
node --test .github/scripts/resolve-post-merge-build.test.mjs
node --test .github/scripts/validate-release.test.mjs
node --test .github/scripts/verify-officecli-install.test.mjs
```

测试覆盖普通合并、`skip-build`、未合并关闭、手动构建、缺失 merge SHA 安全失败、固定 Runner/.NET/OfficeCLI target、源码清单与哈希失败、发布版本漂移、错误标签、缺失 Updater 或 Apple 签名材料、release-only updater artifacts、发布 Workflow 的 tag-only/Secret 边界、macOS 签名/公证验收、Windows 安装后验收、精确版本/架构/法律文件与 smoke 失败路径。

Workflow 合并后，应通过 `workflow_dispatch` 验证普通 macOS/Windows 打包；首次签名发布则在配置密钥后使用新的 SemVer 标签，并在 Draft Release 中核对所有平台资产与 `latest.json`。
