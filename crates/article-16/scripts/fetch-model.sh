#!/usr/bin/env bash
# 下载 LuxTTS / ZipVoice 蒸馏 flow-matching 语音克隆权重(本地零样本 TTS)。
#
# 用法:
#   ./scripts/fetch-model.sh                            # 默认 ~/.local/share/slint-demo/models/lux-tts
#   ./scripts/fetch-model.sh /path/to/models/lux-tts    # 指定目录
#   LUX_TTS_MODEL_DIR=/path/to/models/lux-tts ./scripts/fetch-model.sh
#
# 仓库不分发模型权重。本脚本拉取可直接用的 ONNX 骨干:
#   - text_encoder.onnx / text_encoder_int8.onnx   文本+参考音频 → 条件向量(fp32 / int8)
#   - fm_decoder.onnx  / fm_decoder_int8.onnx      flow-matching 4 步去噪解码(int8 体积/内存更小, CPU 更快)
#   - vocos.onnx                                 48kHz 双路 Vocos 声码器
#   - tokens.txt                                 音素/子词词表(随权重一同提供)
#
# 来源:
#   text_encoder / fm_decoder / tokens.txt  -> https://huggingface.co/YatharthS/LuxTTS
#   vocos.onnx                          -> https://huggingface.co/ProgCat/luxtts-onnx
#
# 免转写说明(本 demo 的核心特性):
#   说话人音色来自参考音频的 mel 条件(encode_prompt), 与文本无关, 因此合成时
#   无需提供参考音频的转写文本。占位 prompt 只给出"长度提示"(约 5 字/秒 × 2 token/字,
#   token id=3 即空格), 并在 0.95 阈值的触发下做二分速度补偿。详见 crate README。
#
# 精度选择: 运行时 article-16 优先加载 int8(text_encoder_int8.onnx 等), 失败回退 fp32。
#   两个变体都下载最稳妥; 若只想最小化体积, 可只保留 fp32 三件套 + vocos + tokens。

set -euo pipefail

DEST="${1:-${LUX_TTS_MODEL_DIR:-$HOME/.local/share/slint-demo/models/lux-tts}}"

# 两个权重源(均为 Apache-2.0 的 LuxTTS ONNX 导出)
TE_FM_BASE="https://huggingface.co/YatharthS/LuxTTS/resolve/main"
VOCOS_BASE="https://huggingface.co/ProgCat/luxtts-onnx/resolve/main"

command -v curl >/dev/null 2>&1 || { echo "需要 curl" >&2; exit 1; }

mkdir -p "$DEST"

# fp32 核心件套(完整可运行)
CORE=(
  "$TE_FM_BASE/text_encoder.onnx"
  "$TE_FM_BASE/fm_decoder.onnx"
  "$TE_FM_BASE/tokens.txt"
  "$VOCOS_BASE/vocos.onnx"
)
# int8 变体(article-16 优先加载, 失败回退 fp32)
INT8=(
  "$TE_FM_BASE/text_encoder_int8.onnx"
  "$TE_FM_BASE/fm_decoder_int8.onnx"
)

download() {
  local url="$1"
  local name
  name="$(basename "$url")"
  if [ -s "$DEST/$name" ]; then
    echo "已存在, 跳过: $name"
  else
    echo "下载: $name -> $DEST/"
    curl -fL --retry 2 -o "$DEST/$name" "$url"
  fi
}

echo "==> 下载 fp32 核心件套"
for u in "${CORE[@]}"; do download "$u"; done

echo "==> 下载 int8 变体(可选, 失败不影响 fp32 回退)"
for u in "${INT8[@]}"; do download "$u"; done

echo
echo "模型目录($DEST)当前内容:"
ls -lh "$DEST"
echo
echo "可用环境变量: LUX_TTS_MODEL_DIR / CLONE_TTS_MODEL_DIR 指定上述目录, 或把目录放到"
echo "  ~/.local/share/slint-demo/models/lux-tts  或  <article-16>/models/lux-tts"
echo "运行: make -C crates/article-16 run-cli REF=<参考音频.wav> TEXT='要合成的文本'"
