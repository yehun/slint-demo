# ============================================================
# Article 10: Slint 剪贴板读写
# 跨平台文本复制粘贴, 桌面端 arboard + Android ClipboardManager
# 支持平台: Linux / Windows / macOS / Android
# ============================================================

# 配置
CRATE_NAME   := article-10
BIN_NAME     := article-10
FEATURES     := desktop

# Rust 目标平台
TARGET_LINUX   := x86_64-unknown-linux-gnu
TARGET_WINDOWS := x86_64-pc-windows-gnu
TARGET_MACOS   := x86_64-apple-darwin
TARGET_MACOS_ARM := aarch64-apple-darwin
TARGET_ANDROID := aarch64-linux-android

# Android 配置
ANDROID_PACKAGE := com.example.article10
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
	@echo "Article 10 — Slint 剪贴板读写"
	@echo ""
	@echo "用法: make <target>"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-22s\033[0m %s\n", $1, $2}'
	@echo ""
	@echo "平台支持: Linux / Windows / macOS / Android"

# ============================================================
# 通用
# ============================================================
.PHONY: run
run: ## 编译并运行 (当前平台)
	cargo run -p $(CRATE_NAME) --features $(FEATURES)

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

.PHONY: build-apk
build-apk: install-target-android ## 构建 debug APK
	JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64 cargo apk2 build -p $(CRATE_NAME) --lib --no-default-features --features android

.PHONY: build-apk-release
build-apk-release: install-target-android ## 构建 release APK
	JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64 cargo apk2 build -p $(CRATE_NAME) --lib --no-default-features --features android --release

.PHONY: run-android
run-android: install-target-android ## 构建 APK, 安装并启动
	cargo apk2 run -p $(CRATE_NAME) --lib --no-default-features --features android --release

.PHONY: logcat
logcat: ## 查看 Android 日志
	adb logcat -s AndroidRuntime $(CRATE_NAME) slint:* *
