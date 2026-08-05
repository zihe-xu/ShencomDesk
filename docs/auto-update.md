# Auto Update 自动更新

ShenDesk 使用 Tauri Updater 2、GitHub Releases 与 Tauri 更新签名实现 macOS / Windows 自动更新。运行时只信任发布构建内嵌的公钥，并从固定 HTTPS 地址读取最新版本清单：

```text
https://github.com/zihe-xu/ShencomDesk/releases/latest/download/latest.json
```

该流程用于未来面向外部用户的正式签名发布。当前内部交付直接使用受访问控制的合并后 Workflow Artifact，不创建版本 Tag、`latest.json` 或未公证的公开 Release，因此不要求 Apple Developer Program。

## 运行时架构

```text
React Update Service
        ↓ ShenDesk IPC
Update Command
        ↓
UpdateService (Application)
        ↓ UpdateBackend port
TauriUpdateBackend (Infrastructure)
        ↓
tauri-plugin-updater
```

- React 不直接调用 `@tauri-apps/plugin-updater`，也不获得 Updater 原生命令权限。
- `UpdateService` 串行化检查和安装；重叠操作返回 `update_busy`。
- `TauriUpdateBackend` 保存待安装的 Tauri `Update` 对象。下载 URL、签名、原始清单和底层错误不会跨 IPC。
- 检查到新版本时，通过共享 EventBus 发布 `update_available`。
- 下载进度使用 Tauri Channel 有序发送。
- 下载、签名验证或安装失败时保留待安装对象，允许用户重试；安装成功后清除。
- 更新检查和下载使用 10 分钟超时，避免大型桌面包在慢速网络下过早中止。

## IPC

### 检查更新

```ts
const update = await checkForUpdates();

if (update) {
  console.log(update.version, update.notes);
}
```

`check_for_updates` 返回 `UpdateInfo | null`：

```ts
interface UpdateInfo {
  currentVersion: string;
  version: string;
  notes: string | null;
  publishedAtUnixSeconds: number | null;
  target: string;
}
```

返回 `null` 表示当前版本已经是最新版本。响应不包含下载地址、签名或原始更新清单。

### 下载、验证并安装

```ts
await installUpdate({
  restart: true,
  onProgress(event) {
    if (event.event === "progress") {
      console.log(event.data.downloaded, event.data.contentLength);
    }
  },
});
```

进度事件顺序：

```text
started → progress (0..N) → finished
```

`finished` 表示更新包已经下载并完成签名验证，命令成功返回表示安装流程也已完成。Windows 安装器在安装阶段可能自动退出应用；macOS 或安装后仍在运行的平台在 `restart: true` 时通过 `request_restart` 进入 ShenDesk 正常退出清理顺序。

## 稳定错误码

| Code | 场景 |
|---|---|
| `update_not_configured` | 当前构建没有内嵌更新公钥 |
| `update_busy` | 已有检查或安装操作在运行 |
| `update_not_available` | 没有经过检查的待安装更新 |
| `update_check_failed` | 更新检查失败 |
| `update_install_failed` | 下载、签名验证或安装失败 |
| `update_operation_failed` | 其他内部更新服务错误 |

Rust 日志可以记录用于诊断的底层错误，但 IPC 只返回固定消息，不暴露 URL、签名、请求头、清单内容或安装器内部信息。

## 签名与密钥

Tauri Updater 强制验证更新签名，ShenDesk 不提供关闭验证或 HTTP 降级的配置。

密钥职责：

