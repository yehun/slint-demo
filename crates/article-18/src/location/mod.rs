// 第十八篇(下), 定位体系 —— 多平台收口层
//
// 设计(脱胎于生产版定位插件的架构):
// 1. 统一接口收口: Slint/地图侧只认 `Location` + `PlatformLocationService`,
//    各平台"怎么拿到坐标"被封装在各自实现里 —— Android 走系统 LocationManager
//    (JNI), 桌面走 WiFi WPS 逐级兜底(扫 AP → Apple/beaconDB → IP 定位)。
// 2. provider 命名判坐标系: 瓦片源是高德(GCJ-02), 而 GPS/浏览器定位输出
//    WGS-84 —— 约定 provider 名含 "gps" 即视为 WGS-84, 由地图侧转 GCJ-02;
//    网络/IP 定位在城市级精度下偏移无意义, 直用不转。Android 系统定位的
//    provider 名恰好就是 "gps"/"network"/"passive", 天然满足约定。
// 3. 回调线程模型: 定位回调发生在 Java 定位线程(Android)/worker 线程(桌面),
//    而 Slint 非 Send —— 回调里只做一件事: 经 `slint::invoke_from_event_loop`
//    把"有新定位"抛回 UI 线程, 地图状态全部在 UI 线程更新。

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
pub use android::PlatformLocationService;
// android_main 接线用(AndroidApp 留存 + 应用类缓存)
#[cfg(target_os = "android")]
pub use android::{cache_classes as platform_cache_classes, init as platform_init};

#[cfg(not(target_os = "android"))]
mod desktop;
#[cfg(not(target_os = "android"))]
pub use desktop::PlatformLocationService;

/// 一次定位结果。
///
/// `provider` 命名约定: 含 "gps" = WGS-84 坐标(需转 GCJ-02 才能贴高德瓦片),
/// 否则 = 坐标直用(城市级网络/IP 定位, 100m 级偏移无意义)。
#[derive(Debug, Clone)]
pub struct Location {
    pub provider: String,
    pub latitude: f64,
    pub longitude: f64,
    /// 水平精度: 米(半径)。0 = 未知。
    pub accuracy: f32,
}

/// 定位结果回调(UI 无关, 在定位线程上被调)。
pub type LocationCallback = Box<dyn Fn(Location) + Send + Sync>;

/// provider 命名 → UI 读数文案。
pub fn provider_label(provider: &str) -> String {
    match provider {
        "gps" => "GPS".into(),
        "network" => "网络定位".into(),
        "passive" => "被动定位".into(),
        "ip" => "IP 定位".into(),
        other => format!("定位({other})"),
    }
}

/// 精度读数: 未知(0) 显示 "-", 否则 ±N m / ±N km。
pub fn accuracy_label(accuracy: f32) -> String {
    if accuracy <= 0.0 {
        "-".into()
    } else if accuracy >= 1000.0 {
        format!("±{:.0} 公里", accuracy / 1000.0)
    } else {
        format!("±{:.0} 米", accuracy)
    }
}
