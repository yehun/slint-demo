//! WGS-84 ↔ GCJ-02(火星坐标) 互转 —— 高德/腾讯底图坐标系。
//!
//! 国内法定地图数据(高德/腾讯)统一打在 GCJ-02 上; 而 GPS 输出的是 WGS-84。
//! 两系同点相差约 100~700 米, 不转换会出现"定位漂移几个街区"。
//! 本 demo 内部所有 Web-Mercator 投影都用 GCJ-02; 公共坐标(默认中心 / 落点
//! 标记 / 屏幕读数)统一以用户直觉的 WGS-84 呈现, 在边界处做转换。
//!
//! 算法是业界通用拟合公式(非精确椭球变换), 精度亚米级, 纯数学无依赖。
//! 中国境外偏移无定义, 原样返回。

use std::f64::consts::PI;

/// 克拉索夫斯基椭球长半轴(米)。
const A: f64 = 6_378_245.0;
/// 第一偏心率平方。
const EE: f64 = 0.006_693_421_622_965_943_23;

/// 是否在中国境外(粗判): 境外坐标不做偏移, 原样返回。
fn out_of_china(lat: f64, lon: f64) -> bool {
    !(72.004..=137.834_7).contains(&lon) || !(0.829_3..=55.827_1).contains(&lat)
}

/// WGS-84 → GCJ-02, 返回 (lat, lon)。
pub fn wgs84_to_gcj02(lat: f64, lon: f64) -> (f64, f64) {
    if out_of_china(lat, lon) {
        return (lat, lon);
    }
    let (d_lon, d_lat) = delta(lon, lat);
    (lat + d_lat, lon + d_lon)
}

/// GCJ-02 → WGS-84, 返回 (lat, lon)。
///
/// 正向公式的反函数无解析解, 用两轮迭代逼近(残差亚米级), 足够地图显示。
pub fn gcj02_to_wgs84(lat: f64, lon: f64) -> (f64, f64) {
    if out_of_china(lat, lon) {
        return (lat, lon);
    }
    let (mut w_lat, mut w_lon) = (lat, lon);
    for _ in 0..2 {
        let (g_lat, g_lon) = wgs84_to_gcj02(w_lat, w_lon);
        w_lat += lat - g_lat;
        w_lon += lon - g_lon;
    }
    (w_lat, w_lon)
}

/// (经度, 纬度) 处的 GCJ 偏移量(度)。
fn delta(lon: f64, lat: f64) -> (f64, f64) {
    let mut d_lat = transform_lat(lon - 105.0, lat - 35.0);
    let mut d_lon = transform_lon(lon - 105.0, lat - 35.0);
    let rad_lat = lat / 180.0 * PI;
    let magic = 1.0 - EE * rad_lat.sin() * rad_lat.sin();
    let sqrt_magic = magic.sqrt();
    d_lat = (d_lat * 180.0) / ((A * (1.0 - EE)) / (magic * sqrt_magic) * PI);
    d_lon = (d_lon * 180.0) / (A / sqrt_magic * rad_lat.cos() * PI);
    (d_lon, d_lat)
}

fn transform_lat(x: f64, y: f64) -> f64 {
    let mut ret = -100.0 + 2.0 * x + 3.0 * y + 0.2 * y * y + 0.1 * x * y + 0.2 * x.abs().sqrt();
    ret += (20.0 * (6.0 * x * PI).sin() + 20.0 * (2.0 * x * PI).sin()) * 2.0 / 3.0;
    ret += (20.0 * (y * PI).sin() + 40.0 * (y / 3.0 * PI).sin()) * 2.0 / 3.0;
    ret += (160.0 * (y / 12.0 * PI).sin() + 320.0 * (y * PI / 30.0).sin()) * 2.0 / 3.0;
    ret
}

fn transform_lon(x: f64, y: f64) -> f64 {
    let mut ret = 300.0 + x + 2.0 * y + 0.1 * x * x + 0.1 * x * y + 0.1 * x.abs().sqrt();
    ret += (20.0 * (6.0 * x * PI).sin() + 20.0 * (2.0 * x * PI).sin()) * 2.0 / 3.0;
    ret += (20.0 * (x * PI).sin() + 40.0 * (x / 3.0 * PI).sin()) * 2.0 / 3.0;
    ret += (150.0 * (x / 12.0 * PI).sin() + 300.0 * (x / 30.0 * PI).sin()) * 2.0 / 3.0;
    ret
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 米 → 度(纬度方向), 断言偏移量级用。
    fn meters_to_deg(m: f64) -> f64 {
        m / 111_320.0
    }

    #[test]
    fn offset_in_china_is_hundred_meter_scale() {
        // 北京/上海/广州: GCJ 偏移应在 100~700 米量级
        let cases = [(39.907_5, 116.391_3), (31.230_4, 121.473_7), (23.129_1, 113.264_4)];
        for (lat, lon) in cases {
            let (glat, glon) = wgs84_to_gcj02(lat, lon);
            let d_lon = (glon - lon) * lat.to_radians().cos();
            let d_lat = glat - lat;
            let meters = (d_lon * 111_320.0).hypot(d_lat * 111_320.0);
            assert!(
                meters > 50.0 && meters < 800.0,
                "({lat},{lon}) 偏移 {meters}m 应在百米量级"
            );
            // 国内 GCJ 相对 WGS 偏东南(d_lon>0 是普遍特征, 宽松验证)
            assert!(glon > lon, "d_lon 应为正: {d_lon}");
        }
    }

    #[test]
    fn roundtrip_within_two_meters() {
        let cases = [
            (39.907_5, 116.391_3),
            (31.230_4, 121.473_7),
            (23.129_1, 113.264_4),
            (30.659_5, 104.065_7),
        ];
        for (lat, lon) in cases {
            let (glat, glon) = wgs84_to_gcj02(lat, lon);
            let (wlat, wlon) = gcj02_to_wgs84(glat, glon);
            let err_m = ((wlat - lat) * 111_320.0).hypot((wlon - lon) * 111_320.0);
            assert!(err_m < 2.0, "({lat},{lon}) 往返误差 {err_m}m");
        }
    }

    #[test]
    fn outside_china_passthrough() {
        // 纽约 / 伦敦: 境外不做偏移
        for (lat, lon) in [(40.712_8, -74.006), (51.507_2, -0.127_6)] {
            let (glat, glon) = wgs84_to_gcj02(lat, lon);
            assert_eq!(glat, lat);
            assert_eq!(glon, lon);
        }
    }

    #[test]
    fn offset_is_smooth_nearby() {
        // 相邻两点的"偏移量本身"应平滑(防公式打错符号导致跳变)
        let (lat, lon) = (39.907_5, 116.391_3);
        let a = delta(lon, lat);
        let b = delta(lon + 0.001, lat);
        assert!((b.0 - a.0).abs() < meters_to_deg(5.0));
        assert!((b.1 - a.1).abs() < meters_to_deg(5.0));
    }
}
