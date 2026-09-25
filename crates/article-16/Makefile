# ============================================================
# Article 16: Slint 语音克隆
# 支持平台: Linux / Windows / macOS / Android
# ============================================================

CRATE_NAME   := article-16
BIN_NAME     := article-16
DESKTOP_FEATURES := desktop
ANDROID_FEATURES := android

TARGET_LINUX     := x86_64-unknown-linux-gnu
TARGET_WINDOWS   := x86_64-pc-windows-gnu
TARGET_MACOS     := x86_64-apple-darwin
TARGET_MACOS_ARM := aarch64-apple-darwin
TARGET_ANDROID   := aarch64-linux-android

ANDROID_PACKAGE := com.example.article16
TARGET_DIR := $(shell cargo metadata --format-version 1 --no-deps 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin)['target_directory'])" 2>/dev/null || echo "../../target")
APK_PATH := $(TARGET_DIR)/debug/apk/$(BIN_NAME).apk

# Java: D8 dex 转换需要 Java 11+
export JAVA_HOME := /usr/lib/jvm/java-21-openjdk-amd64
export ANDROID_HOME := /data/software/java/androiod-sdk
export ANDROID_NDK_HOME := /data/software/java/androiod-sdk/ndk/28.0.13004108

.PHONY: help
help: ## 显示所有可用目标
	@echo "Article 16 — Slint 语音克隆"
	@echo ""
	@grep -E '^[a-zA-Z0-9_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}'

.PHONY: model
model: ## 下载 LuxTTS(ZipVoice) 模型权重 (仓库不分发)
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
cli: ## CLI 验证管线 (不启界面)
	LUX_TTS_MODEL_DIR=$(HOME)/.local/share/slint-demo/models/lux-tts \
		cargo run --release -p slint-tts --example clone -- \
		$(REF) "$(TEXT)" output/clone.wav --model int8

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
