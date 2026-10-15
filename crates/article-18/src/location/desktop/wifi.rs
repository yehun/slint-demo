// 第十八篇(下), 定位体系 —— WiFi WPS 定位(平台扫 AP, 这里管查询)
//
// 两家定位服务坐标都是 WGS-84, provider = "wifi-gps"(含 "gps" → 地图侧转 GCJ-02):
// - Apple gs-loc.apple.com: iPhone 众包数据, 国内城市覆盖极好, 免密钥;
// - beaconDB: 社区维护的 MLS 继任者(ichnaea geolocate 协议), 覆盖稀一些。
// 逐级回退: 先问 Apple, 不中再问 beaconDB。

use std::time::Duration;

use crate::location::Location;

use super::apple::apple_locate;

/// 提交给查询服务的 AP 数上限(信号最强的 N 个)。
pub const WIFI_MAX_APS: usize = 12;

/// 平台 AP 扫描器: 各平台实现一次, 公共层全平台复用。
pub trait WifiScanner: Send + Sync {
    /// 扫描可见 AP: (MAC "aa:bb:..", 强度 0-100), 无需排序(这里统一处理);
    /// None = 扫描不可用(无无线网卡/服务不在)。
    fn scan(&self) -> Option<Vec<(String, u8)>>;
}

/// WiFi 定位: 扫 AP → Apple → beaconDB, 逐级回退。
pub fn wifi_location(scanner: &dyn WifiScanner) -> Option<Location> {
    let mut aps = scanner.scan()?;
    if aps.is_empty() {
        return None;
    }
    log::info!("wifi locate: {} APs visible", aps.len());
    aps.sort_by(|a, b| b.1.cmp(&a.1));
    aps.truncate(WIFI_MAX_APS);
    apple_locate(&aps).or_else(|| beacondb_locate(&aps))
}

/// beaconDB WPS 定位(ichnaea geolocate 协议, 社区维护的 MLS 继任者)。
/// 精度取决于库中 AP 覆盖(城市内常见几十米; 无覆盖则 404, 返回 None)。
pub fn beacondb_locate(aps: &[(String, u8)]) -> Option<Location> {
    let points: Vec<serde_json::Value> = aps
        .iter()
        .map(|(mac, strength)| {
            // 强度是 0-100 百分比, ichnaea 要 dBm: 线性近似换算
            serde_json::json!({
                "macAddress": mac.to_lowercase(),
                "signalStrength": (*strength as f32 / 2.0) - 100.0,
            })
        })
        .collect();
    let body = serde_json::json!({ "wifiAccessPoints": points });
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(6))
        .build()
        .ok()?;
    // 404 = AP 不在库中(响应体可能为空), 一律按无结果处理
    let resp = client
        .post("https://api.beacondb.net/v2/geolocate")
        .json(&body)
        .send()
        .ok()?
        .json::<serde_json::Value>()
        .ok()?;
    let latitude = resp["location"]["lat"].as_f64()?;
    let longitude = resp["location"]["lng"].as_f64()?;
    let accuracy = resp["accuracy"].as_f64().unwrap_or(1_000.0);
    log::info!("wifi location via beacondb: accuracy {accuracy}m ({} APs)", aps.len());
    Some(Location {
        provider: "wifi-gps".into(),
        latitude,
        longitude,
        accuracy: accuracy.clamp(0.0, 100_000.0) as f32,
    })
}

/// 两点球面距离(米), 简化 haversine。
pub fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6_371_000.0;
    let (d_lat, d_lon) = ((lat2 - lat1).to_radians(), (lon2 - lon1).to_radians());
    let a = (d_lat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lon / 2.0).sin().powi(2);
    2.0 * R * a.sqrt()
}
