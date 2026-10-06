// 无界面验证用: 给定模型目录与图片, 跑一次 PP-OCRv4 推理并打印识别结果 + 耗时。
// 仅用于命令行快速验证 OCR 引擎(不依赖 Slint UI)。
//
// 用法:
//   ORT_DYLIB_PATH=/path/to/libonnxruntime.so \
//   cargo run -p slint-ocr --example ocr_cli -- <模型目录> <图片路径>

use slint_ocr::OcrService;

fn main() {
    let model_dir = std::env::args()
        .nth(1)
        .expect("用法: ocr_cli <模型目录> <图片>");
    let image_path = std::env::args()
        .nth(2)
        .expect("用法: ocr_cli <模型目录> <图片>");

    let svc = OcrService::new();
    if !svc.ort_available() {
        eprintln!("✗ onnxruntime 动态库不可用: {:?}", svc.library_error());
        std::process::exit(1);
    }

    if let Err(e) = svc.load_models_from_dir(std::path::Path::new(&model_dir)) {
        eprintln!("✗ 加载模型失败: {e}");
        std::process::exit(1);
    }
    println!("✓ 模型已加载: {model_dir}");

    let img = match image::open(&image_path) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("✗ 打开图片失败: {e}");
            std::process::exit(1);
        }
    };

    match svc.recognize(&img) {
        Ok(result) => {
            println!(
                "✓ 识别完成: {} 行文字, 总 {}ms (det {}ms, rec {}ms)",
                result.lines.len(),
                result.total_ms,
                result.det_ms,
                result.rec_ms
            );
            for (i, line) in result.lines.iter().enumerate() {
                println!(
                    "  {:2} [{:>5.1}%] {}x{}@{},{}  {}",
                    i,
                    line.confidence * 100.0,
                    line.w,
                    line.h,
                    line.x,
                    line.y,
                    line.text
                );
            }
        }
        Err(e) => {
            eprintln!("✗ 识别失败: {e}");
            std::process::exit(1);
        }
    }
}
