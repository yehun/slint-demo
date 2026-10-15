// 第十八篇(下), 定位体系 —— CoreWLAN AP 扫描器(macOS)。
//
// CoreWLAN 扫周边 BSS(CWNetwork 的 `bssid`/`rssi`), 换算成 0-100 强度后
// 交给公共层(Apple → beaconDB → IP 逐级兜底)。
//
// 注意: macOS Sonoma 起 CoreWLAN 扫描会触发系统定位授权; 被拒时扫描
// 返回空, 自然落到 IP 定位(城市级), 功能不中断。

use objc2_core_wlan::CWWiFiClient;

use super::wifi::WifiScanner;

/// CoreWLAN 扫描器: 枚举所有无线接口的周边网络。
pub struct CoreWlanScanner;

impl WifiScanner for CoreWlanScanner {
    fn scan(&self) -> Option<Vec<(String, u8)>> {
        let client = unsafe { CWWiFiClient::sharedWiFiClient() };
        let interfaces = unsafe { client.interfaces() }?;
        let mut seen = std::collections::HashSet::new();
        let mut aps: Vec<(String, u8)> = Vec::new();
        for interface in interfaces.iter() {
            // ssid=None = 列出全部网络; 扫描是同步阻塞调用, 可能在秒级
            let networks = unsafe { interface.scanForNetworksWithSSID_error(None) }.ok()?;
            for network in networks.iter() {
                let Some(bssid) = (unsafe { network.bssid() }) else {
                    continue;
                };
                let mac = bssid.to_string();
                if mac.is_empty() || !seen.insert(mac.clone()) {
                    continue;
                }
                // RSSI(dBm, 负值) → 0-100 强度: 常用线性近似
                let rssi = unsafe { network.rssiValue() } as i32;
                let quality = ((rssi + 100) * 2).clamp(0, 100) as u8;
                aps.push((mac, quality));
            }
        }
        Some(aps)
    }
}
