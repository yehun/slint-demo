// 第十八篇(下), 定位体系 —— Apple 定位服务(gs-loc.apple.com/clls/wloc)。
//
// locationd 协议逆向(社区广泛使用), iPhone 众包数据, 国内城市覆盖极好,
// 免密钥。请求信号最强的几个 AP, 命中任一个即返回周边 AP 集群坐标
// (数百个), 取质心、以平均偏距估精度。
//
// 坐标为 WGS-84, provider = "wifi-gps"(含 "gps" → 地图侧转 GCJ-02)。

use std::time::Duration;

use crate::location::Location;

const APPLE_WLOC_URL: &str = "https://gs-loc.apple.com/clls/wloc";
const APPLE_USER_AGENT: &str = "locationd/1753.17 CFNetwork/711.1.12 Darwin/14.0.0";
/// 提交给 Apple 的 AP 数(任一命中即返回整片集群, 多查几个提高命中概率;
/// 请求体单字节长度前缀, 总长须 < 256, 8 个 AP 约 160B)
const APPLE_QUERY_APS: usize = 8;

/// Apple WPS 定位: `aps` 为 (MAC, 强度 0-100) 列表(信号最强在前更佳)。
pub fn apple_locate(aps: &[(String, u8)]) -> Option<Location> {
    let bssids: Vec<String> = aps
        .iter()
        .take(APPLE_QUERY_APS)
        .map(|(mac, _)| mac.to_lowercase())
        .collect();
    let payload = pb_encode_apple_wloc(&bssids);
    let mut body = Vec::with_capacity(payload.len() + 26);
    body.extend_from_slice(b"\x00\x01\x00\x05en_US\x00\x13com.apple.locationd\x00\x0a8.1.12B411\x00\x00\x00\x01\x00\x00\x00");
    // 单字节长度前缀(payload 已限制 AP 数, 不会超 255)
    body.push(payload.len() as u8);
    body.extend_from_slice(&payload);

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(6))
        .build()
        .ok()?;
    let content = client
        .post(APPLE_WLOC_URL)
        .header("User-Agent", APPLE_USER_AGENT)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "*/*")
        .header("Accept-Language", "en-us")
        .body(body)
        .send()
        .ok()?
        .bytes()
        .ok()?;
    if content.len() <= 10 {
        return None;
    }
    // 前 10 字节是包装头, 其后才是 AppleWLoc protobuf
    let locations = pb_parse_locations(&content[10..]);
    if locations.is_empty() {
        return None;
    }
    let n = locations.len() as f64;
    let latitude = locations.iter().map(|x| x.0).sum::<f64>() / n;
    let longitude = locations.iter().map(|x| x.1).sum::<f64>() / n;
    let spread = locations
        .iter()
        .map(|(la, lo)| super::wifi::haversine_m(latitude, longitude, *la, *lo))
        .sum::<f64>()
        / n;
    log::info!("apple wloc: {} AP locations, spread {spread:.0}m", locations.len());
    Some(Location {
        provider: "wifi-gps".into(),
        latitude,
        longitude,
        accuracy: (spread * 2.0).clamp(20.0, 100_000.0) as f32,
    })
}

// ---------------------------------------------------------------------------
// Apple WLOC 协议编解码(AppleWLoc.proto 逆向定义, 手写 protobuf 编解码避免
// 引入 prost/protobuf 依赖 —— 消息只有 bssid 字符串和 lat/lon 两个 varint)。
// ---------------------------------------------------------------------------

/// AppleWLoc 请求: { unknown_value0=0, wifi_devices=[{bssid}], return_single_result=0 }。
/// return_single_result=0 是关键: 命中任一 AP 时返回周边整片 AP 集群。
fn pb_encode_apple_wloc(bssids: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    // field1 unknown_value0, varint 0
    out.extend_from_slice(&[0x08, 0x00]);
    for bssid in bssids {
        // repeated field2 (WifiDevice): tag 0x12 + len + 子消息
        // 子消息: field1 bssid(string): tag 0x0a + len + bytes
        let mut dev = Vec::with_capacity(bssid.len() + 2);
        dev.push(0x0a);
        dev.push(bssid.len() as u8);
        dev.extend_from_slice(bssid.as_bytes());
        out.push(0x12);
        out.push(dev.len() as u8);
        out.extend_from_slice(&dev);
    }
    // field4 return_single_result = 0
    out.extend_from_slice(&[0x20, 0x00]);
    out
}

/// 解析 AppleWLoc 响应里的 repeated WifiDevice.location, 返回 (lat, lon) 度。
fn pb_parse_locations(data: &[u8]) -> Vec<(f64, f64)> {
    let mut results = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let Ok((tag, ni)) = pb_read_varint(data, i) else {
            break;
        };
        i = ni;
        match tag & 0x7 {
            0 => {
                // varint 字段(unknown_value0 / return_single_result): 跳过
                let Ok((_, ni)) = pb_read_varint(data, i) else {
                    break;
                };
                i = ni;
            }
            2 => {
                let Ok((len, ni)) = pb_read_varint(data, i) else {
                    break;
                };
                let (len, ni) = (len as usize, ni);
                if ni + len > data.len() {
                    break;
                }
                if tag >> 3 == 2 {
                    if let Some(loc) = pb_parse_wifi_device(&data[ni..ni + len]) {
                        results.push(loc);
                    }
                }
                i = ni + len;
            }
            5 => i += 4,
            1 => i += 8,
            _ => break,
        }
    }
    results
}

