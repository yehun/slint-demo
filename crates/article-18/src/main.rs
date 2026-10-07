// 第十八篇, Slint 地图显示 — Desktop 入口
//
// 运行: cargo run -p article-18 --features desktop
// 瓦片: 高德公共栅格瓦片(GCJ-02, 免 API key), 首次运行需联网; 之后走磁盘缓存

#[cfg(feature = "desktop")]
fn main() {
    article_18::desktop_main();
}

#[cfg(not(feature = "desktop"))]
fn main() {
    eprintln!("请使用 cargo apk2 build -p article-18 --no-default-features --features android 构建 Android APK");
}
