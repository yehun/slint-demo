#!/usr/bin/env bash
# 下载 LuxTTS 语音克隆权重(三个 ONNX: text_encoder / fm_decoder / vocos)
#
# 用法:
#   ./scripts/fetch-model.sh [目标目录]        # 默认 ~/.local/share/slint-demo/models/lux-tts
#   MODEL_URL=<zip 地址> ./scripts/fetch-model.sh
#
# 仓库不分发模型: fp32 版解压后约 800MB, int8 版约 200MB。
# 模型包是 WinZip AES 加密 zip, 解压密码见下方 PASS(可用 MODEL_ZIP_PASSWORD 覆盖)。

set -euo pipefail

DEST="${1:-$HOME/.local/share/slint-demo/models/lux-tts}"
URL="${MODEL_URL:-https://dt-storage.oss-accelerate.aliyuncs.com/2026080808/C4cV7kY53r/lux-tts-int8.zip}"
PASS="${MODEL_ZIP_PASSWORD:-\$codeai.cn}"

command -v unzip >/dev/null 2>&1 || { echo "需要 unzip: sudo apt install unzip" >&2; exit 1; }
command -v curl >/dev/null 2>&1 || { echo "需要 curl" >&2; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "下载: $URL"
curl -fL --retry 2 -o "$WORK/model.zip" "$URL"

echo "解压到: $DEST"
mkdir -p "$DEST"
unzip -o -q -P "$PASS" "$WORK/model.zip" -d "$DEST"

echo "完成, 目录内容:"
ls -lh "$DEST"
