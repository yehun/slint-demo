// 第十八篇(下), 定位体系 —— Native WiFi API AP 扫描器(Windows, wlanapi.dll)。
//
// `WlanGetNetworkBssList` 枚举周边 BSS(每个 AP 一条, `lRssi` 为 dBm),
// 换算成 0-100 强度后交给公共层(Apple → beaconDB → IP 逐级兜底)。
// 全程 unsafe; 失败一律返回 None, 由公共层落到 IP 兜底。

use windows::Win32::Foundation::HANDLE;
use windows::Win32::NetworkManagement::WiFi::{
    WlanCloseHandle, WlanEnumInterfaces, WlanFreeMemory, WlanGetNetworkBssList, WlanOpenHandle,
    WLAN_BSS_LIST, WLAN_INTERFACE_INFO_LIST,
};

use super::wifi::WifiScanner;

/// Native WiFi API 的 BSS 类型常量 dot11_BSS_type_infrastructure(仅要基础
/// 设施型 AP; windows 0.56 未导出该常量, 按 wlanapi.h 的枚举值 1 直接构造)。
const DOT11_BSS_TYPE_INFRASTRUCTURE: windows::Win32::NetworkManagement::WiFi::DOT11_BSS_TYPE =
    windows::Win32::NetworkManagement::WiFi::DOT11_BSS_TYPE(1);

/// wlanapi BSS 扫描器: 枚举所有无线接口的周边 BSS。
pub struct WlanBssScanner;

impl WifiScanner for WlanBssScanner {
    fn scan(&self) -> Option<Vec<(String, u8)>> {
        unsafe { wlan_bss_list() }
    }
}

unsafe fn wlan_bss_list() -> Option<Vec<(String, u8)>> {
    let mut negotiated = 0u32;
    let mut handle = HANDLE(0);
    // 2 = WLAN_API_VERSION_2(Vista+)
    if WlanOpenHandle(2, None, &mut negotiated, &mut handle) != 0 {
        return None;
    }
    let mut result = None;
    let mut ifaces: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
    if WlanEnumInterfaces(handle, None, &mut ifaces) == 0 && !ifaces.is_null() {
        let list = &*ifaces;
        let mut seen = std::collections::HashSet::new();
        let mut aps: Vec<(String, u8)> = Vec::new();
        for i in 0..list.dwNumberOfItems as usize {
            // C 风格柔性数组: dwNumberOfItems 个 WLAN_INTERFACE_INFO 连续排布
            let info = &list.InterfaceInfo[i];
            let mut bss: *mut WLAN_BSS_LIST = std::ptr::null_mut();
            // ssid=None = 列出全部 SSID; security_enabled 只影响查询细节, true 即可
            if WlanGetNetworkBssList(
                handle,
                &info.InterfaceGuid,
                None,
                DOT11_BSS_TYPE_INFRASTRUCTURE,
                true,
                None,
                &mut bss,
            ) == 0
                && !bss.is_null()
            {
                let entries = &*bss;
                for j in 0..entries.dwNumberOfItems as usize {
                    let entry = &entries.wlanBssEntries[j];
                    let bssid = entry.dot11Bssid;
                    if bssid == [0u8; 6] {
                        continue;
                    }
                    let mac = format!(
                        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                        bssid[0], bssid[1], bssid[2], bssid[3], bssid[4], bssid[5]
                    );
                    if seen.insert(mac.clone()) {
                        // RSSI(dBm, 负值) → 0-100 强度: 常用线性近似
                        let quality = ((entry.lRssi + 100) * 2).clamp(0, 100) as u8;
                        aps.push((mac, quality));
                    }
                }
                WlanFreeMemory(bss.cast());
            }
        }
        result = Some(aps);
    }
    if !ifaces.is_null() {
        WlanFreeMemory(ifaces.cast());
    }
    WlanCloseHandle(handle, None);
    result
}
