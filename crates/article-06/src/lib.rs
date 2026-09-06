// 第六幕: 基于Slint官方Material建立Android主题体系
// Android 入口 + Rust 端主题切换

mod theme;

slint::include_modules!();

use std::sync::OnceLock;

/// 保存主窗口的弱引用, 供 Rust 端主题切换使用
static WINDOW_WEAK: OnceLock<slint::Weak<MainWindow>> = OnceLock::new();

/// Rust 端设置主题
pub fn set_theme(index: i32) {
    if let Some(weak) = WINDOW_WEAK.get() {
        slint::invoke_from_event_loop(move || {
            if let Some(window) = weak.upgrade() {
                window.invoke_set_theme(index);
            }
        }).unwrap();
    }
}


pub fn desktop_main() {
    let main_window = MainWindow::new().expect("创建主窗口失败");

    // 初始化主题系统 (加载 JSON, 设置主题名, 应用默认主题)
    theme::init(&main_window);

    // 注册保存设置回调
    let ww = main_window.as_weak();
    main_window.on_save_settings(move || {
        if let Some(w) = ww.upgrade() {
            w.set_status_text(
                format!("Desktop 模拟保存: 用户名={}", w.get_username()).into()
            );
        }
    });

    main_window.run().expect("运行事件循环失败");
}


#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    // 初始化 Slint Android 平台
    slint::android::init(app).expect("Slint Android 初始化失败");

    let main_window = MainWindow::new().expect("创建主窗口失败");

    // 初始化全局窗口引用 (供 set_theme 使用)
    let _ = WINDOW_WEAK.set(main_window.as_weak());

    // 初始化主题系统
    theme::init(&main_window);

    // 注册保存设置回调
    let ww = main_window.as_weak();
    main_window.on_save_settings(move || {
        if let Some(w) = ww.upgrade() {
            w.set_status_text("Android 端: 设置已保存".into());
        }
    });

    main_window.show().expect("显示窗口失败");
    slint::run_event_loop().expect("事件循环异常");
}
