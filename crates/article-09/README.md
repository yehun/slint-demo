# 第九幕: Slint 读取摄像头

通过 v4l2 捕获视频帧, 在 Slint 界面中实现实时摄像头预览.

## 功能

- **Desktop 端** (Linux/Windows/macOS): 使用 nokhwa 库采集摄像头帧
- **Android 端**: 使用 Camera2 API + JNI 双向桥接
- 实时预览: RGB 帧数据 → SharedPixelBuffer → Slint Image
- 状态管理: Opening/Opened/Closing/Closed/Error
- 水平翻转 (镜像效果)
- 帧节流 (~30fps)
- 前后摄切换 (Android)

## 目录结构

```
article-09/
├── ui/
│   └── app.slint                         # 界面定义 (MainWindow + CameraModel 全局)
├── src/
│   ├── main.rs                           # Desktop 入口
│   ├── lib.rs                            # Desktop + Android 入口
│   ├── service.rs                        # CameraService trait + 类型定义
│   ├── desktop.rs                        # Desktop 端实现 (nokhwa)
│   ├── utils.rs                          # 水平翻转等工具函数
│   └── android/
│       ├── mod.rs                        # Android 模块入口 (JNI 环境管理)
│       ├── api.rs                        # JNI 调用: Rust → Java (CameraInvoke)
│       ├── callback.rs                   # JNI 回调: Java → Rust (CameraCallback)
│       └── platform.rs                   # Android 平台相机服务
├── android/java/com/example/article_09/
│   ├── MainActivity.java                 # Android 入口 + 回调胶水
│   └── camera/
│       ├── CameraService.java            # 相机服务编排层
│       ├── CameraConfigManager.java      # Camera2 API 底层封装
│       ├── CameraImageService.java       # 帧图像处理 (YUV→RGB→旋转→镜像)
│       ├── ImageConversion.java          # YUV→RGB 整数运算 + 旋转/镜像
│       ├── CameraCallback.java           # JNI 回调声明 (Java native 方法)
│       ├── CameraInvoke.java             # JNI 调用转发 (Rust→Java)
│       ├── CameraCallbacks.java          # 回调接口定义
│       ├── CameraPermissionService.java  # 相机权限管理
│       └── BufferPool.java               # 字节数组对象池 (防 GC 抖动)
├── Cargo.toml
├── build.rs
└── Makefile
```

## 入口

| 平台 | 入口 | 实现 |
|---|---|---|
| Desktop | `src/main.rs` → `desktop_main()` | nokhwa 真实采集 |
| Android | `src/lib.rs` → `android_main()` | Camera2 + JNI 双向桥接 |

## 环境要求

- Rust 1.80+
- Slint master 分支
- Linux: v4l2 驱动 + `/dev/video0`
- Windows: MediaFoundation
- macOS: AVFoundation
- Android: NDK 28+ + JAVA_HOME=Java 21

## 运行

```bash
# Desktop
make run

# 热重载预览
make preview

# Android APK
make build-apk

# Android 安装并运行
make run-android
```

## 核心架构

```
Slint UI (Image 元素)
  ↕ image 属性 (SharedPixelBuffer → Image)
UI 绑定层 (upgrade_in_event_loop)
  ↕ CameraFrame { data, width, height }
CameraService trait (平台无关)
  ↕
平台实现:
  desktop: nokhwa (v4l2/MF/AVF)
  android: Camera2 + JNI 双向桥接
```

## Android 端数据流

```
Rust ──JNI 调用──→ Java ──Camera2──→ 摄像头
                      │
摄像头 ──YUV 帧──→ Java ──JNI 回调──→ Rust
```

### 完整帧路径 (9 步)

```
摄像头传感器
  ↓ YUV_420_888
ImageReader (Java)
  ↓ OnImageAvailableListener
CameraImageService.processImage() (Java)
  ↓ YUV→RGB 转换 + 旋转 + 镜像
CameraCallbacks.OnFrameCallback.onFrame() (Java)
  ↓
CameraCallback.onCameraFrame() (Java native)
  ↓ JNI
Java_com_example_article_09_camera_CameraCallback_onCameraFrame() (Rust)
  ↓ convert_byte_array → CameraFrame
全局回调分发 (Rust)
  ↓
PlatformCameraService 帧回调 (Rust)
  ↓ upgrade_in_event_loop
CameraModel.set_image() (Slint)
  ↓
Image 元素渲染
```

### JNI 双向通路

**Rust → Java (调用)**:
```rust
// 通过 jni_str!/jni_sig! 调用 CameraInvoke 静态方法
let cls = env.find_class(jni_str!("com/example/article_09/camera/CameraInvoke"))?;
env.call_static_method(&cls, jni_str!("open"), jni_sig!(() -> void), &[])?;
```

**Java → Rust (回调)**:
```rust
// 通过 #[unsafe(no_mangle)] 导出 C 符号
#[unsafe(no_mangle)]
pub fn Java_com_example_article_09_camera_CameraCallback_onCameraFrame(
    mut env_unowned: EnvUnowned,
    _class: JClass,
    data: JByteArray,
    width: i32,
    height: i32,
) { ... }
```

### 关键优化

1. **BufferPool**: 字节数组对象池, GC 频率从 5-6次/秒 降到 1-2次/分钟
2. **整数运算 YUV→RGB**: BT.601 整数公式, 比浮点快 ~3x
3. **ThreadLocal<ConversionContext>**: 每线程独享缓冲区, 零竞争
4. **双层节流**: Java 层 33ms + Rust 层 30ms
5. **有界等待**: `join_previous_thread()` 最长 1s

## 踩坑记录

1. **jni_str!/jni_sig! 宏**: `find_class`/`call_static_method` 不能直接用 `&str`, 必须用宏
2. **JNI 函数命名**: `Java_包名_类名_方法名` (包名 `.` 替换为 `_`, 类名不含 `package`)
3. **'static 闭包**: `set_callback` 的闭包需要 `'static`, 共享状态用 `Arc<AtomicBool>`
4. **EnvUnowned::with_env**: JNI 回调运行在 Java 线程, 必须用此方法获取 JNI 环境
5. **JAVA_HOME=Java 21**: D8 dex 编译器需要 Java 11+, 系统默认 Java 8 报 `UnsupportedClassVersionError`
6. **--no-default-features**: `default = ["desktop"]` 会干扰 Android 构建, 必须关闭
7. **android.rs vs android/mod.rs**: 不能同时存在, 目录模块优先

## 平台支持

| 平台 | 状态 | 说明 |
|---|---|---|
| Linux | ✅ | v4l2 通过 nokhwa |
| Windows | ✅ | MediaFoundation 通过 nokhwa |
| macOS | ✅ | AVFoundation 通过 nokhwa |
| Android | ✅ | Camera2 + JNI 双向桥接 |
| WASM | ❌ | 可用 getUserMedia 实现 |

## 系列文章

[github.com/yehun/slint-demo](https://github.com/yehun/slint-demo)
