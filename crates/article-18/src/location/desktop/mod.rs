// 第十八篇(下), 定位体系 —— 桌面实现(WiFi WPS 逐级兜底)
//
// 桌面三平台的系统定位 API 各有门槛(Linux GeoClue 要桌面 agent 授权,
// Windows WinRT / macOS CoreLocation 要能力清单与授权弹窗), 且无 GPS 模组的
// 机器拿到的多半是城市级结果。本 demo 走 WiFi WPS(参考生产版公共兜底层):
//
//   平台扫 AP(MAC + 强度) → WiFi 定位(Apple → beaconDB, 城市内几十米)
//                          → IP 定位(ip-api.com, 城市级 ~30km)
//
// 平台侧只负责"扫 AP"(实现 WifiScanner): linux 走 NetworkManager(D-Bus),
// windows 走 wlanapi.dll 的 BSS 列表, macos 走 CoreWLAN —— 三家扫描结果
// 都归一成 (MAC, 强度) 列表, 公共层全平台复用。
//
// provider 命名约定(定位回调 → 地图侧按 contains("gps") 判 WGS-84):
// - WiFi 定位(Apple/beaconDB) = "wifi-gps" → 转 GCJ-02;
// - IP 定位 = "ip" → 直用(城市级, 100m 级偏移无意义)。
//
// 线程模型: open() 起 worker 线程立即返回; 有 scanner 时按固定周期刷新
// WiFi 帧(网络环境变了结果跟着动), 无 scanner 时 IP 单发即收工。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use parking_lot::Mutex;

use super::LocationCallback;

mod apple;
mod ip;
mod wifi;

#[cfg(target_os = "linux")]
mod linux_scanner;
#[cfg(target_os = "windows")]
mod windows_scanner;
#[cfg(target_vendor = "apple")]
mod macos_scanner;

use wifi::{wifi_location, WifiScanner};

/// WiFi 帧刷新周期(网络环境变化才会有新结果, 不用太密)。
const WIFI_REFRESH_INTERVAL: Duration = Duration::from_secs(60);

/// 平台 AP 扫描器: 三大桌面各接各的系统 WiFi API, 结果统一交给公共层。
fn make_scanner() -> Option<Arc<dyn WifiScanner>> {
    #[cfg(target_os = "linux")]
    {
        // NM 不在/无总线时, scan 返回 None 自动落到 IP 兜底, 这里恒给扫描器
        Some(Arc::new(linux_scanner::NetworkManagerScanner) as Arc<dyn WifiScanner>)
    }
    #[cfg(target_os = "windows")]
    {
        Some(Arc::new(windows_scanner::WlanBssScanner) as Arc<dyn WifiScanner>)
    }
    #[cfg(target_vendor = "apple")]
    {
        Some(Arc::new(macos_scanner::CoreWlanScanner) as Arc<dyn WifiScanner>)
    }
    // 兜底: 其它未知目标没有扫描器, 落 IP 单发
    #[cfg(not(any(
        target_os = "linux",
        target_os = "windows",
        target_vendor = "apple"
    )))]
    {
        None
    }
}

struct Shared {
    callback: Mutex<Option<LocationCallback>>,
    scanner: Option<Arc<dyn WifiScanner>>,
    stop: AtomicBool,
}

pub struct PlatformLocationService {
    shared: Arc<Shared>,
    running: AtomicBool,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl PlatformLocationService {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared {
                callback: Mutex::new(None),
                scanner: make_scanner(),
                stop: AtomicBool::new(false),
            }),
            running: AtomicBool::new(false),
            worker: Mutex::new(None),
        }
    }

    /// 桌面无系统定位总开关(WiFi/IP 兜底随时可试), 恒报可用。
    pub fn enabled(&self) -> bool {
        true
    }

    /// 开启定位。`callback` 在 worker 线程上被调(内部再回投 UI 线程)。
    pub fn open(&self, callback: LocationCallback) -> bool {
        *self.shared.callback.lock() = Some(callback);
        // 已在跑: 只更新回调, 不重启 worker(幂等, 支持重复 open)
        if self.running.swap(true, Ordering::SeqCst) {
            return true;
        }
        self.shared.stop.store(false, Ordering::SeqCst);
        let shared = self.shared.clone();
        match std::thread::Builder::new()
            .name("desktop-location".into())
            .spawn(move || worker(shared))
        {
            Ok(handle) => {
                *self.worker.lock() = Some(handle);
                true
            }
            Err(e) => {
                eprintln!("[location] 起 worker 失败: {e}");
                self.running.store(false, Ordering::SeqCst);
                *self.shared.callback.lock() = None;
                false
            }
        }
    }

    pub fn close(&self) {
        *self.shared.callback.lock() = None;
        if !self.running.swap(false, Ordering::SeqCst) {
            return;
        }
        self.shared.stop.store(true, Ordering::SeqCst);
        // detach: worker 每 500ms 查一次 stop, 很快自己退出
        self.worker.lock().take();
    }
}

fn worker(shared: Arc<Shared>) {
    let has_scanner = shared.scanner.is_some();
    loop {
        if shared.stop.load(Ordering::SeqCst) {
            break;
        }
        // 逐级兜底: WiFi 定位(准) → IP 定位(粗)。各自失败自然落到下一级。
        let fix = shared
            .scanner
            .as_ref()
            .and_then(|s| wifi_location(s.as_ref()))
            .or_else(ip::ip_location);
        if let Some(location) = fix {
            if shared.stop.load(Ordering::SeqCst) {
                break;
            }
            // Box<dyn Fn> 不可克隆: 持锁直调, 不取出
            let guard = shared.callback.lock();
            if let Some(callback) = guard.as_ref() {
                callback(location);
            }
        } else if !shared.stop.load(Ordering::SeqCst) {
            eprintln!("[location] 定位失败(wifi & ip)");
        }
        // 仅 IP 定位: 单发即收工(IP 地址不变, 结果不会变)
        if !has_scanner {
            break;
        }
        // WiFi 帧: 等待刷新周期, 期间响应 stop
        let step = Duration::from_millis(500);
        let mut waited = Duration::ZERO;
        while waited < WIFI_REFRESH_INTERVAL {
            if shared.stop.load(Ordering::SeqCst) {
                return;
            }
            std::thread::sleep(step);
            waited += step;
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod live_tests {
    use super::*;

    /// 实测整条链: 扫描周边 AP → Apple/beaconDB 定位。
    /// 需要系统 D-Bus + WiFi 网卡 + 联网, 手动跑:
    ///   cargo test -p article-18 --features desktop -- --ignored --nocapture
    #[test]
    #[ignore = "需要系统 D-Bus + WiFi + 联网"]
    fn live_wifi_locate() {
        let Some(scanner) = make_scanner() else {
            panic!("no scanner");
        };
        let aps = scanner.scan().expect("NetworkManager 扫描失败");
        println!("可见 AP: {} 个", aps.len());
        assert!(!aps.is_empty(), "周边应有可见 AP");
        let fix = wifi_location(scanner.as_ref());
        println!("定位结果: {fix:?}");
        let fix = fix.expect("wifi 定位应成功");
        assert!(
            fix.accuracy < 30_000.0,
            "精度应显著优于 IP 城市级(30km), 实际 {}m",
            fix.accuracy
        );
    }
}
