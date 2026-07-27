# ShencomDesk

ShenDesk — Shencom Desktop Platform（深传科技桌面应用平台）。

## 技术基线

- Tauri 2
- React 19 + TypeScript
- Vite 8
- Tailwind CSS 4
- shadcn/ui
- Rust

## 项目结构

```text
apps/desktop        桌面应用
packages/shared     共享实现
packages/types      共享类型
docs                项目文档
```

## 本地开发

仓库提交 `package-lock.json` 与 `apps/desktop/src-tauri/Cargo.lock`，开发和 CI 使用相同的依赖解析结果。

```bash
npm ci
npm run dev
npm run tauri -- dev
```

## 构建

前端构建：

```bash
npm run build
```

生成桌面安装包前，先根据 `apps/desktop/app-icon.svg` 生成各平台图标，再执行 Tauri Build：

```bash
npm run tauri -- icon app-icon.svg
npm run tauri -- build
```

## 质量验证

Pull Request 与 `main` 分支由 GitHub Actions 自动执行：

```bash
npm ci
npm run test
npm run build
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --all-features
```

`npm run test` 同时覆盖前端错误映射测试和合并后构建触发判定测试。

## 合并后自动化构建

Pull Request 合并到 `main` 后，`Post-merge desktop build` Workflow 会在 macOS 与 Windows Runner 上构建 Tauri 安装包，并将产物保留 14 天。

- 普通合并：构建合并后的提交并上传 macOS、Windows Artifact。
- 带 `skip-build` 标签的 PR：只记录跳过原因，不执行构建或上传产物。
- PR 关闭但未合并：不执行构建。
- 需要补构建时：可在 GitHub Actions 中使用 `workflow_dispatch` 手动触发。

构建产物当前不包含正式代码签名、Release 发布或 Tauri Updater 元数据。详细规则和验证方式见 [`docs/automated-builds.md`](docs/automated-builds.md)。

> Rust 与 Tauri 的平台依赖请参考 Tauri 官方环境配置文档。
