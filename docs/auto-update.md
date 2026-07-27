# Auto Update 自动更新

ShenDesk 使用 Tauri Updater 2、GitHub Releases 与 Tauri 更新签名实现 macOS / Windows 自动更新。运行时只信任发布构建内嵌的公钥，并从固定 HTTPS 地址读取最新版本清单：

```text
https://github.com/zihe-xu/ShencomDesk/releases/latest/download/latest.json
```

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
- `UpdateService` 串行化检查和安装；重叠操作立即返回 `update_busy`。
- `TauriUpdateBackend` 保存待安装的 Tauri `Update`，下载 URL、签名和底层错误不会跨 IPC。
- 检查到新版本时发布 `update_available` 领域事件。
- 下载进度通过 Tauri Channel 有序发送；Command 在安装完成后才解析成功。
- 下载或安装失败时保留已检查更新，允许用户重试；成功后清除。
- 更新检查与下载安装使用 10 分钟 HTTP 超时，避免大型桌面包在慢速网络下被过早中止。

## IPC

### 检查更新

```ts
const update = await checkForUpdates();
if (update) {
  console.log(update.version, update.notes);
}
```

返回 `null` 表示当前版本已是最新。返回对象只包含当前版本、目标版本、说明、发布日期和目标平台，不包含下载地址或签名。

### 下载并安装

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

`finished` 表示签名包下载完成；Command 解析成功表示安装流程也已完成。macOS 在 `restart: true` 时通过 `request_restart` 进入正常退出清理顺序。Windows 安装器可能在安装阶段自动退出应用，因此前端必须把 `finished` 和 Command 断开都视为可能进入安装切换阶段，而不是把断开直接显示成失败。

## 签名模型

Tauri Updater 强制验证更新签名，不能关闭。ShenDesk 使用一对专用更新密钥：

- 公钥：作为 GitHub Actions Repository Variable `SHENDESK_UPDATER_PUBLIC_KEY` 保存，并在发布编译时通过 `option_env!` 嵌入应用。
- 私钥：作为 GitHub Actions Secret `TAURI_SIGNING_PRIVATE_KEY` 保存，只在签名发布 Job 中注入。
- 私钥密码：可选 Secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。

仓库、日志、Job Summary、Artifact 与 IPC 都不得包含私钥。公钥不是秘密，但变更公钥会使已安装旧版本无法验证新密钥签名的更新，因此密钥轮换必须采用兼容迁移方案，不能直接覆盖。

首次发布前，在受控机器生成密钥：

```bash
cd apps/desktop
npm run tauri signer generate -- -w ~/.tauri/shendesk.key
```

生成后立即把私钥移入组织密码库或密钥管理系统，并把对应公钥配置到 Repository Variable。不要把任一密钥文件复制到仓库目录。

## 发布配置

普通开发与合并后构建使用 `tauri.conf.json`，不创建 updater artifact，也不需要签名私钥。只有签名发布 Workflow 叠加：

```text
src-tauri/tauri.release.conf.json
```

该配置启用：

```json
{
  "bundle": {
    "createUpdaterArtifacts": true
  }
}
```

因此普通 CI 可以在没有密钥的情况下编译；没有内嵌公钥的本地构建调用检查更新时会安全返回 `update_not_configured`，不会回退到未签名更新或 HTTP。

## 发布流程

`.github/workflows/release.yml` 只响应 `v*` 标签。预检在任何打包前验证：

1. 根 `package.json`、桌面 `package.json`、Cargo package 与 Tauri config 版本一致且为 SemVer。
2. 标签严格等于 `v<version>`。
3. 基础 Tauri config 没有启用 updater artifacts。
4. release-only config 已启用 updater artifacts。
5. 公钥变量与私钥 Secret 非空。

通过后按平台串行构建（每个平台都会读取并合并现有 `latest.json`，避免并发覆盖）：

- macOS Apple Silicon DMG 与 `.app.tar.gz` 更新包/签名。
- macOS Intel DMG 与 `.app.tar.gz` 更新包/签名。
- Windows x64 MSI 与 MSI 更新签名。
- 多平台 `latest.json`。

Tauri Action 创建 Draft Release。维护者必须核对版本、平台资产、`.sig` 和 `latest.json` 后再手动发布；Draft 不会被 `/releases/latest/` 端点返回。

推荐版本发布顺序：

```text
更新四处版本
  → PR + CI
  → 合并 main
  → 创建并推送 v<version> 标签
  → 签名发布 Workflow
  → 检查 Draft Release
  → 手动 Publish
  → 已安装客户端检查并验证更新
```

## 错误协议

| Code | 场景 |
|---|---|
| `update_not_configured` | 当前构建没有内嵌更新公钥 |
| `update_unsupported` | 当前平台不受 Updater 支持 |
| `update_busy` | 检查或安装操作正在运行 |
| `update_not_available` | 没有经过检查的待安装更新 |
| `update_check_failed` | HTTPS、清单、版本或客户端�败时上��
npm run /tauri.release.装的 Ta��WASM 编译详情或 trap 文本
- 更新 e���布流rogress��� �� 合�```text
��
- m `{ watc版�保证�w 的 库、日志�]������运uri.release.co�证新���ri CLI 在-Tauri config 没有启用 updater artifacts。
4. release-only��本无�|

## � `taa]�����P、Permissions 与 Capabi�
e�表示当前版��验证更新
```

## 错误协��前 | HTT �ate�私钥�b�
## 错�/Ses��序�安装器可能在安装阶段自动退出应用，因此前端必须把 `finished` 和 Command 断开郦�求目录�s��在V�e | `update_b`s���
U``

进度事什有�版本�u载戛�启用 度� recurs`update_available` �
- ���有�版本�u载戛�启用 度��安装器可���fig  构�Variablepps/de回。：

1.`

#�te_chec64 MSI 与 MSI � 库、�被�I 与 M验证。

Artifact ��都�``text�所直�ig  构�Vons 与 Capiort
 � `mac.�:���会跨 IPC〙��p
n.co�证w 版本��时�ck_::,�流ro��装ses/latest/` 端�不获�不获�不获�不获�不获�不获�不获�不获�不获�不获�不获�不获��回。：

1.`

#�te_chec>ffffff更新，因此密钥轮没� | HTTPS、清�l布 J `latesss") {
�SI 与 MSI � 廄织寯-s") ss")�4验证。

Artifact �口 e`。
- 下载 URL、�rtifact �口 e`。
- 下载 URL、�rtifact �口 e`。
- 下载 URL、�rtifact �口 e`。
- 下载 URL、�rtifact �口 e`。
- 下载 URL、�rtifact �口 e`。
- 下载 URL、�rtifact �口 e`。
- 下载 URL、�rtifact �口 e`。
- 下载 URL、�rtifact �口 �!n# 错误协议

| estart�� 库、�：eull Requ��名材料�_check_failedddddddddddddddddddk_faoqu��名�构建� UI�)��ck_标� ��