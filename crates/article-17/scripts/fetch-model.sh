#!/usr/bin/env bash
# 下载 PP-OCRv4 的 ONNX 模型(det/cls/rec)与字典(keys.txt)到本目录下的 models/pp-ocr。
#
# 模型来源(PaddleOCR 官方, Apache-2.0):
#   - det / rec:  https://huggingface.co/SWHL/RapidOCR  (PP-OCRv4, 已含检测头/识别头)
#   - cls:        https://huggingface.co/SWHL/RapidOCR  (PP-OCRv2 方向分类, 0°/180°)
#   - keys.txt:   PaddleOCR release/2.7 的 ppocr_keys_v1.txt (约 6600 字)
#
# 用法:
#   bash scripts/fetch-model.sh
# 之后运行 demo 时设置:
#   export PP_OCR_MODEL_DIR="$(pwd)/models/pp-ocr"
#   cargo run -p article-17 --features desktop
#
# 也可用文件选择器手动指向本目录; 或用任何含 det.onnx/rec.onnx/keys.txt 的 PP-OCRv4 模型目录。

set -euo pipefail

cd "$(dirname "$0")/.."
OUT="models/pp-ocr"
mkdir -p "$OUT"

BASE="https://huggingface.co/SWHL/RapidOCR/resolve/main"

dl() {
    local url="$1" local_path="$2"
    if [[ -s "$local_path" ]]; then
        echo "✓ 已存在, 跳过: $local_path"
    else
        echo "↓ 下载: $url"
        curl -L --fail --retry 3 -o "$local_path" "$url"
    fi
}

dl "$BASE/PP-OCRv4/ch_PP-OCRv4_det_infer.onnx" "$OUT/det.onnx"
dl "$BASE/PP-OCRv4/ch_PP-OCRv4_rec_infer.onnx" "$OUT/rec.onnx"
dl "$BASE/PP-OCRv1/ch_ppocr_mobile_v2.0_cls_infer.onnx" "$OUT/cls.onnx"
dl "https://raw.githubusercontent.com/PaddlePaddle/PaddleOCR/release/2.7/ppocr/utils/ppocr_keys_v1.txt" "$OUT/keys.txt"

echo
echo "模型已就绪: $OUT"
echo "  det.onnx  rec.onnx  cls.onnx  keys.txt"
echo
echo "运行 demo 前请设置:"
echo "  export PP_OCR_MODEL_DIR=\"$(pwd)/models/pp-ocr\""
