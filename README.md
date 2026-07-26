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

```bash
npm install
npm run dev
npm run tauri -- dev
```

## 构建

```bash
npm run build
npm run tauri -- build
```

> Rust 与 Tauri 的平台依赖请参考 Tauri 官方环境配置文档。
