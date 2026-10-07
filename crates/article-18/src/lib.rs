// 第十八篇, Slint 地图显示
//
// 核心知识点:
// 1. 地图 = 一堆 256×256 的栅格瓦片, 按 Web Mercator 投影坐标平铺
// 2. Flickable 当"视口": 瓦片按世界像素摆进去, 拖动就是平移, 滚轮/按钮改变缩放级
// 3. 缩放时以光标(或视口中心)为锚点, 锚点处的地理坐标缩放前后不动
// 4. 点击地图把"世界像素"反投影成经纬度, 即落点标记; 默认中心 / 回中同理
// 5. 瓦片异步拉取(高德地图 GCJ-02, 免 key) + 磁盘缓存, 到货后经事件循环写回 Slint
//
// 入口:
//   desktop_main —— Windows / Linux / macOS
//   android_main —— Android (cargo apk2)

slint::include_modules!();

mod gcj02;
mod map;

use std::path::PathBuf;

use slint::ComponentHandle;

/// 瓦片磁盘缓存目录: 优先 $MAP_CACHE_DIR, 否则落到系统临时目录。
fn cache_dir() -> PathBuf {
    if let Ok(d) = std::env::var("MAP_CACHE_DIR") {
        return PathBuf::from(d);
    }
    let mut d = std::env::temp_dir();
    d.push("slint-demo-map-tiles");
    d
}

/// Android 真机缓存目录: 应用私有 files/cache 下。
/// std::env::temp_dir() 在 Android 是 /tmp —— 不可写, 落盘静默失败,
/// 缓存形同虚设, 每次缩放都全量走网络(捏合卡顿的主因)。
#[cfg(target_os = "android")]
fn android_cache_dir(app: &slint::android::AndroidApp) -> Option<PathBuf> {
    app.internal_data_path().map(|p| p.join("cache").join("map-tiles"))
}

pub fn desktop_main() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .try_init();

    // reqwest 的异步 future 需要 tokio reactor 在场, 进入 runtime 作用域。
    let rt = tokio::runtime::Runtime::new().expect("创建 tokio runtime 失败");
    let _guard = rt.enter();

    let app = MainWindow::new().expect("创建窗口失败");
    let app_rc = map::bind(app, cache_dir());
    // 显式设初始视口: 组件的 init 回调早于 bind 触发, view-ready 会丢失
    app_rc.start();
    app_rc.main.run().expect("运行窗口失败");
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    // 崩溃先落到 stderr(会被 logcat 捕获), 否则 native 闪退没有任何线索。
    std::panic::set_hook(Box::new(|info| {
        eprintln!("RUST PANIC: {info}");
    }));

    // 应用私有缓存目录(init 会消费 app, 先取路径; AndroidApp: Clone)
    let cache = android_cache_dir(&app).unwrap_or_else(cache_dir);

    slint::android::init(app).expect("init android");

    // reqwest / tokio 异步拉瓦片需要 tokio reactor 在场 —— desktop_main 已 enter,
    // Android 这里也必须建并 enter, 否则 send() 因"无 runtime"报错、spawn_blocking
    // 直接 panic。与桌面走同一套写法。
    let rt = tokio::runtime::Runtime::new().expect("创建 tokio runtime 失败");
    let _guard = rt.enter();

    let window = MainWindow::new().expect("创建窗口失败");
    let app_rc = map::bind(window, cache);

    // 中文显示: 软件渲染器没有跨字体族的字形回退, 默认字体 + 系统回退链会挑到
    // 覆盖不全的 CJK 字体, 简体-only 字形(缩/级/图/绪)成方块。这里直接指定
    // 设备自带的全量简体字体族(小米设备: NotoSansCJK-Regular.ttc 含 SC face;
    // fontique 扫 /system/fonts 注册, family 名须与其登记一致)。
    app_rc
        .main
        .global::<MapModel>()
        .set_font_family("Noto Sans CJK SC".into());

    app_rc.start();
    app_rc.main.show().expect("显示窗口失败");
    slint::run_event_loop().expect("运行窗口失败");
}
