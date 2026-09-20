// 第十四篇, Slint开发的第一个视频播放器 — Desktop 入口
//
// 运行: cargo run -p article-14 --features desktop

#[cfg(feature = "desktop")]
fn main() {
    article_14::desktop_main();
}

#[cfg(not(feature = "desktop"))]
fn main() {
    eprintln!("请使用 cargo apk2 build -p article-14 --no-default-features --features android 构建 Android APK");
}
