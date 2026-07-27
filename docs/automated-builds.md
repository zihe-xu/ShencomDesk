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

- macOS：生成 `.app` 与 `.dmg`。
- Windows：生成 `.msi` 与 NSIS `.exe`。

Artifact 名称包含平台和目标提交的前 12 位 SHA：

```text
shendesk-macos-<short-sha>
shendesk-windows-<short-sha>
```

Artifact 保留 14 天。找不到预期安装包时，上传步骤会失败，避免出现“Workflow 成功但没有可下载产物”的假成功。

## 构建判定

`.github/scripts/resolve-post-merge-build.mjs` 读取 GitHub 事件载荷并输出：

- 是否执行构建；
- 要检出的合并提交 SHA；
- PR 编号；
- 触发方式；
- 构建或跳过原因。

合并事件必须提供 `merge_commit_sha`。缺少该字段时，判定步骤会失败关闭，而不是回退到可能错误的 PR Head 或默认分支提交。

判定结果会写入 Job Summary。每个平台的构建结果、提交 SHA、Artifact 名称和上传结果也会写入各自的 Job Summary。

## 安全边界

- Workflow 仅授予 `contents: read` 权限。
- 构建 Job 检出合并后的 `merge_commit_sha`，不直接构建未受信任的 PR Head。
- Checkout 不保留 GitHub 凭据。
- 当前流程不读取代码签名、发布或更新签名密钥。

## 应用图标

仓库保留可缩放源文件 `apps/desktop/app-icon.svg`。构建 Runner 使用 Tauri CLI 在打包前生成当前平台需要的 `.icns`、`.ico` 与 PNG 图标，避免平台打包因为缺失图标格式失败。

## 验证

构建判定测试已接入根目录测试命令：

```bash
npm run test
```

也可以只运行判定测试：

```bash
node --test .github/scripts/resolve-post-merge-build.test.mjs
```

测试覆盖：

1. 普通合并执行构建；
2. `skip-build` 合并跳过构建；
3. 关闭但未合并跳过构建；
4. 手动触发执行构建；
5. 合并事件缺少 `merge_commit_sha` 时安全失败。

Workflow 合并后，应通过一次 `workflow_dispatch` 验证真实 macOS、Windows 打包环境；后续 PR 则分别用普通合并和带 `skip-build` 标签的合并验证线上事件路径。

用于验证完整构建路径的 PR 不应添加 `skip-build` 标签，否则只会验证跳过分支。
