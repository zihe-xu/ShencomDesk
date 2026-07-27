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

```bash
npm run build
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

## Phase 3：Plugin System

插件内核采用版本化 Manifest 与 Wasmtime 沙箱，当前支持本地插件安装、启用、命令执行、禁用和卸载。ABI v1 默认不启用 WASI，也不允许任何宿主 import；每次调用具有独立 Store、fuel 与内存/实例限制。

- 插件包：同一目录中的 `plugin.json` 与单个 `.wasm` 模块。
- 安装后默认禁用，启用状态可跨应用重启恢复。
- 插件生命周期通过共享 EventBus 发布领域事件。
- React 统一通过 `apps/desktop/src/services/plugins.ts` 使用类型安全 IPC。
- 插件市场、签名信任链、在线下载和企业策略属于后续独立能力。

Manifest、ABI、生命周期、安全边界和 IPC 详见 [`docs/plugin-system.md`](docs/plugin-system.md)。

## 合并后自动化构建

Pull Request 合并到 `main` 后，`Post-merge desktop build` Workflow 会在 macOS 与 Windows Runner 上构建 Tauri 安装包，并将产物保留 14 天。

- 普通合并：构建合并后的提交并上传 macOS、Windows Artifact。
- 带 `skip-build` 标签的 PR：只记录跳过原因，不执行构建或上传产物。
- PR 关闭但未合并：不执行构建。
- 需要补构建时：可在 GitHub Actions 中使用 `workflow_dispatch` 手动触发。

构建产物当前不包含正式代码签名、Release 发布或 Tauri Updater 元数据。详细规则和验证方式见 [`docs/automated-builds.md`](docs/automated-builds.md)。

> Rust 与 Tauri 的平台依赖请参考 Tauri 官方环境配置文档。