/// WifiDevice: field1 bssid(string), field2 location(子消息)。
fn pb_parse_wifi_device(dev: &[u8]) -> Option<(f64, f64)> {
    let mut latitude = None;
    let mut longitude = None;
    let mut i = 0;
    while i < dev.len() {
        let (tag, ni) = pb_read_varint(dev, i).ok()?;
        i = ni;
        match tag & 0x7 {
            2 => {
                let (len, ni) = pb_read_varint(dev, i).ok()?;
                let (len, ni) = (len as usize, ni);
                if ni + len > dev.len() {
                    return None;
                }
                if tag >> 3 == 2 {
                    let (la, lo) = pb_parse_location(&dev[ni..ni + len])?;
                    latitude = Some(la);
                    longitude = Some(lo);
                }
                i = ni + len;
            }
            0 => {
                let (_, ni) = pb_read_varint(dev, i).ok()?;
                i = ni;
            }
            5 => i += 4,
            1 => i += 8,
            _ => return None,
        }
    }
    Some((latitude?, longitude?))
}

/// Location: field1 latitude(int64, 值 ×1e-8), field2 longitude。
/// 负数 varint 是 10 字节二补码, 读成 u64 再转 i64 即还原。
/// (-180, -180) 是"未收录"哨兵, 过滤掉。
fn pb_parse_location(sub: &[u8]) -> Option<(f64, f64)> {
    let mut latitude = None;
    let mut longitude = None;
    let mut i = 0;
    while i < sub.len() {
        let (tag, ni) = pb_read_varint(sub, i).ok()?;
        let (value, ni) = pb_read_varint(sub, ni).ok()?;
        i = ni;
        match tag >> 3 {
            1 => latitude = Some(value as i64 as f64 * 1e-8),
            2 => longitude = Some(value as i64 as f64 * 1e-8),
            _ => {}
        }
    }
    let (la, lo) = (latitude?, longitude?);
    if (la + 180.0).abs() < f64::EPSILON && (lo + 180.0).abs() < f64::EPSILON {
        return None;
    }
    Some((la, lo))
}

fn pb_read_varint(data: &[u8], mut i: usize) -> Result<(u64, usize), ()> {
    let mut result: u64 = 0;
    let mut shift = 0;
    loop {
        let byte = *data.get(i).ok_or(())?;
        i += 1;
        result |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Ok((result, i));
        }
        shift += 7;
        if shift > 63 {
            return Err(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 请求编码与参考实现(python 验证过的线上协议)逐字节一致
    #[test]
    fn encode_matches_reference() {
        let payload = pb_encode_apple_wloc(&["b4:5d:50:94:39:b3".into()]);
        let expected: Vec<u8> = [
            b"\x08\x00",                     // unknown_value0 = 0
            b"\x12\x13",                     // WifiDevice, len 19
            b"\x0a\x11",                     //   bssid, len 17
            b"b4:5d:50:94:39:b3".as_slice(), //   17 字节
            b"\x20\x00",                     // return_single_result = 0
        ]
        .concat();
        assert_eq!(payload, expected);
    }

    /// 响应解析: 含负纬度(10 字节二补码 varint)与 -180/-180 哨兵过滤
    #[test]
    fn parse_locations_with_negative_and_sentinel() {
        // 手工构造 AppleWLoc: WifiDevice(bssid, Location(-34.5, 113.75)) +
        // WifiDevice(Location(-180, -180) 哨兵)
        let mut data = Vec::new();
        let mut dev1 = Vec::new();
        dev1.push(0x0a);
        dev1.push(17);
        dev1.extend_from_slice(b"aa:bb:cc:dd:ee:ff".as_slice()); // 17 字节
        let mut loc1 = Vec::new();
        loc1.push(0x08);
        loc1.extend_from_slice(&pb_varint_bytes((-34.5 / 1e-8) as i64 as u64));
        loc1.push(0x10);
        loc1.extend_from_slice(&pb_varint_bytes((113.75 / 1e-8) as u64));
        dev1.push(0x12);
        dev1.push(loc1.len() as u8);
        dev1.extend_from_slice(&loc1);
        data.push(0x12);
        data.push(dev1.len() as u8);
        data.extend_from_slice(&dev1);
        let mut dev2 = Vec::new();
        let mut loc2 = Vec::new();
        loc2.push(0x08);
        loc2.extend_from_slice(&pb_varint_bytes((-180.0 / 1e-8) as i64 as u64));
        loc2.push(0x10);
        loc2.extend_from_slice(&pb_varint_bytes((-180.0 / 1e-8) as i64 as u64));
        dev2.push(0x12);
        dev2.push(loc2.len() as u8);
        dev2.extend_from_slice(&loc2);
        data.push(0x12);
        data.push(dev2.len() as u8);
        data.extend_from_slice(&dev2);

        let locations = pb_parse_locations(&data);
        assert_eq!(locations.len(), 1);
        assert!((locations[0].0 - (-34.5)).abs() < 1e-6);
        assert!((locations[0].1 - 113.75).abs() < 1e-6);
    }

    fn pb_varint_bytes(mut v: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let byte = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(byte);
                break;
            }
            out.push(byte | 0x80);
        }
        out
    }
}
