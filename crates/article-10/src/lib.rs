// 第十幕: Slint 剪贴板读写
// 跨平台文本复制粘贴, 桌面端 arboard + Android ClipboardManager
//
// 核心知识点:
// 1. arboard —— 纯 Rust 跨平台剪贴板库
// 2. ClipboardService trait —— 平台无关的剪贴板抽象
// 3. Android ClipboardManager —— 系统服务 + JNI 调用
// 4. ClipData / ClipDescription —— Android 剪贴板数据模型
// 5. 主线程约束 —— Android 剪贴板操作必须在 UI 线程执行

slint::include_modules!();

pub mod service;

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
pub mod desktop;

#[cfg(target_os = "android")]
pub mod android;

pub use service::ClipboardService;

// ===== 公共辅助: 注册 UI 回调 =====

fn setup_ui_callbacks(
    clipboard: std::sync::Arc<dyn ClipboardService>,
    main_window: &MainWindow,
) {
    let win_weak = main_window.as_weak();

    // --- 复制按钮 ---
    let clip_weak = std::sync::Arc::downgrade(&clipboard);
    main_window.global::<ClipboardModel>().on_copy(move || {
        let Some(clip) = clip_weak.upgrade() else { return };
        let Some(win) = win_weak.upgrade() else { return };
        let text = win.global::<ClipboardModel>().get_input_text().to_string();
        match clip.set_text(&text) {
            Ok(_) => win.global::<ClipboardModel>().set_status("已复制到剪贴板".into()),
            Err(e) => win.global::<ClipboardModel>().set_status(format!("复制失败: {e}").into()),
        }
    });

    // --- 粘贴按钮 ---
    let clip_weak = std::sync::Arc::downgrade(&clipboard);
    let win_weak = main_window.as_weak();
    main_window.global::<ClipboardModel>().on_paste(move || {
        let Some(clip) = clip_weak.upgrade() else { return };
        let Some(win) = win_weak.upgrade() else { return };
        match clip.get_text() {
            Ok(text) => {
                win.global::<ClipboardModel>().set_input_text(text.into());
                win.global::<ClipboardModel>().set_status("已从剪贴板粘贴".into());
            }
            Err(e) => win.global::<ClipboardModel>().set_status(format!("粘贴失败: {e}").into()),
        }
    });

    // --- 清空按钮 ---
    let clip_weak = std::sync::Arc::downgrade(&clipboard);
    let win_weak = main_window.as_weak();
    main_window.global::<ClipboardModel>().on_clear(move || {
        let Some(clip) = clip_weak.upgrade() else { return };
        let Some(win) = win_weak.upgrade() else { return };
        match clip.set_text("") {
            Ok(_) => win.global::<ClipboardModel>().set_status("已清空剪贴板".into()),
            Err(e) => win.global::<ClipboardModel>().set_status(format!("清空失败: {e}").into()),
        }
    });
}

// ===== Desktop 入口 =====

#[cfg(feature = "desktop")]
pub fn desktop_main() {
    let main_window = MainWindow::new().expect("创建主窗口失败");
    // Arc 必须存活整个程序生命周期, 否则闭包里 upgrade 会失败
    let clipboard: std::sync::Arc<dyn ClipboardService> =
        std::sync::Arc::new(desktop::PlatformClipboardService::new());
    let clipboard_for_ui = clipboard.clone();

    setup_ui_callbacks(clipboard_for_ui, &main_window);

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
    android::with_env(|env| android::cache_classes(env)).expect("缓存 Java 类失败");

    // 4. 创建窗口
    let main_window = MainWindow::new().expect("创建主窗口失败");
    // Arc 必须存活整个程序生命周期, 否则闭包里 upgrade 会失败
    let clipboard: std::sync::Arc<dyn ClipboardService> =
        std::sync::Arc::new(android::PlatformClipboardService::new());
    let clipboard_for_ui = clipboard.clone();

    // 5. 注册回调
    setup_ui_callbacks(clipboard_for_ui, &main_window);

    // 6. 显示窗口
    main_window.show().expect("显示窗口失败");

    // 7. 运行事件循环
    slint::run_event_loop().expect("事件循环异常");
}
