# 第十七篇, Slint OCR 文字识别

用 **Slint** 做一个本地 OCR 演示：一张图片 → PP-OCRv4 三件套（文本检测 / 方向分类 / 文字识别）
全部在 **CPU + onnxruntime** 上本地推理，**不联网、不上传图片**。

## 架构

```
crates/slint-ocr      OCR 引擎(纯 Rust, 直接依赖 ort, 无外部共享依赖)
   ├─ ort_ext.rs      onnxruntime 动态库探测 + 会话构建(桌面 load-dynamic / Android 编译期链接)
   ├─ det.rs          DB 检测后处理(二值化 / 膨胀 / 连通域 / PCA 旋转框)
   ├─ rec.rs          CTC 解码 + 字典映射
   ├─ preprocess.rs   图像 resize / RGB→NCHW 归一化(对齐 RapidOCR 3.0.0)
   └─ lib.rs          OcrService: 懒加载模型 + 串行推理 + 阅读顺序排序

crates/article-17     Slint 演示 UI(深色卡片式)
   ├─ ui/app.slint     模型状态 / 图片选择 / 识别 / 检测框叠加预览 + 识别文本
   └─ src/             模型加载 / 图片选择 / 识别(工作线程) / 框绘制 / 保存文本
```

UI 主线程只管渲染；**模型加载与识别都在后台线程**，结果经 `invoke_from_event_loop`
写回 Slint 属性。检测框由 Rust 画在原图上，整张预览图回传给 `Image` 元素显示。

## 运行

```bash
# 1. 拉取模型(约 30MB, 见 scripts/fetch-model.sh)
bash scripts/fetch-model.sh
export PP_OCR_MODEL_DIR="$(pwd)/models/pp-ocr"

# 2. 运行(需本机有 onnxruntime 动态库, 见下方“依赖”)
cargo run -p article-17 --features desktop
```

也可不设置环境变量，启动后点「加载模型」用文件选择器手动指向含
`det.onnx` / `rec.onnx` / `keys.txt`（cls 可选）的目录。

## 依赖

- **onnxruntime 动态库**（`libonnxruntime.so` / `.dll` / `.dylib`）：运行期通过
  `ORT_DYLIB_PATH` 或程序目录下 `./lib` 自动探测。桌面端 `ort` 以 `load-dynamic`
  方式加载，无需编译期链接。
- **模型**：PP-OCRv4 的 `det/cls/rec` 三个 ONNX + `ppocr_keys_v1.txt`（脚本自动下载）。

### Android

Android 下 `ort` 必须编译期链接（`bionic` 的 `dlsym` 解析不了版本化符号）。需自备
预编译的 `libonnxruntime.so`（aarch64），放到构建链路能找到的位置，再走
`cargo apk2 build -p article-17 --no-default-features --features android`。

## 已知边界

- 检测框为轴对齐外接框（已做 PCA 主方向透视矫正裁剪，提升倾斜文本识别率）。
- 识别字典为中文 PP-OCRv4 默认字典（约 6600 字）；如需英文/多语言，替换 `keys.txt`
  与对应 `rec.onnx` 即可（脚本中注释了其它语言模型来源）。
