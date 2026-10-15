# 第十八篇, Slint 地图显示 + 定位

用 **Slint** 做一个**地图 + 定位**演示：一片可平移、可缩放的栅格瓦片地图，能点击落点、
看中心 / 标记读数，还能**开启系统定位**——蓝点落在真实 GPS 位置（带精度圈），
"回中"升级成"回到我的真实位置"，蓝点跟着人走。地图数据来自 **高德地图** 公共栅格瓦片
（GCJ-02 坐标系，免 API key），首次运行需联网，之后走**磁盘缓存**。

> 本篇参照 `yehun-slint` 项目的**定位页面**（`yehun-ui/ui/panel/test/location/`）而来：
> 复用其「Flickable 瓦片层 + 落点标记 + 右侧控制列 + 底部信息卡」的整体架构，
> 并把瓦片源对齐到与之一致的**高德 GCJ-02** 底图；定位侧同样参考其定位插件的
> 收口架构（统一接口 + provider 命名判坐标系 + 回调线程回投 UI），裁成自包含简化版：
> Android 走系统 `LocationManager`（JNI），桌面走 WiFi WPS 逐级兜底
> （NetworkManager 扫 AP → Apple/beaconDB → IP，城市内实测几十米）。

## 架构

```
crates/article-18      Slint 地图 + 定位演示
   ├─ ui/app.slint       Flickable 瓦片层 / 蓝点 + 精度圈 / 控制列 / 信息卡 / 高德署名
   └─ src/
       ├─ map.rs         墨卡托投影 + 瓦片引擎(高德拉取 + 磁盘缓存)
       │                 + Flickable 视口(平移 / 锚点缩放) + GPS 真值 / 蓝点冻结 / 跟随回中
       ├─ gcj02.rs       WGS-84 ↔ GCJ-02(火星坐标)互转, 纯数学无依赖
       └─ location/      定位体系(多平台收口)
           ├─ mod.rs     Location / 统一接口 / provider 命名 ↔ 文案
           ├─ android.rs 系统 LocationManager(JNI 双向桥, 权限全在 Java 侧闭环)
           └─ desktop/   WiFi WPS 逐级兜底
               ├─ mod.rs           FallbackService(worker 线程, WiFi 帧 60s 刷新)
               ├─ wifi.rs          WifiScanner trait + Apple/beaconDB 查询 + 逐级回退
               ├─ apple.rs         Apple WLOC 定位(手写 protobuf 编解码, 免密钥)
               ├─ ip.rs            IP 定位兜底(ip-api.com, 城市级单发)
               ├─ linux_scanner.rs NetworkManager AP 扫描(D-Bus, zbus)
               ├─ windows_scanner.rs wlanapi.dll BSS 扫描(WlanGetNetworkBssList)
               └─ macos_scanner.rs CoreWLAN AP 扫描( objc2-core-wlan )
```

UI 主线程只管渲染；**瓦片异步拉取、定位请求都在工作线程 / async 任务里**，
结果经事件循环写回 Slint。

## 关键设计

1. **瓦片 = 世界像素**：缩放级 `z` 下整张世界是 `256·2^z` 像素的正方形；经纬度经 Web
   Mercator 投影成「世界像素」，瓦片按世界像素平铺进 `Flickable`。
2. **Flickable 当视口**：`content-x = -offset`，瓦片按世界像素摆放，拖动即平移，随图滚动。
3. **缩放锚点不动**：滚轮 / 按钮缩放时，锚点处的地理坐标缩放前后保持不动（`zoom_to` 公式）。
4. **点击落点**：触点世界像素反投影成经纬度即标记；默认中心 / 回中同理。
5. **坐标系边界转换**：高德瓦片是 GCJ-02，而 GPS / 用户直觉是 WGS-84。引擎内部投影一律
   用 GCJ-02；公共坐标（默认中心 / 落点 / 屏幕读数）以 WGS-84 呈现，仅在 `set_center` /
   反投影 / 落点处做 `wgs84_to_gcj02` / `gcj02_to_wgs84` 转换。

## 运行

```bash
# 1. 编译并运行(Linux, 首次需联网拉取高德瓦片)
cd crates/article-18
make run-linux

# 或自定义瓦片缓存目录
MAP_CACHE_DIR="$HOME/.cache/slint-map" make run-linux

# 想换其他 {z}/{x}/{y}.png 风格源(如自建镜像): 用 GAODE_TILES_URL 覆盖基址
GAODE_TILES_URL="https://my-tile-mirror.example.com" make run-linux
```

