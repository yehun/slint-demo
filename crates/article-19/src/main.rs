// 第十九篇, 噪音测量 — Desktop 入口
//
// 运行: cargo run -p article-19 --features desktop

#[cfg(feature = "desktop")]
fn main() {
    article_19::desktop_main();
}

#[cfg(not(feature = "desktop"))]
fn main() {
    eprintln!("请使用 cargo apk2 build -p article-19 --no-default-features --features android 构建 Android APK");
}