- Repository Variable `SHENDESK_UPDATER_PUBLIC_KEY`：发布编译时通过 `option_env!` 内嵌到应用。
- Repository Secret `TAURI_SIGNING_PRIVATE_KEY`：只注入签名发布 Job。
- 可选 Secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`：保护私钥。

仓库、日志、Job Summary、Artifact 和 IPC 都不得包含私钥。公钥不是秘密，但公钥轮换会影响旧版本验证新更新，因此必须通过兼容迁移版本完成，不能直接覆盖。

首次发布前，应在受控机器生成密钥并立即把私钥移入组织密码库或密钥管理系统：

```bash
cd apps/desktop
pnpm tauri signer generate -- -w ~/.tauri/shendesk.key
```

不要把生成的密钥文件复制到仓库目录。

## 构建配置

普通开发、CI 和合并后构建使用 `tauri.conf.json`，不创建 updater artifact，也不需要签名私钥。

基础配置中的 `plugins.updater.pubkey` 保持为空字符串，仅用于满足 updater 插件启动时的配置反序列化。实际更新公钥不会从该配置读取，而是在发布编译时通过 `SHENDESK_UPDATER_PUBLIC_KEY` 内嵌，并由 `TauriUpdateBackend` 在每次创建 updater 客户端时设置。

签名发布额外叠加：

```text
apps/desktop/src-tauri/tauri.release.conf.json
```

其唯一职责是启用：

```json
{
  "bundle": {
    "createUpdaterArtifacts": true
  }
}
```

普通构建没有内嵌公钥时，调用检查更新会安全返回 `update_not_configured`，不会连接未签名或不安全的更新源。

## 发布 Workflow

`.github/workflows/release.yml` 只响应 `v*` Tag。预检在任何打包前验证：

1. 根 `package.json`、桌面 `package.json`、Cargo package 与 Tauri config 版本一致且为 SemVer。
2. Tag 严格等于 `v<version>`。
3. 基础 Tauri config 未启用 updater artifacts。
4. release-only config 已启用 updater artifacts。
5. Updater 公钥 Variable 与私钥 Secret 非空。
6. Apple Developer ID 证书、签名身份、Team ID、Apple ID 与 App 专用密码均已配置。
7. `tauri-action` 固定到经审查的完整提交 SHA。

通过后按平台串行构建，避免多个 Job 同时读取、删除和上传 `latest.json`：

- macOS Apple Silicon：DMG、更新包和 `.sig`。
- macOS Intel：DMG、更新包和 `.sig`。
- Windows x64：MSI、更新包和 `.sig`。
- 聚合后的多平台 `latest.json`。

macOS 构建将 Developer ID Application 证书交给 Tauri 完成应用签名和 Apple 公证，并在构建后使用 `codesign --verify`、`xcrun stapler validate` 和 `spctl --assess` 验证签名、公证票据与 Gatekeeper 判定。只有 Tauri Updater `.sig`、但没有 Apple 平台签名的应用会被 Gatekeeper 拒绝，不能作为可发布基线。

Workflow 创建 Draft Release。维护者必须核对版本、平台资产、签名和 `latest.json` 后再手动发布；Draft 不会被 `/releases/latest/` 返回。

## 首次签名发布清单

1. 安全生成并备份更新密钥。
2. 配置 `SHENDESK_UPDATER_PUBLIC_KEY`。
3. 配置 `TAURI_SIGNING_PRIVATE_KEY` 和可选密码。
4. 配置 `APPLE_SIGNING_IDENTITY` 与 `APPLE_TEAM_ID` Repository Variables。
5. 配置 `APPLE_CERTIFICATE`、`APPLE_CERTIFICATE_PASSWORD`、`APPLE_ID` 与 `APPLE_PASSWORD` Secrets；`APPLE_CERTIFICATE` 是 Developer ID Application `.p12` 的 Base64 内容，`APPLE_PASSWORD` 是 App 专用密码。
6. 同步更新四处应用版本。
7. 合并版本变更并确认普通 CI 通过。
8. 创建严格匹配的 `v<version>` Tag。
9. 等待三个平台签名构建和 macOS Gatekeeper 验证完成。
10. 检查 Draft Release 包含全部安装包、更新包、`.sig` 与 `latest.json`。
11. 在对应架构测试机器完成全新安装，再发布 Release。

## 验证

根目录测试包含发布预检约束：

```bash
pnpm test
```

Rust CI 另外验证：

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

真实签名产物只能在配置密钥后由版本 Tag Workflow 验收。本功能 PR 不创建版本 Tag，也不生成或提交真实密钥。
