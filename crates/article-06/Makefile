# ============================================================
# Article 06: 基于Slint官方Material建立Android主题体系
# 支持平台: Linux / Windows / macOS / Android
# ============================================================

# 配置
CRATE_NAME   := article-06
BIN_NAME     := article-06
FEATURES     := desktop

# Rust 目标平台
TARGET_LINUX   := x86_64-unknown-linux-gnu
TARGET_WINDOWS := x86_64-pc-windows-gnu
TARGET_MACOS   := x86_64-apple-darwin
TARGET_MACOS_ARM := aarch64-apple-darwin
TARGET_ANDROID := aarch64-linux-android

# Android 配置
ANDROID_PACKAGE := com.example.article_06
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
help: ## 显示所有可用目标
	@echo "Article 06 — Slint 官方 Material 主题体系"
	@echo ""
	@echo "用法: make <target>"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}'
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
	slint-viewer -L material=../../assets/themes/material/material.slint --style material --auto-reload ui/app.slint

.PHONY: check
check: ## 快速检查 (cargo check)
	cargo check -p $(CRATE_NAME) --features $(FEATURES)

.PHONY: build
build: ## 编译当前平台 debug
	cargo build -p $(CRATE_NAME) --features $(FEATURES)

.PHONY: build-release
build-release: ## 编译当前平台 release
	cargo build -p $(CRATE_NAME) --features $(FEATURES) --release

.PHONY: clean
clean: ## 清理编译产物
	cargo clean -p $(CRATE_NAME)

# ============================================================
# Linux
# ============================================================
.PHONY: install-target-linux
install-target-linux: ## 安装 Linux 目标
	rustup target add $(TARGET_LINUX) 2>/dev/null || true

.PHONY: run-linux
run-linux: ## 编译并运行 (Linux)
	cargo run -p $(CRATE_NAME) --features $(FEATURES) --target $(TARGET_LINUX)

.PHONY: build-linux
build-linux: install-target-linux ## 交叉编译 Linux
	cargo build -p $(CRATE_NAME) --features $(FEATURES) --target $(TARGET_LINUX) --release

# ============================================================
# Windows (需要 mingw-w64)
# ============================================================
.PHONY: install-target-windows
install-target-windows: ## 安装 Windows 目标
	rustup target add $(TARGET_WINDOWS) 2>/dev/null || true

.PHONY: build-windows
build-windows: install-target-windows ## 交叉编译 Windows
	@echo "交叉编译 Windows (需要 mingw-w64: sudo apt install mingw-w64)"
	cargo build -p $(CRATE_NAME) --features $(FEATURES) --target $(TARGET_WINDOWS) --release
	@echo "产物: target/$(TARGET_WINDOWS)/release/$(BIN_NAME).exe"

# ============================================================
# macOS (需要 osxcross 或 macOS 主机)
# ============================================================
.PHONY: install-target-macos
install-target-macos: ## 安装 macOS 目标
	rustup target add $(TARGET_MACOS) 2>/dev/null || true
	rustup target add $(TARGET_MACOS_ARM) 2>/dev/null || true

.PHONY: build-macos
build-macos: install-target-macos ## 交叉编译 macOS (x86_64)
	@echo "交叉编译 macOS (需要 osxcross 工具链)"
	cargo build -p $(CRATE_NAME) --features $(FEATURES) --target $(TARGET_MACOS) --release
	@echo "产物: target/$(TARGET_MACOS)/release/$(BIN_NAME)"

.PHONY: build-macos-arm
build-macos-arm: install-target-macos ## 交叉编译 macOS (Apple Silicon)
	cargo build -p $(CRATE_NAME) --features $(FEATURES) --target $(TARGET_MACOS_ARM) --release
	@echo "产物: target/$(TARGET_MACOS_ARM)/release/$(BIN_NAME)"

# ============================================================
# Android (使用 cargo-apk2 构建 APK)
#   需要: Android NDK + JAVA_HOME (Java 11+)
# ============================================================
.PHONY: install-target-android
install-target-android: ## 安装 Android 目标
	rustup target add $(TARGET_ANDROID) 2>/dev/null || true

.PHONY: build-apk
build-apk: install-target-android ## 构建 debug APK
	JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64 cargo apk2 build -p $(CRATE_NAME) --lib --features android

.PHONY: build-apk-release
build-apk-release: install-target-android ## 构建 release APK
	JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64 cargo apk2 build -p $(CRATE_NAME) --lib --features android --release

.PHONY: install-apk
install-apk: ## 安装 debug APK 到连接的设备
	@if [ ! -f "$(APK_PATH)" ]; then \
		echo "❌ APK 不存在: $(APK_PATH)"; \
		echo "   先运行 make build-apk"; \
		exit 1; \
	fi
	adb install -r "$(APK_PATH)"

.PHONY: run-android
run-android: install-target-android ## 构建 APK, 安装并启动
	cargo apk2 run -p $(CRATE_NAME) --lib --features android

.PHONY: logcat
logcat: ## 查看 Android 日志
	adb logcat -s AndroidRuntime $(BIN_NAME) slint:* *

# ============================================================
# 一键安装所有目标
# ============================================================
.PHONY: install-targets
install-targets: install-target-linux install-target-windows install-target-macos install-target-android ## 安装所有交叉编译目标
	@echo "所有目标平台已就绪"
	rustup target list --installed
