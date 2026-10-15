// 第十八篇(下), 定位体系 —— IP 定位兜底(城市级粗定位, 一次性)。
//
// provider = "ip", 不含 "gps" —— 地图侧直用坐标不转系,
// 与 Android 网络定位的坐标系处理惯例一致。

use std::time::Duration;

use crate::location::Location;

/// IP 定位精度按城市级估: 30km(真值未知, 只用于精度圈展示)。
pub const IP_ACCURACY_METERS: f32 = 30_000.0;

/// 免密钥 IP 定位(ip-api.com, http)。
pub fn ip_location() -> Option<Location> {
    let (latitude, longitude) = fetch()?;
    Some(Location {
        provider: "ip".into(),
        latitude,
        longitude,
        accuracy: IP_ACCURACY_METERS,
    })
}

fn fetch() -> Option<(f64, f64)> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(6))
        .build()
        .ok()?;
    let value: serde_json::Value = client
        .get("http://ip-api.com/json/?fields=status,lat,lon")
        .send()
        .ok()?
        .json()
        .ok()?;
    if value["status"] == "success" {
        Some((value["lat"].as_f64()?, value["lon"].as_f64()?))
    } else {
        None
    }
}
