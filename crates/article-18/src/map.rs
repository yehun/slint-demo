// 第十八篇，Slint 地图显示 —— 地图引擎与 Slint 绑定层
//
// 坐标系(Web Mercator 投影 + GCJ-02 火星坐标):
//   - 世界像素: 缩放级 z 下整张世界地图是 256·2^z 像素的正方形
//   - 经纬度 → 世界像素: lon_to_x / lat_to_y
//   - 世界像素 → 经纬度: x_to_lon / y_to_lat
//   - 瓦片: 256×256, 索引 (z, x, y)
//
// 高德瓦片打在 GCJ-02 坐标系上, 而 GPS / 用户直觉是 WGS-84。两者同点相差
// 约 100~700 米, 不转换会"漂移几个街区"。本引擎规则:
//   - 内部所有投影数学都用 GCJ-02(瓦片就是 GCJ-02 的)
//   - 公共坐标(默认中心 / 落点 / 屏幕读数)以 WGS-84 呈现, 在边界处转换
//     · set_center(lat,lon): WGS-84 → GCJ-02 再投影
//     · 反投影 / 落点: GCJ-02 → WGS-84 再存储与显示
//
// Flickable 视口模型:
//   - offset_x/y = 视口左上角对应的"世界像素"(屏幕坐标 screen = world - offset)
//   - Flickable.content-x = -offset_x, 瓦片按世界像素摆进去, 自然随图滚动
//   - 滚轮缩放以光标为锚点: 锚点处的地理坐标缩放前后不动(zoom_to 公式)
//   - 点击地图: 触点世界像素反投影成经纬度, 即"落点"标记
//
// 平移留边 / 高缩放级精度问题(世界边长 > f32 整数精度)在生产版 yehun-slint
// 用"窗口化坐标"解决; 本 demo 把缩放级限制在 1–18, 日常够用。

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use slint::{ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};

use crate::gcj02::{gcj02_to_wgs84, wgs84_to_gcj02};
use crate::{MainWindow, MapModel, MapTile};

const TILE_SIZE: f64 = 256.0;
pub const MIN_ZOOM: u32 = 1;
pub const MAX_ZOOM: u32 = 18;

// 默认视口中心: 北京天安门(WGS-84, 即 GPS 读数)。set_center 内部会转 GCJ-02。
const DEFAULT_CENTER: (f64, f64) = (39.908_7, 116.397_5);
const DEFAULT_ZOOM: u32 = 12;
// 已离开视野但仍保留在内存/磁盘里的瓦片圈数(避免频繁淘汰)
const KEEP_CACHED_TILES: isize = 12;

// ---------- 墨卡托投影(对 GCJ-02 经纬度同样成立) ----------
pub fn world_size(z: u32) -> f64 {
    TILE_SIZE * (1u64 << z) as f64
}
pub fn lon_to_x(lon: f64, z: u32) -> f64 {
    (lon / 360.0 + 0.5) * world_size(z)
}
pub fn lat_to_y(lat: f64, z: u32) -> f64 {
    let n = world_size(z);
    let lat_rad = lat * std::f64::consts::PI / 180.0;
    (0.5 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / (2.0 * std::f64::consts::PI)) * n
}
pub fn x_to_lon(x: f64, z: u32) -> f64 {
    360.0 * (x / world_size(z) - 0.5)
}
pub fn y_to_lat(y: f64, z: u32) -> f64 {
    let n = world_size(z);
    let lat_rad = (0.5 - y / n) * 2.0 * std::f64::consts::PI;
    90.0 - 360.0 * (-lat_rad).exp().atan() / std::f64::consts::PI
}

// 高德栅格瓦片 URL(GCJ-02, 国内直连免 key)。
// 子域 webrd01~04 / webst01~04 轮换分摊并发。
// style=8(webrd)=中文注记完整道路图; style=6(webst)=卫星影像;
// style=8(webst)=透明路网注记层, 专门叠在卫星影像上用。
fn gaode_road_url(z: u32, x: isize, y: isize) -> String {
    let s = (x + y).rem_euclid(4) as isize + 1;
    format!("https://webrd0{s}.is.autonavi.com/appmaptile?lang=zh_cn&size=1&scale=1&style=8&x={x}&y={y}&z={z}")
}

fn gaode_sat_url(z: u32, x: isize, y: isize) -> String {
    let s = (x + y).rem_euclid(4) as isize + 1;
    format!("https://webst0{s}.is.autonavi.com/appmaptile?style=6&x={x}&y={y}&z={z}")
}

fn gaode_sat_road_url(z: u32, x: isize, y: isize) -> String {
    let s = (x + y).rem_euclid(4) as isize + 1;
    format!("https://webst0{s}.is.autonavi.com/appmaptile?style=8&x={x}&y={y}&z={z}")
}

/// 瓦片图层: 0=道路底图, 1=卫星影像, 2=卫星图上的路网注记(透明, 叠加层)。
/// 参与瓦片坐标排序/缓存键 —— 两种底图各自缓存, 来回切换不重拉。
const LAYER_ROAD: u8 = 0;
const LAYER_SAT: u8 = 1;
const LAYER_SAT_ROAD: u8 = 2;

