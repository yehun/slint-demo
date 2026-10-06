# ============================================================
# Article 17: Slint 本地 OCR (PP-OCRv4, onnxruntime)
# 支持平台: Linux / Windows / macOS / Android
# ============================================================

CRATE_NAME   := article-17
BIN_NAME     := article-17
DESKTOP_FEATURES := desktop
ANDROID_FEATURES := android

TARGET_LINUX     := x86_64-unknown-linux-gnu
TARGET_WINDOWS   := x86_64-pc-windows-gnu
TARGET_MACOS     := x86_64-apple-darwin
TARGET_MACOS_ARM := aarch64-apple-darwin
TARGET_ANDROID   := aarch64-linux-android

ANDROID_PACKAGE := com.example.article17
TARGET_DIR := $(shell cargo metadata --format-version 1 --no-deps 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin)['target_directory'])" 2>/dev/null || echo "../../target")
APK_PATH := $(TARGET_DIR)/debug/apk/$(BIN_NAME).apk

# onnxruntime 动态库路径 (ort 2.0.0-rc.13 对应 onnxruntime 1.28.0)
ORT_DYLIB ?= /home/yehun/.local/lib/libonnxruntime.so
# CLI 验证用的模型目录与测试图片 (可被 make cli DIR=... IMG=... 覆盖)
MODEL_DIR ?= $(HOME)/.local/share/slint-demo/models/pp-ocr
IMG ?= output/ocr_test.png

# Java: D8 dex 转换需要 Java 11+
export JAVA_HOME := /usr/lib/jvm/java-21-openjdk-amd64
export ANDROID_HOME := /data/software/java/androiod-sdk
export ANDROID_NDK_HOME := /data/software/java/androiod-sdk/ndk/28.0.13004108

.PHONY: help
help: ## 显示所有可用目标
	@echo "Article 17 — Slint 本地 OCR (PP-OCRv4)"
	@echo ""
	@grep -E '^[a-zA-Z0-9_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}'

.PHONY: model
model: ## 下载 PP-OCRv4 模型 (det/cls/rec) 与字典 keys.txt
	./scripts/fetch-model.sh

.PHONY: check
check: ## 快速检查
	cargo check -p $(CRATE_NAME) --features $(DESKTOP_FEATURES)

.PHONY: build
build: ## 编译 debug
	cargo build -p $(CRATE_NAME) --features $(DESKTOP_FEATURES)

.PHONY: build-release
build-release: ## 编译 release (推理必须 release, debug 慢 10 倍以上)
	cargo build -p $(CRATE_NAME) --features $(DESKTOP_FEATURES) --release

.PHONY: run-linux
run-linux: ## 编译并运行 (Linux)
	cargo run -p $(CRATE_NAME) --features $(DESKTOP_FEATURES) --release --target $(TARGET_LINUX)

.PHONY: build-windows
build-windows: ## 交叉编译 Windows (需要 mingw-w64)
	cargo build -p $(CRATE_NAME) --features $(DESKTOP_FEATURES) --target $(TARGET_WINDOWS) --release

.PHONY: build-macos
build-macos: ## 交叉编译 macOS (需要 osxcross)
	cargo build -p $(CRATE_NAME) --features $(DESKTOP_FEATURES) --target $(TARGET_MACOS) --release

.PHONY: cli
cli: ## CLI 验证管线 (不启界面, 输出识别行)
	ORT_DYLIB_PATH=$(ORT_DYLIB) \
		cargo run --release -p slint-ocr --example ocr_cli -- \
		$(MODEL_DIR) $(IMG)

.PHONY: build-apk
build-apk: ## 构建 debug APK (需自备 libonnxruntime)
	rustup target add $(TARGET_ANDROID) 2>/dev/null || true
	cargo apk2 build -p $(CRATE_NAME) --lib --no-default-features --features $(ANDROID_FEATURES)

.PHONY: install-apk
install-apk: ## 安装 APK 到设备
	adb install -r "$(APK_PATH)"

.PHONY: run-android
run-android: build-apk install-apk ## 构建 + 安装 + 启动
	adb shell am start -n $(ANDROID_PACKAGE)/.MainActivity

.PHONY: logcat
logcat: ## 看 Android 日志
	adb logcat -s AndroidRuntime $(BIN_NAME) slint:* *

.PHONY: clean
clean: ## 清理本 crate 产物
	cargo clean -p $(CRATE_NAME)
