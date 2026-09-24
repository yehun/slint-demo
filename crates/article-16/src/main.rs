// 第十六篇, Slint 语音克隆 — Desktop 入口
//
// 运行: cargo run -p article-16 --features desktop
// 模型: 设置 LUX_TTS_MODEL_DIR, 或启动后点“加载模型”手动选择

#[cfg(feature = "desktop")]
fn main() {
    article_16::desktop_main();
}

#[cfg(not(feature = "desktop"))]
fn main() {
    eprintln!("请使用 cargo apk2 build -p article-16 --no-default-features --features android 构建 Android APK");
}