/// 给定当前底图 cur, 判断瓦片图层 layer 是否属于当前图层集合。
/// 自由函数(不借 self), 供 retain 闭包直接使用。
fn in_current_layers(cur: u8, layer: u8) -> bool {
    if cur == LAYER_SAT {
        layer == LAYER_SAT || layer == LAYER_SAT_ROAD
    } else {
        layer == LAYER_ROAD
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
struct TileCoordinate {
    z: u32,
    x: isize,
    y: isize,
    layer: u8,
}

struct MapEngine {
    client: reqwest::Client,
    cache_dir: PathBuf,
    loaded_tiles: BTreeMap<TileCoordinate, Image>,
    loading_tiles: BTreeMap<TileCoordinate, Pin<Box<dyn Future<Output = Image>>>>,
    /// 瓦片源基址: 设了 $GAODE_TILES_URL 则按路径式 {z}/{x}/{y}.png 取瓦片;
    /// 空字符串 = 默认高德道路底图(gaode_road_url)。
    tile_url: String,
    zoom_level: u32,
    visible_width: f64,
    visible_height: f64,
    offset_x: f64,
    offset_y: f64,
    /// 落点标记 (lat, lon) —— 以 WGS-84 存储/显示
    marker: Option<(f64, f64)>,
    /// 视口是否已初始化(首个 set_center 之后为 true)。
    /// 用来忽略 Flickable 启动瞬间因内容尺寸尚为 0 而误发的 flicked(0,0)。
    initialized: bool,
    /// 当前底图图层: LAYER_ROAD(路网) 或 LAYER_SAT(卫星, 另叠路网注记)。
    layer: u8,
}

impl MapEngine {
    fn new(cache_dir: PathBuf) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("Slint-Map-Demo/1.0 (slint-demo article-18; gaode)")
            .build()
            .expect("创建 HTTP 客户端失败");
        MapEngine {
            client,
            cache_dir,
            loaded_tiles: Default::default(),
            loading_tiles: Default::default(),
            tile_url: std::env::var("GAODE_TILES_URL").unwrap_or_default(),
            zoom_level: MIN_ZOOM,
            visible_width: 0.0,
            visible_height: 0.0,
            offset_x: 0.0,
            offset_y: 0.0,
            marker: None,
            initialized: false,
            layer: LAYER_ROAD,
        }
    }

    /// 该瓦片坐标是否属于当前底图的图层集合。
    /// 卫星模式 = 卫星影像 + 路网注记两层; 路网模式 = 道路底图一层。
    fn is_current_layer(&self, layer: u8) -> bool {
        in_current_layers(self.layer, layer)
    }

    /// 把 (lat, lon) [WGS-84] 放到视口中心并设缩放级。
    /// 内部转 GCJ-02 再做墨卡托投影(高德瓦片坐标系)。
    fn set_center(&mut self, lat: f64, lon: f64, zoom: u32) {
        self.zoom_level = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        let (glat, glon) = wgs84_to_gcj02(lat, lon);
        let wx = lon_to_x(glon, self.zoom_level);
        let wy = lat_to_y(glat, self.zoom_level);
        // offset = 视口左上角对应的世界像素 = 中心世界像素 − 半视口。
        // 必须这样算(而不是反过来): 屏幕 = world − offset, 且 Flickable
        // content-x = -offset, Slint 的滚动域是 [width−content-width, 0] —— 符号
        // 反了会让 content-x 为正、被钳到 0, 瓦片全部落到视口外 → 整屏空白。
        self.offset_x = wx - self.visible_width / 2.0;
        self.offset_y = wy - self.visible_height / 2.0;
        self.initialized = true;
        self.loaded_tiles.clear();
        self.loading_tiles.clear();
        self.reset_view();
    }

    /// 拖动结束: 由 Flickable.content-x/y 反推 offset。
    fn pan_to(&mut self, content_x: f64, content_y: f64) {
        self.offset_x = -content_x;
        self.offset_y = -content_y;
        self.reset_view();
    }

    /// 缩放一级, 锚点 (ax, ay) 为视口(屏幕)坐标 —— 锚点处的地理坐标缩放前后不动。
    ///
    /// 不清空已加载瓦片: 旧级瓦片由 reset_view 按"与视口相交"保留, 在新级
    /// 瓦片到货前按比例缩放着垫底 —— 捏合缩放不再整屏闪白。
    fn zoom_to(&mut self, zoom: u32, ax: f64, ay: f64) {
        let zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        if self.zoom_level == zoom {
            return;
        }
        let exp2 = f64::exp2(zoom as f64 - self.zoom_level as f64);
        self.offset_x = (self.offset_x + ax) * exp2 - ax;
        self.offset_y = (self.offset_y + ay) * exp2 - ay;
        self.zoom_level = zoom;
        self.reset_view();
    }

    /// 点击落点: 世界像素 (wx, wy) [GCJ-02] 反投影成 GCJ-02 经纬度, 再转 WGS-84 存。
    fn set_marker_world(&mut self, wx: f64, wy: f64) {
        let glat = y_to_lat(wy, self.zoom_level);
        let glon = x_to_lon(wx, self.zoom_level);
        let (lat, lon) = gcj02_to_wgs84(glat, glon);
        self.marker = Some((lat, lon));
    }

    /// 根据当前 offset / 可视尺寸算出可见瓦片范围, 请求缺失瓦片, 并淘汰远处瓦片。
    ///
    /// 淘汰规则是"跨缩放级"的: 旧级瓦片按 2^(当前级-瓦片级) 换算到当前世界像素
    /// 空间后与视口(含 KEEP 余量)相交者保留 —— 它们在新级瓦片到货前按比例
    /// 缩放垫底, 捏合/按钮缩放不再闪白。当前级视口瓦片全部到货后才清掉垫底。
    fn reset_view(&mut self) {
        let m = (1i64 << self.zoom_level).min(isize::MAX as i64) as isize;
        let min_x = (self.offset_x / TILE_SIZE).floor() as isize;
        let min_y = (self.offset_y / TILE_SIZE).floor() as isize;
        let max_x = (((self.offset_x + self.visible_width) / TILE_SIZE).ceil() as isize + 1)
            .clamp(0, m);
        let max_y = (((self.offset_y + self.visible_height) / TILE_SIZE).ceil() as isize + 1)
            .clamp(0, m);

        // 视口在世界像素空间中的范围(含 KEEP 余量)
        let margin = KEEP_CACHED_TILES as f64 * TILE_SIZE;
        let vl = self.offset_x - margin;
        let vt = self.offset_y - margin;
        let vr = self.offset_x + self.visible_width + margin;
        let vb = self.offset_y + self.visible_height + margin;
        // 瓦片 (z,x,y) 换算到当前级世界像素: 尺寸 = 256 * 2^(当前级-z)
        let intersects = |c: &TileCoordinate| -> bool {
            let s = f64::exp2(self.zoom_level as f64 - c.z as f64);
            let x = c.x as f64 * TILE_SIZE * s;
            let y = c.y as f64 * TILE_SIZE * s;
            let w = TILE_SIZE * s;
            x + w > vl && x < vr && y + w > vt && y < vb
        };

        self.loaded_tiles.retain(|c, _| intersects(c));
        // 在途请求跨级保留(完成后即成垫底瓦片), 只裁视口外的
        self.loading_tiles.retain(|c, _| intersects(c));

        // 当前级视口瓦片已全部就绪 => 垫底瓦片不再需要
        let current_complete = (min_x..max_x).all(|x| {
            (min_y..max_y).all(|y| {
                [LAYER_ROAD, LAYER_SAT, LAYER_SAT_ROAD].iter()
                    .filter(|&&l| self.is_current_layer(l))
                    .all(|&l| {
                        self.loaded_tiles
                            .contains_key(&TileCoordinate { z: self.zoom_level, x, y, layer: l })
                    })
            })
        });
        if current_complete {
            let cur = self.layer;
            self.loaded_tiles.retain(|c, _| in_current_layers(cur, c.layer));
        }

        // 克隆出局部句柄, 让闭包只借局部变量, 不与 self.loading_tiles 的 &mut 冲突
        let client = self.client.clone();
        let cache_dir = self.cache_dir.clone();
        let base = self.tile_url.clone();
        // 卫星模式要拉两层: 影像 + 透明路网注记
        let layers: &[u8] = if self.layer == LAYER_SAT {
            &[LAYER_SAT, LAYER_SAT_ROAD]
        } else {
            &[LAYER_ROAD]
        };
        for x in min_x..max_x {
            for y in min_y..max_y {
                for &l in layers {
                let coord = TileCoordinate { z: self.zoom_level, x, y, layer: l };
                if self.loaded_tiles.contains_key(&coord)
                    || self.loading_tiles.contains_key(&coord)
                {
                    continue;
                }
                self.loading_tiles.entry(coord).or_insert_with(|| {
                    let client = client.clone();
                    let cache_dir = cache_dir.clone();
                    let url = if base.is_empty() {
                        match coord.layer {
                            LAYER_SAT => gaode_sat_url(coord.z, coord.x, coord.y),
                            LAYER_SAT_ROAD => gaode_sat_road_url(coord.z, coord.x, coord.y),
                            _ => gaode_road_url(coord.z, coord.x, coord.y),
                        }
                    } else {
                        format!("{}/{}/{}/{}.png", base, coord.z, coord.x, coord.y)
                    };
                    Box::pin(async move { fetch_tile(&client, &cache_dir, url, coord).await })
                });
                }
            }
        }
    }

    /// 轮询在途请求: 完成的搬进 loaded_tiles, 标记 changed。
    fn poll(&mut self, ctx: &mut Context, changed: &mut bool) {
        self.loading_tiles.retain(|coord, fut| match fut.as_mut().poll(ctx) {
            Poll::Ready(img) => {
                self.loaded_tiles.insert(*coord, img);
                *changed = true;
                false
            }
            Poll::Pending => true,
        });
        // 全部在途请求结束 => 当前图层已齐, 垫底的另一底图/旧级瓦片可以退场
        if self.loading_tiles.is_empty() {
            let n = self.loaded_tiles.len();
            let cur = self.layer;
            self.loaded_tiles.retain(|c, _| in_current_layers(cur, c.layer));
            if self.loaded_tiles.len() != n {
                *changed = true;
            }
        }
    }
}

/// 地面分辨率: 当前缩放级、纬度处, 1 世界(逻辑)像素代表多少米。
/// Web Mercator: 赤道 z0 一个像素 = 40075016.7m/256 ≈ 156543m,
/// 随纬度按 cosφ 收缩、每升一级减半。
fn ground_m_per_px(lat_deg: f64, zoom: u32) -> f64 {
    156543.03392 * lat_deg.to_radians().cos() / f64::exp2(zoom as f64)
}

/// 图形比例尺条(参考主流地图 App): 在 1-2-5 序列里挑最接近
/// 目标条长(100 逻辑px)的"整距离", 返回 (米, 条长px)。
/// 整距离保证标签是人类友好的 "500 米 / 2 公里", 条长随级别平滑伸缩。
fn scale_bar(m_per_px: f64) -> (f64, f64) {
    let target = 100.0 * m_per_px;
    let pow = 10f64.powf(target.log10().floor());
    let mut best = pow;
    for k in [2.0, 5.0, 10.0] {
        if (k * pow - target).abs() < (best - target).abs() {
            best = k * pow;
        }
    }
    (best, best / m_per_px)
}

/// 距离标签: ≥1km 用"公里"(1-2-5 序列必为整数), 否则"米"。
fn format_distance(m: f64) -> String {
    if m >= 1000.0 {
        format!("{} 公里", (m / 1000.0).round() as i64)
    } else {
        format!("{} 米", m.round() as i64)
    }
}

/// 取一块瓦片: 先查磁盘缓存, 没有再网络拉取并落盘, 最后解码成 slint::Image。
async fn fetch_tile(
    client: &reqwest::Client,
    cache_dir: &PathBuf,
    url: String,
    coord: TileCoordinate,
) -> Image {
    // 缓存键必须含图层: 卫星/路网同 z/x/y 是不同图像, 不分层会互串
    // (切到卫星后命中路网缓存文件, 把路网图当卫星显示)
    let path = cache_dir
        .join(coord.z.to_string())
        .join(coord.x.to_string())
        .join(format!("{}_{}.png", coord.layer, coord.y));

    let bytes: Vec<u8> = if path.exists() {
        match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("读取瓦片缓存失败 {}: {e}", path.display());
                return Image::default();
            }
        }
    } else {
        let resp = match client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("瓦片请求失败 {url}: {e}");
                return Image::default();
            }
        };
        if !resp.status().is_success() {
            eprintln!("瓦片请求失败 {url}: {:?}", resp.status());
            return Image::default();
        }
        let b = match resp.bytes().await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("瓦片读取失败 {url}: {e}");
                return Image::default();
            }
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, &b);
        b.to_vec()
    };

    // 解码放到线程, 不阻塞 UI 事件循环
    let url2 = url.clone();
    let buffer = tokio::task::spawn_blocking(move || match image::load_from_memory(&bytes) {
        Ok(img) => {
            let img = img.into_rgba8();
            let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                img.as_raw(),
                img.width(),
                img.height(),
            );
            Some(buf)
        }
        Err(e) => {
            eprintln!("瓦片解码失败 {url2}: {e}");
            None
        }
    })
    .await;
    match buffer {
        Ok(Some(buf)) => Image::from_rgba8(buf),
        _ => Image::default(),
    }
}

