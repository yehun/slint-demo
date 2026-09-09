# ============================================================
# Article 08: SystemTrayIcon / 系统托盘图标
# 在系统托盘中添加图标和菜单, 打造常驻后台的跨平台桌面应用
# 支持平台: Linux / Windows / macOS (桌面端 only)
# ============================================================

# 配置
CRATE_NAME   := article-08
BIN_NAME     := article-08
FEATURES     := desktop

# Rust 目标平台
TARGET_LINUX   := x86_64-unknown-linux-gnu
TARGET_WINDOWS := x86_64-pc-windows-gnu
TARGET_MACOS   := x86_64-apple-darwin
TARGET_MACOS_ARM := aarch64-apple-darwin

# cargo metadata 动态获取 target 目录
TARGET_DIR := $(shell cargo metadata --format-version 1 --no-deps 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin)['target_directory'])" 2>/dev/null || echo "../target")

# ============================================================
# 默认目标
# ============================================================
.PHONY: help
help: ## 显示所有可用目标
	@echo "Article 08 — SystemTrayIcon / 系统托盘图标"
	@echo ""
	@echo "用法: make <target>"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}'
	@echo ""
	@echo "平台支持: Linux / Windows / macOS (桌面端)"

# ============================================================
# 通用
# ============================================================
.PHONY: run
run: ## 编译并运行 (当前平台)
	cargo run -p $(CRATE_NAME) --features $(FEATURES)

.PHONY: preview
preview: ## 使用 slint-viewer 预览 UI (无需编译 Rust, 支持热重载)
	slint-viewer --auto-reload ui/app.slint

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
# 一键安装所有目标
# ============================================================
.PHONY: install-targets
install-targets: install-target-linux install-target-windows install-target-macos ## 安装所有交叉编译目标
	@echo "所有目标平台已就绪"
	rustup target list --installed
