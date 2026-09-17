// 第十三篇, Slint开发的第一个音乐播放器 — Desktop 入口
//
// 运行: cargo run -p article-13 --features desktop

#[cfg(feature = "desktop")]
fn main() {
    article_13::desktop_main();
}

#[cfg(not(feature = "desktop"))]
fn main() {
    eprintln!("请使用 cargo apk2 build -p article-13 --no-default-features --features android 构建 Android APK");
}