pub(crate) struct App {
    engine: RefCell<MapEngine>,
    pub(crate) main: MainWindow,
    poll_handle: RefCell<Option<slint::JoinHandle<()>>>,
}

impl App {
    fn new(main: MainWindow, cache_dir: PathBuf) -> Rc<Self> {
        Rc::new(App {
            engine: RefCell::new(MapEngine::new(cache_dir)),
            main,
            poll_handle: RefCell::new(None),
        })
    }

    /// 把当前引擎状态推给 Slint: 视口 + 瓦片模型 + 覆盖层(中心/标记/状态)。
    fn apply(self: Rc<Self>) {
        let (offset_x, offset_y, zoom) = {
            let eng = self.engine.borrow();
            (eng.offset_x, eng.offset_y, eng.zoom_level)
        };
        let world = world_size(zoom) as f32;
        self.main
            .invoke_set_viewport(-offset_x as f32, -offset_y as f32, world, world);
        self.publish();
        self.do_poll();
    }

    /// 刷新 Slint 上的瓦片 + 读数。
    ///
    /// 瓦片几何按各自缩放级换算到当前世界像素空间: 尺寸 = 256 * 2^(当前级-z)。
    /// 旧级瓦片(垫底)先画、当前级后画(压顶), 换级过程中新图在旧图上浮现。
    fn publish(&self) {
        let eng = self.engine.borrow();
        // 影像在下、注记在上: (z, layer) 升序, 同格内 LAYER_SAT 先画、
        // LAYER_SAT_ROAD 后画, 路网名恰好压在卫星图上
        let mut items: Vec<(&TileCoordinate, &Image)> = eng.loaded_tiles.iter().collect();
        items.sort_by_key(|(c, _)| (c.z, c.layer));
        let tiles: Vec<MapTile> = items
            .iter()
            .map(|(c, img)| {
                let s = f32::exp2(eng.zoom_level as f32 - c.z as f32);
                MapTile {
                    tile: (*img).clone(),
                    x: c.x as f32 * TILE_SIZE as f32 * s,
                    y: c.y as f32 * TILE_SIZE as f32 * s,
                    w: TILE_SIZE as f32 * s,
                }
            })
            .collect();
        let model = self.main.global::<MapModel>();
        model.set_tiles(ModelRc::new(VecModel::from(tiles)));
        model.set_zoom(eng.zoom_level as f32);
        model.set_zoom_text(format!("{}", eng.zoom_level).into());

        // 视口中心的世界像素 = 屏幕中心 + offset, 反投影得 GCJ-02 经纬度, 再转 WGS-84 显示
        let cx = eng.visible_width / 2.0 + eng.offset_x;
        let cy = eng.visible_height / 2.0 + eng.offset_y;
        let glat = y_to_lat(cy, eng.zoom_level);
        let glon = x_to_lon(cx, eng.zoom_level);
        let (lat, lon) = gcj02_to_wgs84(glat, glon);
        model.set_center_lat(format!("{:.5}", lat).into());
        model.set_center_lon(format!("{:.5}", lon).into());

        // 图形比例尺(右下角横条): 1-2-5 取整的整距离
        let mpp = ground_m_per_px(lat, eng.zoom_level);
        let (dist_m, bar_px) = scale_bar(mpp);
        model.set_scale_bar_width(bar_px as f32);
        model.set_scale_bar_label(format_distance(dist_m).into());

        if let Some((lat, lon)) = eng.marker {
            // 存的是 WGS-84, 投到高德瓦片需先转 GCJ-02
            let (glat, glon) = wgs84_to_gcj02(lat, lon);
            let mx = lon_to_x(glon, eng.zoom_level);
            let my = lat_to_y(glat, eng.zoom_level);
            model.set_marker_x(mx as f32);
            model.set_marker_y(my as f32);
            model.set_has_marker(true);
            model.set_marker_lat(format!("{:.5}", lat).into());
            model.set_marker_lon(format!("{:.5}", lon).into());
        } else {
            model.set_has_marker(false);
            model.set_marker_lat("-".into());
            model.set_marker_lon("-".into());
        }
        model.set_status(
            if eng.loading_tiles.is_empty() {
                "就绪".into()
            } else {
                format!("瓦片加载中… ({} 块)", eng.loading_tiles.len()).into()
            },
        );
        // 底图切换按钮文案: 当前路网 → 提示可切"卫"; 当前卫星 → 提示可切回"图"
        model.set_is_satellite(eng.layer == LAYER_SAT);
    }

