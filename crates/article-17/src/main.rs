// 第十七篇, Slint OCR 文字识别 — Desktop 入口
//
// 运行: cargo run -p article-17 --features desktop
// 模型: 设置 PP_OCR_MODEL_DIR 指向含 det.onnx/cls.onnx/rec.onnx/keys.txt 的目录,
//       或启动后点“加载模型”手动选择

#[cfg(feature = "desktop")]
fn main() {
    article_17::desktop_main();
}

#[cfg(not(feature = "desktop"))]
fn main() {
    eprintln!("请使用 cargo apk2 build -p article-17 --no-default-features --features android 构建 Android APK");
}
