# slint-demo

Slint 跨平台系列文章的 **配套 demo 仓库**.

> 项目地址: [github.com/yehun/slint-demo](https://github.com/yehun/slint-demo)

---

## 项目初衷

写 Slint 技术文系列时, 给自己立了一个规矩: **每一篇都配上可运行的 demo, 跑不通就不发文章.**

这个仓库就是那些 demo 的家. 每篇文章对应一个可独立运行的 crate, 统一收在 `crates/` 下, 共享一份依赖和字体资源, 各自支持 **Desktop (Linux/Windows/macOS) / Android / WASM** 五平台构建与运行.

---

## 示例目录

| 目录 | 对应文章 | 主题 | 状态 |
|---|---|---|---|
| [crates/article-02](crates/article-02/) | 第二幕 | 地基与三端初始化 | ✅ 可运行 |
| [crates/slint-common](crates/slint-common/) | — | 共享模板 (预留) | 📦 预留 |
| `crates/article-03` | 第三幕 | Android JNI 桥 | 🚧 待写 |

---

## 快速开始

```bash
# 1. 环境要求: Rust 2024 + wasm-pack 0.15+ + cargo-apk2 + Java 11+ + Android NDK

# 2. 查看某个 demo 的详细说明
cat crates/article-02/README.md

# 3. 运行 desktop
cd crates/article-02
make run-linux

# 4. 构建 Android APK / WASM
make build-apk
make build-wasm
```

每个 demo crate 自带 `Makefile`, 支持 `run-linux` / `run-windows` / `run-macos` / `run-android` / `build-wasm` 等 targets, 详细用法见各 crate 的 `README.md`.

---

## 技术栈

- **UI 框架**: [Slint](https://slint.dev/) master 分支 (1.18.0-dev)
- **语言**: Rust 2024 edition
- **构建**: Cargo + slint-build + wasm-pack + cargo-apk2
- **三端共用一个界面**: `ui/app.slint` + 共享中文字体 (MiSans)

---

## 许可

本仓库为文章配套示例代码, 仅作学习参考.