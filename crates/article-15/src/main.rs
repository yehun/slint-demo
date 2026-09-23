// 第十五篇, Slint 国际化(i18n) — Desktop 入口
//
// 运行: cargo run -p article-15 --features desktop

#[cfg(feature = "desktop")]
fn main() {
    article_15::desktop_main();
}

#[cfg(not(feature = "desktop"))]
fn main() {
    eprintln!("请使用 cargo apk2 build -p article-15 --no-default-features --features android 构建 Android APK");
}
