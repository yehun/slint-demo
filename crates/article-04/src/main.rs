// 第四幕: 窗口居中与多显示器适配 — desktop 入口

slint::include_modules!();

use i_slint_backend_winit::WinitWindowAccessor;
use i_slint_backend_winit::winit::dpi::PhysicalPosition;
use i_slint_backend_winit::winit::event::WindowEvent;
use i_slint_backend_winit::EventResult;

fn main() {
    let window = MainWindow::new().expect("创建窗口失败");
    register_window_callbacks(&window);
    window.show().expect("显示窗口失败");

    // 居中窗口 (启动时忽略错误)
    let _ = center_window(window.window());

    // 打印并填充显示器信息
    print_monitors(&window);
    let info = query_monitors_string(window.window());
    window.set_monitor_info(info);

    // 注册 off-screen 检测
    init_offscreen_guard(&window);

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

    // 居中窗口
    let ww = window.as_weak();
    window.on_center_window(move || {
        if let Some(w) = ww.upgrade() {
            match center_window(w.window()) {
                Ok(()) => w.set_status_text("窗口已居中".into()),
                Err(msg) => w.set_status_text(msg.into()),
            }
        }
    });

    // 恢复到主显示器
    let ww = window.as_weak();
    window.on_recover_window(move || {
        if let Some(w) = ww.upgrade() {
            match recover_to_primary(w.window()) {
                Ok(()) => w.set_status_text("已恢复到主显示器".into()),
                Err(msg) => w.set_status_text(msg.into()),
            }
        }
    });

    // 查询显示器信息
    let ww = window.as_weak();
    window.on_query_monitors(move || {
        if let Some(w) = ww.upgrade() {
            query_monitors_string(w.window())
        } else {
            slint::SharedString::default()
        }
    });
}

/// 窗口居中: 在当前显示器上居中
/// 返回 Ok(()) 成功, Err(String) 失败原因
fn center_window(window: &slint::Window) -> Result<(), String> {
    let is_wayland = std::env::var("XDG_SESSION_TYPE")
        .map(|v| v == "wayland")
        .unwrap_or(false);

    if is_wayland {
        return Err("Wayland 不支持应用设置窗口位置, 窗口位置由合成器控制".into());
    }

    window.with_winit_window(|win| {
        if let Some(monitor) = win.current_monitor() {
            let ms = monitor.size();
            let mp = monitor.position();
            let ws = win.outer_size();
            let x = mp.x + (ms.width as i32 - ws.width as i32) / 2;
            let y = mp.y + (ms.height as i32 - ws.height as i32) / 2;
            win.set_outer_position(PhysicalPosition::new(x, y));
            Ok(())
        } else {
            Err("无法获取当前显示器信息".into())
        }
    }).unwrap_or(Err("无法访问底层窗口".into()))
}

/// 恢复到主显示器居中(用于窗口跑丢后的恢复)
fn recover_to_primary(window: &slint::Window) -> Result<(), String> {
    let is_wayland = std::env::var("XDG_SESSION_TYPE")
        .map(|v| v == "wayland")
        .unwrap_or(false);

    if is_wayland {
        return Err("Wayland 不支持应用设置窗口位置, 窗口位置由合成器控制".into());
    }

    window.with_winit_window(|win| {
        if let Some(monitor) = win.primary_monitor() {
            let ms = monitor.size();
            let mp = monitor.position();
            let ws = win.outer_size();
            let x = mp.x + (ms.width as i32 - ws.width as i32) / 2;
            let y = mp.y + (ms.height as i32 - ws.height as i32) / 2;
            win.set_outer_position(PhysicalPosition::new(x, y));
            Ok(())
        } else {
            Err("无法获取主显示器信息".into())
        }
    }).unwrap_or(Err("无法访问底层窗口".into()))
}

