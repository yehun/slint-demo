# ============================================================
# Article 02: 地基与三端初始化
# 支持平台: Linux / Windows / macOS / Android
# ============================================================

# 配置
CRATE_NAME   := article-02
BIN_NAME     := article-02
FEATURES     := desktop

# Rust 目标平台
TARGET_LINUX   := x86_64-unknown-linux-gnu
TARGET_WINDOWS := x86_64-pc-windows-gnu
TARGET_MACOS   := x86_64-apple-darwin
TARGET_MACOS_ARM := aarch64-apple-darwin
TARGET_ANDROID := aarch64-linux-android

# Android 配置
ANDROID_PACKAGE := com.yehun.article_02
ANDROID_LIB     := libarticle_02.so
# cargo metadata 动态获取 target 目录 (兼容自定义 target-dir)
TARGET_DIR := $(shell cargo metadata --format-version 1 --no-deps 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin)['target_directory'])" 2>/dev/null || echo "../target")
APK_PATH := $(TARGET_DIR)/debug/apk/$(BIN_NAME).apk
APK_RELEASE_PATH := $(TARGET_DIR)/release/apk/$(BIN_NAME).apk

# Java: D8 dex 编译器需要 Java 11+, 系统默认 Java 8 不兼容
export JAVA_HOME := /usr/lib/jvm/java-21-openjdk-amd64
export ANDROID_HOME := /data/software/java/androiod-sdk
export ANDROID_NDK_HOME := /data/software/java/androiod-sdk/ndk/28.0.13004108

# ============================================================
# 默认目标
# ============================================================
.PHONY: help
help: ## 显示所有可用目标
	@echo "Article 02 — 地基与三端初始化"
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

.PHONY: run-windows
run-windows: install-target-windows ## 交叉编译 Windows
	@echo "交叉编译 Windows (需要 mingw-w64: sudo apt install mingw-w64)"
	cargo build -p $(CRATE_NAME) --features $(FEATURES) --target $(TARGET_WINDOWS) --release
	@echo "产物: target/$(TARGET_WINDOWS)/release/$(BIN_NAME).exe"

.PHONY: build-windows
build-windows: run-windows ## 同 run-windows

# ============================================================
# macOS (需要 osxcross 或 macOS 主机)
# ============================================================
.PHONY: install-target-macos
install-target-macos: ## 安装 macOS 目标
	rustup target add $(TARGET_MACOS) 2>/dev/null || true
	rustup target add $(TARGET_MACOS_ARM) 2>/dev/null || true

.PHONY: run-macos
run-macos: install-target-macos ## 交叉编译 macOS (x86_64)
	@echo "交叉编译 macOS (需要 osxcross 工具链)"
	cargo build -p $(CRATE_NAME) --features $(FEATURES) --target $(TARGET_MACOS) --release
	@echo "产物: target/$(TARGET_MACOS)/release/$(BIN_NAME)"

.PHONY: run-macos-arm
run-macos-arm: install-target-macos ## 交叉编译 macOS (Apple Silicon)
	cargo build -p $(CRATE_NAME) --features $(FEATURES) --target $(TARGET_MACOS_ARM) --release
	@echo "产物: target/$(TARGET_MACOS_ARM)/release/$(BIN_NAME)"

.PHONY: build-macos
build-macos: run-macos ## 同 run-macos

# ============================================================
# Android (使用 cargo-apk2 构建 APK)
#   需要: Android NDK + JAVA_HOME (Java 11+)
# ============================================================
.PHONY: install-target-android
install-target-android: ## 安装 Android 目标
	rustup target add $(TARGET_ANDROID) 2>/dev/null || true

.PHONY: build-apk
build-apk: install-target-android ## 构建 debug APK
	cargo apk2 build -p $(CRATE_NAME) --lib --features $(FEATURES)

.PHONY: build-apk-release
build-apk-release: install-target-android ## 构建 release APK
	cargo apk2 build -p $(CRATE_NAME) --lib --features $(FEATURES) --release

