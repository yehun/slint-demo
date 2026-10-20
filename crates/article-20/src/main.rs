// 第二十篇, 证件照排版 — Desktop 入口
//
// 运行: cargo run -p article-20 --features desktop

#[cfg(feature = "desktop")]
fn main() {
    article_20::desktop_main();
}

#[cfg(not(feature = "desktop"))]
fn main() {
    eprintln!("请使用 cargo apk2 build -p article-20 --no-default-features --features android 构建 Android APK");
}