/// 在指定显示器上居中 (monitor_index 从 0 开始)
#[allow(dead_code)]
fn center_on_monitor(window: &slint::Window, monitor_index: usize) {
    window.with_winit_window(|win| {
        if let Some(monitor) = win.available_monitors().nth(monitor_index) {
            let ms = monitor.size();
            let mp = monitor.position();
            let ws = win.outer_size();
            let x = mp.x + (ms.width as f32 / 2.0 - ws.width as f32 / 2.0) as i32;
            let y = mp.y + (ms.height as f32 / 2.0 - ws.height as f32 / 2.0) as i32;
            win.set_outer_position(PhysicalPosition::new(x, y));
        }
    });
}

/// 检测窗口是否在屏幕内
fn is_window_on_screen(window: &slint::Window) -> bool {
    window
        .with_winit_window(|win| {
            let win_pos = win.outer_position().ok()?;
            let win_size = win.outer_size();

            let win_left = win_pos.x;
            let win_top = win_pos.y;
            let win_right = win_pos.x + win_size.width as i32;
            let win_bottom = win_pos.y + win_size.height as i32;
            let win_area = win_size.width as i64 * win_size.height as i64;

            if win_area == 0 {
                return Some(true);
            }

            let mut intersection_area: i64 = 0;
            for monitor in win.available_monitors() {
                let m_pos = monitor.position();
                let m_size = monitor.size();

                let m_left = m_pos.x;
                let m_top = m_pos.y;
                let m_right = m_pos.x + m_size.width as i32;
                let m_bottom = m_pos.y + m_size.height as i32;

                let ix = win_left.max(m_left);
                let iy = win_top.max(m_top);
                let ix2 = win_right.min(m_right);
                let iy2 = win_bottom.min(m_bottom);

                if ix < ix2 && iy < iy2 {
                    intersection_area += (ix2 - ix) as i64 * (iy2 - iy) as i64;
                }
            }

            // 窗口面积的 30% 以上在屏幕上, 就算"在屏幕内"
            Some(intersection_area * 100 / win_area > 30)
        })
        .flatten()
        .unwrap_or(true)
}

/// off-screen 检测: 窗口移出屏幕时自动恢复到主显示器
fn init_offscreen_guard(window: &MainWindow) {
    let ww = window.as_weak();
    i_slint_backend_winit::WinitWindowAccessor::on_winit_window_event(
        window.window(),
        move |_win, event| {
            match event {
                WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                    if let Some(w) = ww.upgrade() {
                        if !is_window_on_screen(w.window()) {
                            let _ = recover_to_primary(w.window());
                        }
                    }
                }
                _ => {}
            }
            EventResult::Propagate
        },
    );
}

/// 打印所有显示器信息到 stdout
fn print_monitors(window: &MainWindow) {
    i_slint_backend_winit::WinitWindowAccessor::with_winit_window(window.window(), |win| {
        println!("=== 显示器信息 ===");
        for (i, monitor) in win.available_monitors().enumerate() {
            let pos = monitor.position();
            let size = monitor.size();
            let scale = monitor.scale_factor();
            let name = monitor.name().unwrap_or_default();
            println!(
                "  显示器 {}: \"{}\"  位置=({},{})  尺寸={}×{}  缩放={:.1}",
                i, name, pos.x, pos.y, size.width, size.height, scale
            );
        }
        println!("==================");
    });
}

/// 查询所有显示器信息, 返回格式化字符串供 UI 显示
fn query_monitors_string(window: &slint::Window) -> slint::SharedString {
    let mut result = String::new();
    i_slint_backend_winit::WinitWindowAccessor::with_winit_window(window, |win| {
        for (i, monitor) in win.available_monitors().enumerate() {
            let pos = monitor.position();
            let size = monitor.size();
            let scale = monitor.scale_factor();
            let name = monitor.name().unwrap_or_default();
            let current = win
                .current_monitor()
                .and_then(|cm| cm.name())
                .map(|n| n == name)
                .unwrap_or(false);
            let marker = if current { " ← 当前" } else { "" };
            result.push_str(&format!(
                "#{} \"{}\"  {}×{}px  缩放:{:.1}  位置:({},{}){}\n",
                i, name, size.width, size.height, scale, pos.x, pos.y, marker
            ));
        }
    });
    if result.is_empty() {
        "无法获取显示器信息".into()
    } else {
        result.into()
    }
}
