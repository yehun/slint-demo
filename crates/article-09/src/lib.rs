// 第九幕: Slint 读取摄像头
// 通过 v4l2 捕获视频帧, 在 Slint 界面中实现实时摄像头预览
//
// 核心知识点:
// 1. nokhwa —— 纯 Rust 跨平台相机库 (Linux=v4l2, Windows=MF, macOS=AVF)
// 2. CameraService trait —— 平台无关的相机服务抽象
// 3. 帧数据采集 —— 独立线程采集, 回调通知新帧
// 4. 帧节流 —— 30ms 内到达的帧直接丢弃, 避免 UI 线程过载
// 5. 水平翻转 —— 摄像头采集的画面需要镜像翻转
// 6. RGB → Slint Image —— SharedPixelBuffer::<Rgb8Pixel> + Image::from_rgb8
// 7. 跨线程投递 —— upgrade_in_event_loop 把帧数据投递到 UI 线程
// 8. 状态管理 — Opening/Opened/Closing/Closed/Error 状态机
// 9. JNI 双向通路 — Rust→Java (CameraInvoke) + Java→Rust (CameraCallback)
// 10. 安全区适配 — safe-area-insets 避让系统栏

slint::include_modules!();

pub mod service;
pub mod utils;

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
pub mod desktop;

#[cfg(target_os = "android")]
pub mod android;

pub use service::{CameraFrame, CameraService, CameraState, FrameCallback, StateCallback};

// ===== 公共辅助函数 =====

fn create_image_from_bytes(bytes: &[u8], width: u32, height: u32) -> slint::Image {
    let buffer =
        slint::SharedPixelBuffer::<slint::Rgb8Pixel>::clone_from_slice(bytes, width, height);
    slint::Image::from_rgb8(buffer)
}

/// 注册帧回调和状态回调, 返回 opened Arc
fn setup_camera_callbacks(
    camera: &dyn CameraService,
    main_window: &MainWindow,
) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
    let opened = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    // 帧回调
    let win_weak = main_window.as_weak();
    camera
        .set_frame_callback(Box::new(move |frame| {
            let win_weak = win_weak.clone();
            let (data, w, h) = (frame.data, frame.width, frame.height);
            let _ = win_weak.upgrade_in_event_loop(move |app| {
                let model = app.global::<CameraModel>();
                model.set_image(create_image_from_bytes(&data, w, h));
            });
        }))
        .expect("设置帧回调失败");

    // 状态回调
    let win_weak = main_window.as_weak();
    let opened_for_state = opened.clone();
    camera
        .set_state_callback(Box::new(move |state| {
            let win_weak = win_weak.clone();
            let opened = opened_for_state.clone();
            let _ = win_weak.upgrade_in_event_loop(move |app| {
                let model = app.global::<CameraModel>();
                match state {
                    CameraState::Opening => {
                        model.set_opened(false);
                        model.set_open_state(false);
                    }
                    CameraState::Opened => {
                        opened.store(true, std::sync::atomic::Ordering::SeqCst);
                        model.set_opened(true);
                        model.set_open_state(true);
                    }
                    CameraState::Closing => {
                        model.set_opened(false);
                        model.set_open_state(false);
                    }
                    CameraState::Closed | CameraState::Error => {
                        opened.store(false, std::sync::atomic::Ordering::SeqCst);
                        model.set_opened(false);
                        model.set_open_state(true);
                    }
                }
            });
        }))
        .expect("设置状态回调失败");

    opened
}

/// 注册 UI 回调 (打开/关闭/切换)
fn setup_ui_callbacks<T: CameraService + Send + Sync + 'static>(
    camera: std::sync::Arc<T>,
    main_window: &MainWindow,
) {
    let model = main_window.global::<CameraModel>();
    let can_switch = camera.can_switch();
    model.set_can_switch(can_switch);

    let cam_weak = camera.clone();
    model.on_open_camera(move || {
        eprintln!("[article-09] open-camera 按钮点击");
        let cam = cam_weak.clone();
        std::thread::spawn(move || {
            if let Err(e) = cam.open() {
                eprintln!("[article-09] open() 错误: {}", e);
            }
        });
    });

    let cam_weak = camera.clone();
    model.on_close_camera(move || {
        eprintln!("[article-09] close-camera 按钮点击");
        let cam = cam_weak.clone();
        if let Err(e) = cam.close() {
            eprintln!("[article-09] close() 错误: {}", e);
        }
    });

    if can_switch {
        let cam_weak = camera.clone();
        model.on_switch_camera(move || {
            eprintln!("[article-09] switch-camera 按钮点击");
            let cam = cam_weak.clone();
            if let Err(e) = cam.switch() {
                eprintln!("[article-09] switch() 错误: {}", e);
            }
        });
    }
}

/// 设置安全区 — 在 .slint 中通过 Window 的 safe-area-insets 属性绑定

// ===== Desktop 入口 =====

#[cfg(feature = "desktop")]
pub fn desktop_main() {
    let main_window = MainWindow::new().expect("创建主窗口失败");
    let camera = desktop::PlatformCameraService::new();

    setup_camera_callbacks(&camera, &main_window);

    let camera = std::sync::Arc::new(camera);
    setup_ui_callbacks(camera, &main_window);

    main_window.show().expect("显示窗口失败");
    slint::run_event_loop().expect("事件循环异常");
}

// ===== Android 入口 =====

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    // 1. 保存 AndroidApp 引用供 JNI 调用使用
    android::init(app.clone());

    // 2. 初始化 Slint Android 平台
    slint::android::init(app).expect("Slint Android 初始化失败");

    // 3. 缓存 Java 类全局引用 (必须在主线程调用)
    android::with_env(|env| {
        android::cache_classes(env)
    }).expect("缓存 Java 类失败");

    // 4. 创建窗口
    let main_window = MainWindow::new().expect("创建主窗口失败");
    let camera = android::PlatformCameraService::new();

    // 5. 注册回调 (安全区在 .slint 中通过 root.safe-area-insets 绑定)
    setup_camera_callbacks(&camera, &main_window);
    let camera = std::sync::Arc::new(camera);
    setup_ui_callbacks(camera, &main_window);

    // 7. 显示窗口
    main_window.show().expect("显示窗口失败");

    // 8. 运行事件循环
    slint::run_event_loop().expect("事件循环异常");
}
