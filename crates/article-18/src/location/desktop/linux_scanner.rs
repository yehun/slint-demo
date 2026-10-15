// 第十八篇(下), 定位体系 —— NetworkManager AP 扫描器(Linux, D-Bus)。
//
// 经系统总线问 NetworkManager 要所有无线接口的可见 AP 列表
// (BSSID + 信号强度 0-100), 接到公共 WifiScanner 接口上。纯 Rust zbus,
// 不引入系统库依赖; blocking API 在后台 worker 线程里跑, 不碰 Slint。

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedObjectPath;

use super::wifi::WifiScanner;

const NM_DEST: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";

/// NetworkManager AP 扫描器(每次扫描独立建系统总线连接, 简单可靠)。
pub struct NetworkManagerScanner;

impl WifiScanner for NetworkManagerScanner {
    fn scan(&self) -> Option<Vec<(String, u8)>> {
        nm_access_points()
    }
}

/// 经 NetworkManager 枚举可见 AP(去重; 排序/截断由公共层统一做)。
fn nm_access_points() -> Option<Vec<(String, u8)>> {
    let conn = Connection::system().ok()?;
    let nm = Proxy::new(&conn, NM_DEST, NM_PATH, "org.freedesktop.NetworkManager").ok()?;
    let devices: Vec<OwnedObjectPath> = nm.call("GetAllDevices", &()).ok()?;
    let mut seen = std::collections::HashSet::new();
    let mut aps: Vec<(String, u8)> = Vec::new();
    for device in devices {
        // 非无线设备调 Wireless 接口会报错, 跳过即可
        let Ok(device) = Proxy::new(
            &conn,
            NM_DEST,
            device,
            "org.freedesktop.NetworkManager.Device.Wireless",
        ) else {
            continue;
        };
        let Ok(points): Result<Vec<OwnedObjectPath>, _> = device.call("GetAccessPoints", &())
        else {
            continue;
        };
        for point in points {
            let Ok(ap) = Proxy::new(
                &conn,
                NM_DEST,
                point,
                "org.freedesktop.NetworkManager.AccessPoint",
            ) else {
                continue;
            };
            let Ok(mac) = ap.get_property::<String>("HwAddress") else {
                continue;
            };
            let Ok(strength) = ap.get_property::<u8>("Strength") else {
                continue;
            };
            if seen.insert(mac.clone()) {
                aps.push((mac, strength));
            }
        }
    }
    Some(aps)
}
