# 合并后自动化构建

## 目标

`.github/workflows/build.yml` 负责在代码进入 `main` 后生成 ShenDesk 桌面安装包。质量检查继续由 `.github/workflows/ci.yml` 承担，两个 Workflow 的职责互不混用：

- `CI`：Pull Request 和 `main` 的前端测试、前端构建、Rust 格式检查、Clippy 与 Rust 测试。
- `Post-merge desktop build`：合并完成后的 macOS、Windows Tauri 打包与 Artifact 上传。

## 触发规则

| 场景 | 判定 | 结果 |
| --- | --- | --- |
| PR 合并到 `main`，没有 `skip-build` | 构建 | 构建合并提交并上传两个平台的 Artifact |
| PR 合并到 `main`，带 `skip-build` | 跳过 | 不执行构建步骤，不上传 Artifact |
| PR 关闭但未合并 | 跳过 | 不执行构建步骤，不上传 Artifact |
| `workflow_dispatch` | 构建 | 构建手动触发时选择的提交 |

`skip-build` 必须添加到将要合并的 Pull Request；给 Issue 添加同名标签不会影响 Workflow。

## 构建目标与产物

Workflow 使用原生 GitHub-hosted Runner：

- macOS：使用 `macos-latest`，生成 `.app` 与 `.dmg`。
- Windows：固定使用 `windows-2022` 与 Visual Studio 2022，生成 `.msi` 与 NSIS `.exe`。

Windows Runner 不使用滚动的 `windows-latest` 标签，避免 GitHub 切换默认 Windows / Visual Studio 镜像时未经评估地改变生产构建环境。升级 Runner 必须通过独立 PR 和真实打包验证。

Artifact 名称包含平台和目标提交的前 12 位 SHA：

```text
shendesk-macos-<short-sha>
shendesk-windows-<short-sha>
```

Artifact 保留 14 天。找不到预期安装包时，上传步骤会失败，避免出现“Workflow 成功但没有可下载产物”的假成功。

## 构建诊断

macOS 使用 Bash 的 `pipefail` 与 `tee`，Windows 使用 PowerShell 的 `Tee-Object`，将 Tauri 的完整标准输出和标准错误同时写入控制台与 `apps/desktop/tauri-build.log`。原生 Shell 直接保留 Tauri 命令退出码，不再依赖额外的进程包装脚本。

平台构建失败时，Workflow 会上传：

```text
shendesk-<platform>-<short-sha>-diagnostics
```

诊断 Artifact 保留 7 天。诊断日志缺失会使上传步骤明确失败；成功构建不会上传诊断日志，避免重复保存正常构建输出。

## 构建判定

`.github/scripts/resolve-post-merge-build.mjs` 读取 GitHub 事件载荷并输出：

- 是否执行构建；
- 要检出的合并提交 SHA；
- PR 编号；
- 触发方式；
- 构建或跳过原因。

合并事件必须提供 `merge_commit_sha`。缺少该字段时，判定步骤会失败关闭，而不是回退到可能错误的 PR Head 或默认分支提交。

判定结果会写入 Job Summary。每个平台的构建结果、提交 SHA、Artifact 名称、诊断日志状态和上传结果也会写入各自的 Job Summary。

## 安全边界

- Workflow 仅授予 `contents: read` 权限。
- 构建 Job 检出合并后的 `merge_commit_sha`，不直接构建未受信任的 PR Head。
- Checkout 不保留 GitHub 凭据。
- 当前流程不读取代码签名、发布或更新签名密钥。

## 应用图标

仓库保留可缩放源文件 `apps/desktop/app-icon.svg`。构建 Runner 在打包前执行：

```bash
npm run tauri -- icon app-icon.svg
```

该命令在 `apps/desktop/src-tauri/icons` 中生成桌面平台图标。`tauri.conf.json > bundle > icon` 显式声明以下文件：

```text
icons/32x32.png
icons/128x128.png
icons/128x128@2x.png
icons/icon.icns
icons/icon.ico
```

Windows WiX / NSIS 打包依赖 `.ico`，macOS App / DMG 打包依赖 `.icns`。仅生成文件但不在 Bundle 配置中声明，会导致平台安装包无法稳定找到对应图标。

## 验证

构建判定与 Workflow 约束测试已接入根目录测试命令：

```bash
npm run test
```

也可以只运行自动化构建测试：

```bash
node --test .github/scripts/resolve-post-merge-build.test.mjs
```

测试覆盖：

1. 普通合并执行构建；
2. `skip-build` 合并跳过构建；
3. 关闭但未合并跳过构建；
4. 手动触发执行构建；
5. 合并事件缺少 `merge_commit_sha` 时安全失败；
6. Windows Runner 固定为 `windows-2022`；
7. macOS / Windows 原生日志捕获与失败诊断配置存在；
8. Tauri Bundle 显式声明 PNG、ICNS 和 ICO 图标。

Workflow 合并后，应通过一次 `workflow_dispatch` 验证真实 macOS、Windows 打包环境；后续 PR 则分别用普通合并和带 `skip-build` 标签的合并验证线上事件路径。

用于验证完整构建路径的 PR 不应添加 `skip-build` 标签，否则只会验证跳过分支。