- 拖动画面：平移（同时解除定位跟随）
- 鼠标滚轮：以光标为锚点缩放
- 右侧 `+ / −`：以视口中心缩放
- 右侧 `⌖`：**定位按钮** —— 首次点击开启定位（Android 弹系统授权框 / 桌面 WiFi 定位）；
  已开启再点 = 回到我的真实位置（蓝点回 GPS + 恢复跟随）
- 信息卡定位行行尾 `✕`：关闭定位（停刷新，蓝点停在最后位置），`⌖` 可重新开启
- 双指捏合：**连续预览缩放**（`transform-scale` 围绕质心跟手，零延迟），
  释放吸附最近整级 + 残留倍率 120ms 缓动回 1（无跳变）
- 单击地图：在该处落点 —— **仅定位未开启时生效**；定位中一律忽略（防误触摸
  打乱 GPS 蓝点），关闭定位后恢复

## 定位体系要点

1. **统一接口收口**：UI 侧只认 `Location { provider, lat, lon, accuracy }` +
   `open(callback) / close`，平台差异全部压进各平台实现。
2. **provider 命名 = 坐标系声明**：名字含 `gps` 视为 WGS-84（引擎自动转 GCJ-02
   贴高德瓦片）；网络 / IP 定位直用（城市级，偏移无意义）。
3. **线程模型**：定位回调发生在定位线程，`App`(Rc) 非 Send —— 回调里只把标量经
   `slint::invoke_from_event_loop` 抛回 UI 线程，从 `thread_local` 的弱引用取回 App。
4. **三个状态位**：`gps 真值`（只由定位回调写入）/ `marker 蓝点`（唯一标记）/
   `manual 冻结`（手动选点）/ `follow 跟随`（新修复自动回中），交互语义全部由此导出。
5. **Android JNI 两个坑**：app 类要用 Activity 的 ClassLoader 预加载缓存（原生线程
   `find_class` 走 system classloader 找不到）；定位回调派发到主线程 Looper。
6. **开关前置检查**：open 前先 `enabled()` 查系统定位总开关（Android），关着直接在
   读数区提示"定位不可用 · 请开启系统定位"，不空等永远不来的回调。
7. **跟随回中不清瓦片**：`center_on` 只改 offset + 补拉缺瓦片，避免每秒回中闪白；
   `pan_to` 忽略程序化设视口引起的同值回弹（否则 set → flicked → set 循环 + 跟随误关）。

## 坐标差异（重要）

| 项目 | 瓦片源 | 坐标系 | 说明 |
|---|---|---|---|
| yehun-slint 定位页 | 高德 | **GCJ-02**(国测局加密) | 中国地图"偏移"坐标系 |
| 本 demo | 高德 | **GCJ-02**(底图) | 底图同系；对外读数以 WGS-84 呈现 |

高德 / 腾讯底图统一打在 **GCJ-02** 上；而手机 GPS、标准经纬度是 **WGS-84**，两系同点
相差约 **100~700 米**。本 demo 的战略：**底图用 GCJ-02，UI 读数用 WGS-84**，转换只在
投影边界发生，用户看到的中心 / 落点始终是"标准 GPS 坐标"，不会和真实 GPS 对不上。

> 定位接入后该约定直接生效：Android GPS / 浏览器定位的 provider 名含 `gps`，
> WGS-84 坐标进引擎自动转 GCJ-02；网络 / IP 定位坐标直用。

## 已知取舍

- 缩放级限制在 **1–18**：极高缩放级世界边长超过 `f32` 整数精度，标记 / 瓦片会出现亚像素
  抖动。`yehun-slint` 生产版用「窗口化坐标（相对视口）」彻底解决，本 demo 为可读性做了简化。
  （高德道路底图原生支持到 19 级，核心城区有瓦片；想试更高精度可把 `MAX_ZOOM` 调到 19，
  但 z19 的世界像素已超出 `f32` 安全整数范围，边缘瓦片会有可见错位。）
- 拖动后的惯性动画期间，中心读数 / 落点会短暂轻微滞后（下一次交互即对齐）。
- 桌面定位走 **WiFi WPS 逐级兜底**（三平台扫 AP → Apple → beaconDB → IP，
  城市内实测几十米；`cargo test -p article-18 --features desktop -- --ignored --nocapture`
  可手动实测整条链）。三平台扫描器：Linux = NetworkManager(D-Bus)，
  Windows = wlanapi.dll `WlanGetNetworkBssList`，macOS = CoreWLAN（Sonoma 起
  触发系统定位授权，被拒时扫描为空自然落 IP）。生产版在 Linux 还有 GeoClue2
  主源（首帧超时兜底 + 粗帧 WiFi 改进），demo 未纳入。
- Android 定位监听直接注册在 `LocationManager` 上（无 Service）：前台用完即走够用；
  需要后台持续定位时再按生产版包一层 `Service`（记得在 manifest 声明）。
