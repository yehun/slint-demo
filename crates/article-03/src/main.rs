// 第三幕: 自定义无头窗口 — desktop 入口

slint::include_modules!();

fn main() {
    let window = MainWindow::new().expect("创建窗口失败");
    register_window_callbacks(&window);
    window.show().expect("显示窗口失败");
    center_window(window.window());
    slint::run_event_loop().expect("事件循环异常");
}

/// 注册所有窗口操作回调
fn register_window_callbacks(window: &MainWindow) {
    let ww = window.as_weak();

    // 关闭
    window.on_close_window(move || {
        if let Some(w) = ww.upgrade() {
            w.window()
                .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        }
    });

    // 最小化
    let ww = window.as_weak();
    window.on_min_window(move |minimized| {
        if let Some(w) = ww.upgrade() {
            w.window().set_minimized(minimized);
        }
    });

    // 最大化 / 还原
    let ww = window.as_weak();
    window.on_max_window(move |maximized| {
        if let Some(w) = ww.upgrade() {
            w.window().set_maximized(maximized);
            w.set_is_maximized(maximized);
        }
    });

    // 拖动窗口
    let ww = window.as_weak();
    window.on_move_window(move || {
        if let Some(w) = ww.upgrade() {
            i_slint_backend_winit::WinitWindowAccessor::with_winit_window(
                w.window(),
                |win| {
                    if win.is_maximized() {
                        win.set_maximized(false);
                    }
                    let _ = win.drag_window();
                },
            );
        }
    });
}

/// 窗口居中
fn center_window(window: &slint::Window) {
    use i_slint_backend_winit::WinitWindowAccessor;
    use i_slint_backend_winit::winit::dpi::PhysicalPosition;

    window.with_winit_window(|win| {
        if let Some(monitor) = win.current_monitor() {
            let ms = monitor.size();
            let ws = win.outer_size();
            let x = (ms.width as f32 / 2.0 - ws.width as f32 / 2.0) as i32;
            let y = (ms.height as f32 / 2.0 - ws.height as f32 / 2.0) as i32;
            win.set_outer_position(PhysicalPosition::new(x, y));
        }
    });
}
