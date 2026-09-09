// 第八幕: SystemTrayIcon / 系统托盘图标
// 在系统托盘中添加图标和菜单, 打造常驻后台的跨平台桌面应用
//
// 核心知识点:
// 1. SystemTrayIcon — 内置托盘图标元素
//    - 必须是导出组件的根元素 (不能作为 Window 的子元素)
//    - 只能包含 Menu 子元素
//    - 几何属性 (x/y/width/height) 无效
// 2. 属性: icon / tooltip / title / visible / color_scheme
// 3. 回调: clicked (左键点击)
// 4. 菜单: Menu > MenuItem / MenuSeparator / 嵌套 Menu
// 5. 平台后端: Linux=ksni, macOS=AppKit, Windows=Shell_NotifyIconW
// 6. 隐藏到托盘: Window.hide() + tray 的 keepalive 保持事件循环
// 7. 关闭拦截: window().on_close_requested 返回 CloseRequestResponse::HideWindow
// 8. 托盘闪烁: Timer 定时切换 flash-state, 配合 flashing 属性控制启停

slint::include_modules!();

// ===== Desktop 入口 =====

#[cfg(feature = "desktop")]
pub fn desktop_main() {
    let main_window = MainWindow::new().expect("创建主窗口失败");
    let tray = TrayIcon::new().expect("创建托盘图标失败");

    // --- 窗口隐藏到托盘 ---
    let win_weak = main_window.as_weak();
    main_window.on_hide_to_tray(move || {
        if let Some(w) = win_weak.upgrade() {
            w.hide().expect("隐藏窗口失败");
            w.set_in_tray(true);
            w.set_log_text("窗口已隐藏到托盘. 点击托盘图标恢复.".into());
        }
    });

    // --- 退出应用 ---
    tray.on_quit(|| {
        slint::quit_event_loop().expect("退出事件循环失败");
    });
    main_window.on_quit(|| {
        slint::quit_event_loop().expect("退出事件循环失败");
    });

    // --- 关闭请求拦截: 点 X 按钮时隐藏到托盘, 不退出 ---
    let win_weak = main_window.as_weak();
    main_window.window().on_close_requested(move || {
        if let Some(w) = win_weak.upgrade() {
            w.hide().expect("隐藏窗口失败");
            w.set_in_tray(true);
            w.set_log_text("关闭按钮被拦截 → 隐藏到托盘.".into());
        }
        slint::CloseRequestResponse::HideWindow
    });

    // ===== 消息闪烁效果 =====

    let tray_weak = tray.as_weak();
    let win_weak = main_window.as_weak();

    // 托盘点击 / 菜单 "显示/隐藏" 切换窗口
    tray.on_toggle_window(move || {
        if let Some(w) = win_weak.upgrade() {
            let is_visible = w.window().is_visible();
            if is_visible {
                w.hide().expect("隐藏窗口失败");
                w.set_in_tray(true);
                w.set_log_text("窗口已隐藏到托盘.".into());
            } else {
                w.show().expect("显示窗口失败");
                w.window().request_redraw();
                w.set_in_tray(false);
                w.set_has_unread(false);
                w.set_log_text("窗口已恢复, 未读消息已清除.".into());
                if let Some(t) = tray_weak.upgrade() {
                    t.set_flashing(false);
                    t.set_flash_state(false);
                }
            }
        }
    });

    // 模拟来消息: 启动闪烁
    let tray_weak = tray.as_weak();
    let win_weak = main_window.as_weak();
    main_window.on_simulate_message(move || {
        if let Some(w) = win_weak.upgrade() {
            w.set_has_unread(true);
            w.set_log_text("📩 收到新消息! 托盘图标开始闪烁...".into());
        }
        if let Some(t) = tray_weak.upgrade() {
            t.set_flashing(true);
        }
    });

    // 取消闪烁: 停止闪烁但不影响窗口状态
    let tray_weak = tray.as_weak();
    let win_weak = main_window.as_weak();
    main_window.on_stop_flash(move || {
        if let Some(w) = win_weak.upgrade() {
            w.set_has_unread(false);
            w.set_log_text("已取消闪烁.".into());
        }
        if let Some(t) = tray_weak.upgrade() {
            t.set_flashing(false);
            t.set_flash_state(false);
        }
    });

    // 闪烁定时器: 每 500ms 切换一次相位
    let tray_weak = tray.as_weak();
    let flash_timer = slint::Timer::default();
    flash_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(500),
        move || {
            if let Some(t) = tray_weak.upgrade() {
                if t.get_flashing() {
                    t.set_flash_state(!t.get_flash_state());
                }
            }
        },
    );
    Box::leak(Box::new(flash_timer));

    main_window.show().expect("显示窗口失败");
    slint::run_event_loop().expect("事件循环异常");
}