.PHONY: install-apk
install-apk: ## 安装 debug APK 到连接的设备
	@if [ ! -f "$(APK_PATH)" ]; then \
		echo "❌ APK 不存在: $(APK_PATH)"; \
		echo "   先运行 make build-apk"; \
		exit 1; \
	fi
	adb install -r "$(APK_PATH)"

.PHONY: install-apk-release
install-apk-release: ## 安装 release APK 到连接的设备
	@if [ ! -f "$(APK_RELEASE_PATH)" ]; then \
		echo "❌ Release APK 不存在: $(APK_RELEASE_PATH)"; \
		echo "   先运行 make build-apk-release"; \
		exit 1; \
	fi
	adb install -r "$(APK_RELEASE_PATH)"

.PHONY: run-android
run-android: build-apk ## 构建 APK, 安装并启动
	@echo "安装 APK..."
	@adb install -r "$(APK_PATH)" || (echo "❌ adb 无设备连接"; exit 1)
	@echo "启动 $(ANDROID_PACKAGE)..."
	adb shell am start -n $(ANDROID_PACKAGE)/.MainActivity

.PHONY: run-android-release
run-android-release: build-apk-release ## 构建 release APK, 安装并启动
	@echo "安装 Release APK..."
	@adb install -r "$(APK_RELEASE_PATH)" || (echo "❌ adb 无设备连接"; exit 1)
	@echo "启动 $(ANDROID_PACKAGE)..."
	adb shell am start -n $(ANDROID_PACKAGE)/.MainActivity

.PHONY: check-android
check-android: install-target-android ## 仅检查 Android 编译 (不链接)
	cargo apk2 check -p $(CRATE_NAME) --features $(FEATURES)

.PHONY: logcat
logcat: ## 查看 Android 日志
	adb logcat -s AndroidRuntime $(BIN_NAME) slint:* *

# ============================================================
# WASM (使用 wasm-pack 构建)
#   wasm-pack 0.15.0 内置 wasm-bindgen 0.2.127, 与项目版本一致
# ============================================================
WASM_TARGET := wasm32-unknown-unknown
WASM_OUT_DIR := $(TARGET_DIR)/wasm
WASM_PKG_DIR := $(WASM_OUT_DIR)/pkg

.PHONY: install-target-wasm
install-target-wasm: ## 安装 WASM 目标
	rustup target add $(WASM_TARGET) 2>/dev/null || true

.PHONY: build-wasm
build-wasm: install-target-wasm ## 构建 WASM (使用 wasm-pack)
	@rm -rf $(WASM_PKG_DIR)
# 	wasm-pack build --target web --out-dir $(WASM_PKG_DIR) --release --no-opt --no-typescript --features wasm
	wasm-pack build --release --features=wasm --target web --out-dir $(WASM_PKG_DIR) --no-opt
	@cp wasm/index.html $(WASM_PKG_DIR)/index.html
	@echo "✅ WASM 构建完成: $(WASM_PKG_DIR)"

.PHONY: run-wasm
run-wasm: build-wasm ## 构建并启动 WASM 本地服务器
	@echo "启动 WASM 开发服务器 (http://127.0.0.1:8080)..."
	@fuser -k 8080/tcp 2>/dev/null || true
	@cd $(WASM_PKG_DIR) && python3 -m http.server 8080
	@sleep 1
	@echo "已启动, 浏览器打开 http://127.0.0.1:8080"

.PHONY: check-wasm
check-wasm: install-target-wasm ## 仅检查 WASM 编译
	cargo check -p $(CRATE_NAME) --target $(WASM_TARGET) --features wasm

# ============================================================
# 一键安装所有目标
# ============================================================
.PHONY: install-targets
install-targets: install-target-linux install-target-windows install-target-macos install-target-android ## 安装所有交叉编译目标
	@echo "所有目标平台已就绪"
	rustup target list --installed
