# article-02 — 第二幕: 地基与三端初始化

Slint 跨平台系列 **第二幕** 的配套 demo: 从零搭建一个能同时在 Linux 桌面、Android 手机和浏览器 (WASM) 上跑起来的 Slint 项目.

> 文章: [第二幕, 地基与三端初始化](../../../docs/) | 仓库总览: [slint-demo](../../README.md)

---

## 功能

- 计数器界面: 标题 / 计数 / `点击 +1` / `重置` / 输入框
- 三端共用 `ui/app.slint` 界面描述
- 等比 DPI 缩放, 窗口自适应布局
- 嵌入 MiSans 中文字体, 解决 Android/WASM 中文乱码

## 目录结构

```
article-02/
├── Cargo.toml              # cdylib + Android 打包配置
├── build.rs                # 编译 .slint + 嵌入中文字体
├── Makefile                # 五平台构建脚本
├── src/
│   ├── main.rs             # desktop 入口
│   └── lib.rs              # Android/WASM 入口
├── ui/
│   └── app.slint           # 三端共用界面描述
├── platform/
│   └── android/java/       # Android NativeActivity 入口
└── wasm/
    └── index.html          # WASM 加载页面
```

### 三端入口

| 平台 | 入口 | 位置 |
|---|---|---|
| Desktop | `fn main()` | `src/main.rs` |
| Android | `fn android_main(app)` | `src/lib.rs` |
| WASM | `#[wasm_bindgen(start)] pub async fn main()` | `src/lib.rs` |

---

## 环境要求

| 工具 | 版本 | 用途 |
|---|---|---|
| Rust | 2024 edition | rustup 安装 |
| Slint | master 分支 (git) | 三端 UI 框架 |
| wasm-pack | 0.15+ | WASM 构建 |
| cargo-apk2 | 最新 (`cargo install cargo-apk2`) | Android APK 构建 |
| Java | 11+ | D8 dex 编译 |
| Android NDK | 28.x | Android 交叉编译 |

---

## 运行

每个目标由 Makefile 统一管理, 先看帮助:

```bash
cd crates/article-02
make help
```

### Desktop (Linux / Windows / macOS)

```bash
make run-linux          # Linux (原生编译 + 运行)
make run-windows        # Windows (交叉编译, 需 mingw-w64)
make run-macos          # macOS (交叉编译, 需 osxcross)
make run-macos-arm      # macOS Apple Silicon (交叉编译)
```

### Android (APK 构建 + 安装 + 运行)

> 需要: NDK + `ANDROID_HOME` + Java 11+
> - Makefile 内置 `JAVA_HOME` / `ANDROID_HOME` / `ANDROID_NDK_HOME`, 按本机修改
> - `.cargo/config.toml` (workspace 根) 已配置 NDK 链接器

```bash
make build-apk           # 构建 debug APK (输出到 target/debug/apk/)
make build-apk-release   # 构建 release APK (输出到 target/release/apk/)
make install-apk         # 安装到连接的设备
make run-android         # 构建 → 安装 → 启动 (一条龙)
make logcat              # 查看运行日志
```

**APK 打包链路**: `cargo-apk2` 自动完成 Rust 编译 → dex 转换 → APK 打包 → 签名(debug keystore); `platform/android/java/MainActivity.java` (extends NativeActivity) 加载 native 库并触发 Rust 的 `android_main`.

### WASM (浏览器)

```bash
make build-wasm          # 构建 WASM (wasm-pack, 输出到 target/wasm/pkg/)
make run-wasm            # 构建并启动 http://127.0.0.1:8080
make check-wasm          # 仅检查编译
```

浏览器打开 http://127.0.0.1:8080. 支持窗口缩放 / 浏览器缩放 (Ctrl+/-) / 跨屏移动时的等比 DPI 自适应.

---

## 关键技术 (踩坑记录)

### 1. WASM 事件循环必须 spawn 模式

默认 `spawn_event_loop = false`, WASM 下调 `run_event_loop()` 会走阻塞的 `run()`, 报 "Using exceptions for control flow". 入口必须手动开启:

```rust
let backend = i_slint_backend_winit::Backend::builder()
    .with_spawn_event_loop(true)
    .build()?;
slint::platform::set_platform(Box::new(backend))?;
```

### 2. HTML 必须包含 `<canvas id="canvas">`

Slint winit 后端会自动查找 `id="canvas"` 的元素传给 winit, 缺失则无法渲染.

### 3. WASM 等比 DPI 缩放

`wasm/index.html` 中用 JS 让 canvas 跟随窗口尺寸, winit 的 `ResizeObserver` 检测 content box 变化并按 `devicePixelRatio` 换算物理像素重绘:

```js
function resize() {
    canvas.style.width = window.innerWidth + 'px';
    canvas.style.height = window.innerHeight + 'px';
}
window.addEventListener('resize', resize);
resize();
```

### 4. 中文字体嵌入

`ui/app.slint` 引入字体 + `build.rs` 用 `embed_resources(EmbedFiles)` 打包, 解决 Android/WASM 无中文字体导致的乱码:

```slint
import "../../../assets/fonts/MiSans-Regular.ttf";
```

字体文件在 workspace 根 `assets/fonts/MiSans-Regular.ttf`, 三端共用.

### 5. wasm-pack 版本

需 ≥0.15 (内置 wasm-bindgen 0.2.127), 旧版本存在版本冲突导致初始化失败.

---

## 常见问题

| 问题 | 解决方案 |
|---|---|
| Android `UnsupportedClassVersionError` | D8 dex 需 Java 11+, Makefile 强制 `JAVA_HOME` 指向 Java 21 |
| Android `Relocations in generic ELF` | workspace 根 `.cargo/config.toml` 配置 NDK 链接器 |
| WASM "Using exceptions for control flow" | 开启 `spawn_event_loop(true)` (见上) |
| WASM 找不到 canvas | HTML 必须包含 `<canvas id="canvas">` |
| Android/WASM 中文乱码 | 嵌入 MiSans 字体 + `embed_resources(EmbedFiles)` |
| WASM 布局在左上角 | JS 设置 canvas style 尺寸, winit ResizeObserver 自动重排 |

---

## 平台支持矩阵

| 功能 | Desktop | Android | WASM |
|---|---|---|---|
| UI 渲染 | ✅ | ✅ | ✅ |
| 事件循环 | ✅ | ✅ | ✅ (spawn 模式) |
| 等比 DPI 缩放 | ✅ (scale_factor) | ✅ (自动) | ✅ (ResizeObserver) |
| 中文字体 | ✅ | ✅ (嵌入) | ✅ (嵌入) |
| 窗口自适应布局 | ✅ | ✅ | ✅ |
| 音频/录音 | ✅ | ✅ | ❌ |
| 视频播放 | ✅ | ✅ | ❌ |
| 摄像头 | ✅ | ✅ | ❌ |
| 文件系统 | ✅ | ⚠️ 需桥接 | ❌ |

---

## 添加新 demo

新的文章对应新的 crate. 完整 11 步模板指南 (Cargo.toml / build.rs / 三端入口 / app.slint / MainActivity.java / index.html / Makefile) 见仓库根 `README.md` 的历史版本或参考本项目结构复制修改: 复制 `crates/article-02/` 为 `crates/article-XX/`, 修改包名、Android 包名和 `ui/app.slint` 界面即可.