    /// 启动/重启瓦片轮询: 在途请求完成后把新瓦片写回 Slint。
    fn do_poll(self: Rc<Self>) {
        if let Some(handle) = self.poll_handle.take() {
            handle.abort();
        }
        self.publish();
        let weak = Rc::downgrade(&self);
        slint::spawn_local(async move {
            std::future::poll_fn(|ctx| {
                if let Some(app) = weak.upgrade() {
                    let mut changed = false;
                    app.engine.borrow_mut().poll(ctx, &mut changed);
                    if changed {
                        app.publish();
                    }
                    if app.engine.borrow().loading_tiles.is_empty() {
                        Poll::Ready(())
                    } else {
                        Poll::Pending
                    }
                } else {
                    Poll::Ready(())
                }
            })
            .await;
        })
        .expect("启动瓦片轮询失败");
    }

    /// 显式启动初始化 —— 必须在 bind() 之后、run() 之前调用。
    ///
    /// 不能只依赖 Slint 组件的 `init => view-ready()`: `MainWindow::new()` 期间
    /// 组件就已实例化并触发 init(连带发出 view-ready), 而那时 `bind()` 还没把
    /// `on_view_ready` 接上, 信号直接丢失, 于是初始视口永远设不进去 —— 正是本
    /// demo 首版"整屏空白、缩放级停在 1"的根因。这里主动 kick 一次最稳。
    pub(crate) fn start(self: &Rc<Self>) {
        App::init_viewport(Rc::downgrade(self), 0);
    }

