# article-15 — 第十五篇: Slint 国际化(i18n)

Slint 跨平台系列 **第十五篇** 的配套 demo: 一套可运行的国际化示例, 支持桌面三端(Windows / Linux / macOS)
与 Android, 既能自动识别系统语言, 也能手动一键热切换。

> 文章: [20260922-第十五幕, Slint国际化](../../20260922-第十五幕, Slint国际化.md) | 仓库总览: [slint-demo](../../README.md)

---

## 功能

- **UI 文案翻译**: `.slint` 里用 `@tr("...")` 标记, `.po` → `.mo` → bundled 进二进制
- **Rust 文案翻译**: `i18n::gettext("...")`, 用内置极简 `.mo` 解析器(`include_bytes!` 嵌入)
- **运行期热切换**: `slint::select_bundled_translation(locale)`, 点按钮整窗立即换语言
- **系统语言检测**: 桌面用 `sys-locale`; Android 用 JNI 调 `Locale.getDefault().toLanguageTag()`
- **两套语言**: `zh_CN` / `en`

---

## 目录结构

```
crates/article-15/
├── Cargo.toml          # desktop / android features + cargo-apk2 元数据
├── build.rs            # msgfmt 编译 .po + with_bundled_translations
├── Makefile            # 五平台构建
├── lang/               # 翻译源(.po/.pot) + 构建产物(.mo, gitignore, build.rs 生成)
│   ├── article-15.pot  # slint-tr-extractor 提取出的模板(工具生成, 非手写)
│   ├── zh_CN/LC_MESSAGES/{article-15,article-15-rs}.{po,mo}
│   └── en/LC_MESSAGES/article-15.{po,mo}
├── src/
│   ├── main.rs         # Desktop 入口
│   ├── lib.rs          # desktop_main / android_main + i18n 初始化 + 语言切换 + 系统语言检测
│   └── i18n.rs         # Rust 侧内置 .mo 解析器
├── ui/app.slint        # @tr + I18nModel 全局状态 + 切换按钮
└── android/java/com/example/article15/MainActivity.java
```

---

## 构建与运行

```bash
# Desktop
make run-linux          # Linux (或 run-windows / run-macos)
make check              # 快速检查

# Android
make build-apk          # 构建 debug APK
make install-apk        # 安装到设备
make run-android        # 构建 + 安装 + 启动
make logcat             # 查看日志

# 单独验证 Android(桌面 check 抓不到 Android-only 代码, 必须单独跑)
cargo check -p article-15 --target aarch64-linux-android \
    --no-default-features --features android
```

> Android 构建要求: NDK 已配置(供 `gettext-sys` 交叉编译) + `JAVA_HOME` 指向 Java 11+(D8 dex)。
> 详见文章"关键踩坑"一节。

---

## 翻译工作流 —— 官方 `slint-tr-extractor`

Slint 官方对国际化有完整文档, 并提供了提取工具 [`slint-tr-extractor`](https://crates.io/crates/slint-tr-extractor)。
本项目的 `.po` **不是手写的**, 而是先用官方工具从 `.slint` 里提取出 `.pot` 模板, 再对模板填翻译:

```bash
cargo install slint-tr-extractor        # 一次性安装

# 1) 提取: .slint 里的 @tr 字符串 -> lang/article-15.pot
make i18n-pot
#   = slint-tr-extractor --package-name article-15 -o lang/article-15.pot ui/app.slint

# 2) 更新: 用 .pot 刷新各语言 .po(保留已有翻译) + 编译 .mo
make i18n
```

生成结果(`msgctxt` 由工具按"所在组件名"自动写入):

```po
#: ui/app.slint:32
msgctxt "MainWindow"
msgid "Hello, world!"
msgstr "你好，世界！"
```

> **命名要点**: 官方规定 `domain` = **crate 名**。本项目 crate 为 `article-15`, 因此 UI 侧目录名/文件名是
> `lang/<locale>/LC_MESSAGES/article-15.{po,mo}`。
>
> **边界**: `slint-tr-extractor` 只处理 `.slint` 里的 `@tr`。Rust 代码里的字符串
> (`article-15-rs.*`) 不在其管辖范围, 需单独维护。

---

## 国际化要点速查

| 线 | 标记 | 加载 |
|---|---|---|
| UI 文本 | `.slint` 里 `@tr("...")` | `slint::init_translations!` + `slint::select_bundled_translation` |
| Rust 文本 | `i18n::gettext("...")` | 内置 `.mo` 解析器 + `include_bytes!` |

- `@tr` 的 context = 所在组件名(本 demo 为 `MainWindow`), `.po` 的 `msgctxt` 必须对上, 否则静默回退英文。
- 改了 `.po` 记得重新生成 `.mo`(`build.rs` 会在 `.po` 变化时重跑 `msgfmt`)。
