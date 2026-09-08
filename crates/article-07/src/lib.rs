// 第七幕: SwipeGestureHandler / 触摸手势
// 基于手势实现 Tab 左右滑动切换和图片缩放旋转
//
// 核心知识点:
// 1. SwipeGestureHandler — 内置滑动手势识别器
//    - handle_swipe_left/right/up/down: 启用方向
//    - swiped / moved / cancelled: 回调
//    - pressed_position / current_position: 位置追踪
// 2. TouchArea + pointer_event — 手动实现缩放旋转
//    - Slint 1.15 没有 ScaleRotateGestureHandler
//    - 通过 TouchArea 的 pointer_event + scroll_event 实现
//    - 属性驱动变换: scale / rotation / translate

slint::include_modules!();

// ===== Desktop 入口 =====

#[cfg(feature = "desktop")]
pub fn desktop_main() {
    let main_window = MainWindow::new().expect("创建主窗口失败");
    main_window.run().expect("运行事件循环失败");
}


// ===== Android 入口 =====

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    // 初始化 Slint Android 平台
    slint::android::init(app).expect("Slint Android 初始化失败");

    let main_window = MainWindow::new().expect("创建主窗口失败");

    main_window.show().expect("显示窗口失败");
    slint::run_event_loop().expect("事件循环异常");
}
