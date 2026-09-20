# ============================================================
# Article 14: Slint开发的第一个视频播放器
# ffmpeg-next 解码 + cpal 音频 + A/V 同步
# 支持平台: Linux / Windows / macOS / Android
# ============================================================

# 配置
CRATE_NAME   := article-14
BIN_NAME     := article_14
FEATURES     := desktop

# Rust 目标平台
TARGET_LINUX   := x86_64-unknown-linux-gnu
TARGET_WINDOWS := x86_64-pc-windows-gnu
TARGET_MACOS   := x86_64-apple-darwin
TARGET_MACOS_ARM := aarch64-apple-darwin
TARGET_ANDROID := aarch64-linux-android

# Android 配置
ANDROID_PACKAGE := com.example.article14
TARGET_DIR := $(shell cargo metadata --format-version 1 --no-deps 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin)['target_directory'])" 2>/dev/null || echo "../target")
APK_PATH := $(TARGET_DIR)/debug/apk/$(BIN_NAME).apk

# Java: D8 dex 编译器需要 Java 11+
JAVA_HOME := /usr/lib/jvm/java-21-openjdk-amd64
export JAVA_HOME
export ANDROID_HOME := /data/software/java/androiod-sdk
export ANDROID_NDK_HOME := /data/software/java/androiod-sdk/ndk/28.0.13004108

# ============================================================
# 默认目标
# ============================================================
.PHONY: help
help:
	@echo "Article 14 — Slint开发的第一个视频播放器"
	@echo ""
	@echo "用法: make <target>"
	@echo ""
	@echo "  run                 编译并运行 (当前平台)"
	@echo "  run-release         编译并运行 (release 模式)"
	@echo "  preview             使用 slint-viewer 预览 UI (无需编译 Rust, 支持热重载)"
	@echo "  build               编译当前平台 debug"
	@echo "  clean               清理编译产物"
	@echo "  install-target-android  安装 Android 目标"
	@echo "  build-apk           构建 debug APK"
	@echo "  build-apk-release   构建 release APK"
	@echo "  run-android         构建 APK, 安装并启动"
	@echo "  logcat              查看 Android 日志"
	@echo ""
	@echo "平台支持: Linux / Windows / macOS / Android"

# ============================================================
# Desktop
# ============================================================
.PHONY: run
run: ## 编译并运行 (当前平台)
	cargo run -p $(CRATE_NAME) --features $(FEATURES)

.PHONY: run-release
run-release: ## 编译并运行 (release 模式)
	RUST_LOG=info,zbus=error,ashpd=error,tracing=warn cargo run -p $(CRATE_NAME) --features $(FEATURES) --release

.PHONY: preview
preview: ## 使用 slint-viewer 预览 UI (无需编译 Rust, 支持热重载)
	slint-viewer --auto-reload ui/app.slint

.PHONY: build
build: ## 编译当前平台 debug
	cargo build -p $(CRATE_NAME) --features $(FEATURES)

.PHONY: clean
clean: ## 清理编译产物
	cargo clean -p $(CRATE_NAME)

# ============================================================
# Android (使用 cargo-apk2 构建 APK)
# ============================================================
.PHONY: install-target-android
install-target-android: ## 安装 Android 目标
	rustup target add $(TARGET_ANDROID) 2>/dev/null || true

# Android 预编译 ffmpeg 路径(来自 yehun-slint)
ANDROID_FFMPEG_DIR := $(HOME)/jobs/yehun-slint/yehun-plugin/ffmpeg/build/android
ANDROID_NDK := /data/software/java/androiod-sdk/ndk/28.0.13004108
# bindgen 需要正确的 sysroot 来解析 Android 头文件
BINDGEN_SYSROOT := $(ANDROID_NDK)/toolchains/llvm/prebuilt/linux-x86_64/sysroot
# aaudio 库路径(API 31)
AAUDIO_LIB := $(BINDGEN_SYSROOT)/usr/lib/aarch64-linux-android/31

.PHONY: build-apk
build-apk: install-target-android ## 构建 debug APK
	JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64 \
	FFMPEG_DIR=$(ANDROID_FFMPEG_DIR) \
	PKG_CONFIG_PATH=$(ANDROID_FFMPEG_DIR)/lib/pkgconfig \
	PKG_CONFIG_SYSROOT_DIR=$(ANDROID_FFMPEG_DIR) \
	BINDGEN_EXTRA_CLANG_ARGS=--sysroot=$(BINDGEN_SYSROOT) \
	RUSTFLAGS="-L $(AAUDIO_LIB)" \
	cargo apk2 build -p $(CRATE_NAME) --lib --no-default-features --features android

.PHONY: build-apk-release
build-apk-release: install-target-android ## 构建 release APK
	JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64 \
	FFMPEG_DIR=$(ANDROID_FFMPEG_DIR) \
	PKG_CONFIG_PATH=$(ANDROID_FFMPEG_DIR)/lib/pkgconfig \
	PKG_CONFIG_SYSROOT_DIR=$(ANDROID_FFMPEG_DIR) \
	BINDGEN_EXTRA_CLANG_ARGS=--sysroot=$(BINDGEN_SYSROOT) \
	RUSTFLAGS="-L $(AAUDIO_LIB)" \
	cargo apk2 build -p $(CRATE_NAME) --lib --no-default-features --features android --release

.PHONY: run-android
run-android: install-target-android ## 构建 APK, 安装并启动
	JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64 \
	FFMPEG_DIR=$(ANDROID_FFMPEG_DIR) \
	PKG_CONFIG_PATH=$(ANDROID_FFMPEG_DIR)/lib/pkgconfig \
	PKG_CONFIG_SYSROOT_DIR=$(ANDROID_FFMPEG_DIR) \
	BINDGEN_EXTRA_CLANG_ARGS=--sysroot=$(BINDGEN_SYSROOT) \
	RUSTFLAGS="-L $(AAUDIO_LIB)" \
	cargo apk2 run -p $(CRATE_NAME) --lib --no-default-features --features android --release

.PHONY: logcat
logcat: ## 查看 Android 日志
	adb logcat -s AndroidRuntime $(CRATE_NAME) slint:* *
