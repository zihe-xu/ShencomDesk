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

`npm run test` 同时覆盖前端错误映射、合并后构建判定和签名发布预检约束。

## Phase 3：Plugin System

插件内核采用版本化 Manifest 与 Wasmtime 沙箱，当前支持本地插件安装、启用、命令执行、禁用和卸载。ABI v1 默认不启用 WASI，也不允许任何宿主 import；每次调用具有独立 Store、fuel 与内存/实例限制。

- 插件包：同一目录中的 `plugin.json` 与单个 `.wasm` 模块。
- 安装后默认禁用，启用状态可跨应用重启恢复。
- 插件生命周期通过共享 EventBus 发布领域事件。
- React 统一通过 `apps/desktop/src/services/plugins.ts` 使用类型安全 IPC。
- 插件市场、签名信任链、在线下载和企业策略属于后续独立能力。

Manifest、ABI、生命周期、安全边界和 IPC 详见 [`docs/plugin-system.md`](docs/plugin-system.md)。

## Phase 3：Auto Update

ShenDesk 使用 Tauri Updater 与 GitHub Releases 提供 macOS / Windows 签名更新能力：

- React 只调用 `check_for_updates` 与 `install_update` 两个自定义 IPC，不获得原生 Updater 权限。
- Rust 使用固定 HTTPS `latest.json` 地址，并强制校验 Tauri 更新签名。
- 更新 URL、签名和底层网络/安装错误不会返回 WebView。
- 下载进度通过有序 Tauri Channel 发送。
- 普通开发构建可以不包含公钥；检查更新会安全返回 `update_not_configured`。
- `v<version>` Tag 触发 Draft Release，串行生成 macOS ARM64、macOS Intel、Windows x64 安装包、更新签名和聚合后的 `latest.json`；macOS 正式包还必须通过 Developer ID 签名与 Apple 公证。

签名发布前需要配置 Tauri Updater 密钥，以及 macOS Developer ID 证书与 Apple 公证材料。缺少任一必需配置时，Tag 发布会在跨平台构建前安全失败。完整变量、Secret、运行时和发布流程见 [`docs/auto-update.md`](docs/auto-update.md)。

## 合并后自动化构建

Pull Request 合并到 `main` 后，`Post-merge desktop build` Workflow 会在 macOS 与 Windows Runner 上构建 Tauri 安装包，并将产物保留 14 天。

- 普通合并：构建合并后的提交并上传 macOS、Windows Artifact。
- 带 `skip-build` 标签的 PR：只记录跳过原因，不执行构建或上传产物。
- PR 关闭但未合并：不执行构建。
- 需要补构建时：可在 GitHub Actions 中使用 `workflow_dispatch` 手动触发。

合并后 Artifact 用于构建验证，不包含正式更新签名或 Release 元数据；正式发布只由版本 Tag 的 `Signed desktop release` Workflow 生成。构建规则见 [`docs/automated-builds.md`](docs/automated-builds.md)。

> Rust 与 Tauri 的平台依赖请参考 Tauri 官方环境配置文档。
