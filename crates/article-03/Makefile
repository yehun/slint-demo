# ============================================================
# Article 03: 自定义无头窗口
# 仅支持平台: Linux
# ============================================================

CRATE_NAME   := article-03
BIN_NAME     := article-03

TARGET_LINUX := x86_64-unknown-linux-gnu

.PHONY: help
help:
	@echo "Article 03 — 自定义无头窗口"
	@echo ""
	@echo "用法: make <target>"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}'

.PHONY: check
check:
	cargo check -p $(CRATE_NAME)

.PHONY: build
build:
	cargo build -p $(CRATE_NAME)

.PHONY: build-release
build-release:
	cargo build -p $(CRATE_NAME) --release

.PHONY: clean
clean:
	cargo clean -p $(CRATE_NAME)

.PHONY: run
run:
	cargo run -p $(CRATE_NAME)

.PHONY: build-linux
build-linux:
	cargo build -p $(CRATE_NAME) --target $(TARGET_LINUX) --release