    /// 页面就绪后等可视尺寸, 再设初始视口(尺寸未就绪则定时重试)。
    fn init_viewport(app_weak: std::rc::Weak<App>, attempt: u32) {
        let Some(app) = app_weak.upgrade() else {
            return;
        };
        // 已初始化(如 init 回调与显式 start 都触发)则不再重设, 免得把用户
        // 已经平移/缩放过的视图拽回默认中心。
        if app.engine.borrow().initialized {
            return;
        }
        let w = app.main.get_visible_width() as f64;
        let h = app.main.get_visible_height() as f64;
        if w <= 1.0 || h <= 1.0 {
            if attempt < 50 {
                let a2 = std::rc::Weak::clone(&app_weak);
                slint::Timer::single_shot(std::time::Duration::from_millis(100), move || {
                    App::init_viewport(a2, attempt + 1);
                });
            } else {
                log::warn!("地图初始视口: 窗口尺寸始终未就绪");
            }
            return;
        }
        {
            let mut eng = app.engine.borrow_mut();
            eng.visible_width = w;
            eng.visible_height = h;
            eng.set_center(DEFAULT_CENTER.0, DEFAULT_CENTER.1, DEFAULT_ZOOM);
        }
        app.apply();
    }
}

/// 把 Slint 侧的所有回调接到引擎上。返回持有的 App(Rc), 供调用方 run()。
pub fn bind(app: MainWindow, cache_dir: PathBuf) -> Rc<App> {
    let app_rc = App::new(app, cache_dir);

    {
        let a = Rc::clone(&app_rc);
        app_rc.main.global::<MapModel>().on_flicked(move |ox, oy| {
            // 视口就绪前的 flicked 是 Flickable 因内容尺寸尚为 0 自发钳位产生的
            // (0,0), 若照单全收会把 offset 清成 0、地图跳到世界角落。忽略之。
            let ready = a.engine.borrow().initialized;
            if !ready {
                return;
            }
            let w = a.main.get_visible_width() as f64;
            let h = a.main.get_visible_height() as f64;
            let mut eng = a.engine.borrow_mut();
            eng.visible_width = w;
            eng.visible_height = h;
            eng.pan_to(ox as f64, oy as f64);
            drop(eng);
            Rc::clone(&a).apply();
        });
    }
    {
        let a = Rc::clone(&app_rc);
        app_rc.main.global::<MapModel>().on_zoom_in(move |ax, ay| {
            let w = a.main.get_visible_width() as f64;
            let h = a.main.get_visible_height() as f64;
            let mut eng = a.engine.borrow_mut();
            eng.visible_width = w;
            eng.visible_height = h;
            let z = (eng.zoom_level + 1).min(MAX_ZOOM);
            eng.zoom_to(z, ax as f64, ay as f64);
            drop(eng);
            Rc::clone(&a).apply();
        });
    }
    {
        let a = Rc::clone(&app_rc);
        app_rc.main.global::<MapModel>().on_zoom_out(move |ax, ay| {
            let w = a.main.get_visible_width() as f64;
            let h = a.main.get_visible_height() as f64;
            let mut eng = a.engine.borrow_mut();
            eng.visible_width = w;
            eng.visible_height = h;
            let z = eng.zoom_level.saturating_sub(1).max(MIN_ZOOM);
            eng.zoom_to(z, ax as f64, ay as f64);
            drop(eng);
            Rc::clone(&a).apply();
        });
    }
    {
        let a = Rc::clone(&app_rc);
        app_rc.main.global::<MapModel>().on_zoom_in_center(move || {
            let w = a.main.get_visible_width() as f64;
            let h = a.main.get_visible_height() as f64;
            let mut eng = a.engine.borrow_mut();
            eng.visible_width = w;
            eng.visible_height = h;
            let z = (eng.zoom_level + 1).min(MAX_ZOOM);
            eng.zoom_to(z, w / 2.0, h / 2.0);
            drop(eng);
            Rc::clone(&a).apply();
        });
    }
    {
        let a = Rc::clone(&app_rc);
        app_rc.main.global::<MapModel>().on_zoom_out_center(move || {
            let w = a.main.get_visible_width() as f64;
            let h = a.main.get_visible_height() as f64;
            let mut eng = a.engine.borrow_mut();
            eng.visible_width = w;
            eng.visible_height = h;
            let z = eng.zoom_level.saturating_sub(1).max(MIN_ZOOM);
            eng.zoom_to(z, w / 2.0, h / 2.0);
            drop(eng);
            Rc::clone(&a).apply();
        });
    }
    {
        let a = Rc::clone(&app_rc);
        app_rc.main.global::<MapModel>()
            .on_set_coordinate(move |wx, wy| {
                let mut eng = a.engine.borrow_mut();
                eng.set_marker_world(wx as f64, wy as f64);
                drop(eng);
                a.publish(); // 只更新覆盖层, 不触发新瓦片
            });
    }
    {
        let a = Rc::clone(&app_rc);
        app_rc.main.global::<MapModel>().on_recenter(move || {
            let mut eng = a.engine.borrow_mut();
            if let Some((lat, lon)) = eng.marker {
                let z = eng.zoom_level;
                eng.set_center(lat, lon, z);
            }
            drop(eng);
            Rc::clone(&a).apply();
        });
    }
    {
        // 底图切换: 路网 ↔ 卫星。旧底图瓦片按垫底规则保留, 新图层到货后自动退场;
        // 两种底图各自入缓存键, 来回切换不重拉。
        let a = Rc::clone(&app_rc);
        app_rc.main.global::<MapModel>().on_toggle_satellite(move || {
            let w = a.main.get_visible_width() as f64;
            let h = a.main.get_visible_height() as f64;
            let mut eng = a.engine.borrow_mut();
            eng.visible_width = w;
            eng.visible_height = h;
            eng.layer = if eng.layer == LAYER_SAT { LAYER_ROAD } else { LAYER_SAT };
            eng.reset_view();
            drop(eng);
            Rc::clone(&a).apply();
        });
    }
    {
        let aw = Rc::downgrade(&app_rc);
        app_rc.main.global::<MapModel>().on_view_ready(move || {
            App::init_viewport(aw.clone(), 0);
        });
    }
    {
        // 窗口尺寸变化: 更新引擎视口并补拉新露出的瓦片。
        // 布局期 changed 会连发, 用阈值滤掉亚像素抖动; 未初始化(首帧前)不处理,
        // 交给 init_viewport。
        let a = Rc::clone(&app_rc);
        app_rc.main.global::<MapModel>().on_viewport_changed(move || {
            let w = a.main.get_visible_width() as f64;
            let h = a.main.get_visible_height() as f64;
            let mut eng = a.engine.borrow_mut();
            if !eng.initialized
                || ((eng.visible_width - w).abs() < 0.5
                    && (eng.visible_height - h).abs() < 0.5)
            {
                return;
            }
            eng.visible_width = w;
            eng.visible_height = h;
            eng.reset_view();
            drop(eng);
            Rc::clone(&a).apply();
        });
    }
    app_rc
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 窗口尺寸变大: 更新视口后 reset_view 应为"新露出"的右侧/下侧区域
    /// 发起拉取(loading_tiles 出现原视口范围之外的瓦片坐标)。
    #[test]
    fn resize_wider_fetches_newly_exposed_tiles() {
        let mut eng = MapEngine::new(std::env::temp_dir());
        eng.visible_width = 400.0;
        eng.visible_height = 400.0;
        eng.set_center(DEFAULT_CENTER.0, DEFAULT_CENTER.1, DEFAULT_ZOOM);
        assert!(eng.initialized);

        // 视口扩大一倍: 模拟 on_viewport_changed 的处理逻辑
        eng.visible_width = 800.0;
        eng.visible_height = 800.0;
        eng.reset_view();

        // 中心世界像素不变(offset 不因 resize 变), 新范围瓦片应进入拉取队列
        let (glat, glon) = wgs84_to_gcj02(DEFAULT_CENTER.0, DEFAULT_CENTER.1);
        let wx = lon_to_x(glon, DEFAULT_ZOOM);
        let wy = lat_to_y(glat, DEFAULT_ZOOM);
        let cx_tile = (wx / TILE_SIZE).floor() as isize;
        let cy_tile = (wy / TILE_SIZE).floor() as isize;
        let right_new = TileCoordinate {
            z: DEFAULT_ZOOM,
            x: cx_tile + 2,
            y: cy_tile,
            layer: LAYER_ROAD,
        };
        assert!(
            eng.loading_tiles.contains_key(&right_new) || eng.loaded_tiles.contains_key(&right_new),
            "加宽后右侧新露出的瓦片应被请求或已加载"
        );
    }

    /// set_center 之后: 视口中心世界像素应等于目标点, 且 content-x(=-offset)
    /// 必须 ≤ 0 —— 否则会被 Slint 的滚动域 [width−content-width, 0] 钳到 0,
    /// 瓦片全部落到视口外(这就是"整屏空白"的成因)。
    #[test]
    fn set_center_puts_target_at_viewport_center() {
        let mut eng = MapEngine::new(std::env::temp_dir());
        eng.visible_width = 800.0;
        eng.visible_height = 600.0;
        eng.set_center(DEFAULT_CENTER.0, DEFAULT_CENTER.1, DEFAULT_ZOOM);

        let (glat, glon) = wgs84_to_gcj02(DEFAULT_CENTER.0, DEFAULT_CENTER.1);
        let wx = lon_to_x(glon, DEFAULT_ZOOM);
        let wy = lat_to_y(glat, DEFAULT_ZOOM);

        let cx = eng.visible_width / 2.0 + eng.offset_x;
        let cy = eng.visible_height / 2.0 + eng.offset_y;
        assert!((cx - wx).abs() < 1e-6, "视口中心 x 偏差: cx={cx} wx={wx}");
        assert!((cy - wy).abs() < 1e-6, "视口中心 y 偏差: cy={cy} wy={wy}");

        assert!(-eng.offset_x <= 0.0, "content-x 必须 ≤ 0, 实际 {}", -eng.offset_x);
        assert!(-eng.offset_y <= 0.0, "content-y 必须 ≤ 0, 实际 {}", -eng.offset_y);

        assert!(!eng.loading_tiles.is_empty(), "应已针对可见范围发起瓦片请求");
    }

    /// 卫星瓦片 URL: webst 子域 + style=6 影像 / style=8 透明注记, 子域轮换稳定。
    #[test]
    fn satellite_urls() {
        let u = gaode_sat_url(12, 3372, 1553);
        assert!(u.starts_with("https://webst0"), "{u}");
        assert!(u.contains("style=6"), "{u}");
        let u2 = gaode_sat_road_url(12, 3372, 1553);
        assert!(u2.contains("style=8") && u2.starts_with("https://webst0"), "{u2}");
        // 同一瓦片两个图层取同一子域(轮换只看 x+y), 利好 HTTP 连接复用
        assert_eq!(u.split('?').next(), u2.split('?').next());
    }

    /// 图形比例尺: 地面分辨率符合 Web Mercator 量级; 比例尺条落在 1-2-5
    /// 序列上且条长在可读范围(50~160px); 标签是整值"公里/米"。
    #[test]
    fn scale_bar_matches_expectations() {
        assert!((ground_m_per_px(0.0, 0) - 156_543.03).abs() < 1.0, "z0 赤道基准");
        let mpp = ground_m_per_px(39.9, 12);
        assert!((28.0..31.0).contains(&mpp), "北京 z12 m/px 异常: {mpp}");

        let (dist, px) = scale_bar(mpp);
        assert!((50.0..160.0).contains(&px), "条长应可读: {px}px");
        assert!((dist / mpp - px).abs() < 1e-9);
        assert_eq!(dist, 2000.0, "北京 z12 应取 2 公里");
        assert_eq!(format_distance(dist), "2 公里");

        // 缩小一级: 距离应变长, 条长仍在范围内
        let (dist11, px11) = scale_bar(ground_m_per_px(39.9, 11));
        assert!(dist11 > dist && (50.0..160.0).contains(&px11));
        assert_eq!(format_distance(dist11), "5 公里");

        assert_eq!(format_distance(500.0), "500 米");
    }

    /// 换缩放级不清空旧级瓦片: 与新视口相交的旧级瓦片保留为垫底(不闪白),
    /// 新发起的在途请求全部是当前级。这是捏合缩放流畅度的关键行为。
    #[test]
    fn zoom_keeps_old_level_tiles_as_backdrop() {
        let mut eng = MapEngine::new(std::env::temp_dir());
        eng.visible_width = 800.0;
        eng.visible_height = 600.0;
        eng.set_center(DEFAULT_CENTER.0, DEFAULT_CENTER.1, DEFAULT_ZOOM);

        // 把视口中心处的当前级瓦片假装成"已加载"
        let (glat, glon) = wgs84_to_gcj02(DEFAULT_CENTER.0, DEFAULT_CENTER.1);
        let wx = lon_to_x(glon, DEFAULT_ZOOM);
        let wy = lat_to_y(glat, DEFAULT_ZOOM);
        let cx_tile = (wx / TILE_SIZE).floor() as isize;
        let cy_tile = (wy / TILE_SIZE).floor() as isize;
        for dx in 0..=1 {
            for dy in 0..=1 {
                eng.loaded_tiles.insert(
                    TileCoordinate { z: DEFAULT_ZOOM, x: cx_tile + dx, y: cy_tile + dy, layer: LAYER_ROAD },
                    slint::Image::default(),
                );
            }
        }

        // 放大一级(锚点=视口中心)
        eng.zoom_to(DEFAULT_ZOOM + 1, 400.0, 300.0);

        // 旧级(z12)瓦片仍在 loaded —— 垫底
        assert!(
            eng.loaded_tiles.keys().any(|c| c.z == DEFAULT_ZOOM),
            "换级后旧级瓦片应保留为垫底"
        );
        // 旧级瓦片按 2^(13-12)=2 倍换算后仍应盖住视口中心
        let s = f64::exp2(eng.zoom_level as f64 - DEFAULT_ZOOM as f64);
        let bx = cx_tile as f64 * TILE_SIZE * s;
        let by = cy_tile as f64 * TILE_SIZE * s;
        let bw = TILE_SIZE * s;
        let center_px = eng.visible_width / 2.0 + eng.offset_x; // 视口中心世界像素
        assert!(
            bx <= center_px && center_px <= bx + bw,
            "垫底瓦片应覆盖视口中心: bx={bx} bw={bw} center={center_px}"
        );
        let center_py = eng.visible_height / 2.0 + eng.offset_y;
        assert!(
            by <= center_py && center_py <= by + bw,
            "垫底瓦片应覆盖视口中心: by={by} bw={bw} center={center_py}"
        );
        // 在途请求必须包含当前级
        assert!(
            eng.loading_tiles.keys().any(|c| c.z == eng.zoom_level),
            "应已发起新级瓦片请求"
        );
    }
}
