# 桌面自动化构建与发布

## Workflow 职责

仓库将质量验证、合并后诊断构建和正式签名发布拆成三个独立流程：

- `CI`：Pull Request 和 `main` 的前端测试、前端构建、Rust 格式检查、Clippy 与 Rust 测试。
- `Post-merge desktop build`：代码进入 `main` 后生成 macOS、Windows 安装包并上传短期 Artifact。
- `Signed desktop release`：版本标签触发，使用受保护密钥生成 Tauri Updater 签名、`latest.json` 和 Draft GitHub Release。

普通构建不读取发布密钥；只有标签发布的 release Job 具有 `contents: write` 和签名 Secret。

## 合并后构建触发规则

| 场景 | 判定 | 结果 |
| --- | --- | --- |
| PR 合并到 `main`，没有 `skip-build` | 构建 | 构建合并提交并上传两个平台的 Artifact |
| PR 合并到 `main`，带 `skip-build` | 跳过 | 不执行构建步骤，不上传 Artifact |
| PR 关闭但未合并 | 跳过 | 不执行构建步骤，不上传 Artifact |
| `workflow_dispatch` | 构建 | 构建手动触发时选择的提交 |

`skip-build` 必须添加到将要合并的 Pull Request；给 Issue 添加同名标签不会影响 Workflow。

## 合并后构建目标与产物

`.github/workflows/build.yml` 使用原生 GitHub-hosted Runner：

- macOS：`macos-latest`，生成 `.app` 与 `.dmg`。
- Windows：固定 `windows-2022` 与 Visual Studio 2022，生成 `.msi` 与 NSIS `.exe`。

Windows Runner 不使用滚动的 `windows-latest` 标签，避免 GitHub 切换默认 Windows / Visual Studio 镜像时未经评估地改变生产构建环境。升级 Runner 必须通过独立 PR 和真实打包验证。

Artifact 名称包含平台和目标提交的前 12 位 SHA：

```text
shendesk-macos-<short-sha>
shendesk-windows-<short-sha>
```

Artifact 保留 14 天。找不到预期安装包时上传步骤失败，避免“Workflow 成功但没有产物”的假成功。

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

`.github/workflows/release.yml` 只响应 `v*` 标签，不响应 Pull Request。预检脚本 `.github/scripts/validate-release.mjs` 在运行任何跨平台打包前验证版本、标签、release-only Tauri config 和签名材料配置状态；私钥正文不会进入预检进程。

发布矩阵使用固定、原生架构 Runner：

| 目标 | Runner | Bundle | Updater artifact |
|---|---|---|---|
| macOS Apple Silicon | `macos-26` | DMG | `.app.tar.gz` + `.sig` |
| macOS Intel | `macos-26-intel` | DMG | `.app.tar.gz` + `.sig` |
| Windows x64 | `windows-2022` | MSI | `.msi` + `.sig` |

`tauri-apps/tauri-action` 将平台资产和 `latest.json` 上传到同一 Draft Release。Draft 必须在人工核验后发布，发布后客户端固定的 `/releases/latest/download/latest.json` 才能发现它。

签名发布使用：

- Repository Variable：`SHENDESK_UPDATER_PUBLIC_KEY`
- Secret：`TAURI_SIGNING_PRIVATE_KEY`
- 可选 Secret：`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

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
- 预检只接收私钥是否已配置的布尔值，不接收或输出私钥正文。
- 发布 Job 只把私钥注入最终 Tauri 签名步骤。
- 持有私钥的 `tauri-action` 使用完整提交 SHA 固定版本，避免可变标签改变执行代码。
- Release 默认为 Draft，避免未审核清单立即成为 `latest`。
- 更新签名和 OS 平台代码签名是不同信任层；Apple notarization 与 Windows Authenticode 需独立配置。

## 应用图标

仓库保留可缩放源文件 `apps/desktop/app-icon.svg`。构建 Runner 使用 Tauri CLI 在打包前生成当前平台需要的 `.icns`、`.ico` 与 PNG 图标。

## 验证

根目录测试命令同时覆盖合并判定、Workflow 约束与签名发布预检：

```bash
npm run test
```

也可以分别运行：

```bash
node --test .github/scripts/resolve-post-merge-build.test.mjs
node --test .github/scripts/validate-release.test.mjs
```

测试覆盖普通合并、`skip-build`、未合并关闭、手动构建、缺失 merge SHA 安全失败、固定 Windows Runner、发布版本漂移、错误标签、缺失密钥、release-only updater artifacts、发布 Workflow 的 tag-only/Secret 边界，以及 ARM/Intel 原生 macOS Runner 约束。

Workflow 合并后，应通过 `workflow_dispatch` 验证普通 macOS/Windows 打包；首次签名发布则在配置密钥后使用新的 SemVer 标签，并在 Draft Release 中核对所有平台资产与 `latest.json`。
