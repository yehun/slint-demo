// 第五幕: Slint基于JNI对接Android API交互 — Desktop 入口

slint::include_modules!();

// Desktop 模块
mod desktop;
use desktop::{get_battery_info, get_device_info, get_network_info, show_toast};

fn main() {
    let window = DeviceInfoWindow::new().expect("创建窗口失败");
    register_window_callbacks(&window);
    register_java_event_callback(&window);
    window.show().expect("显示窗口失败");

    slint::run_event_loop().expect("事件循环异常");
}

/// 处理来自 Java 侧的事件, 更新 Slint UI
fn handle_java_event(ui_weak: &slint::Weak<DeviceInfoWindow>, name: &str, value: &str) {
    if let Some(w) = ui_weak.upgrade() {
        match name {
            "toast" => {
                w.set_status_text(format!("Java→Rust: Toast '{}'", value).into());
                w.set_device_info(format!("收到 Java 事件: toast = {}", value).into());
            }
            "battery" => {
                w.set_battery_info(format!("Java 推送电量: {}", value).into());
                w.set_status_text("收到 Java 电池推送".into());
            }
            _ => {
                w.set_status_text(format!("Java→Rust 未知事件: {}", name).into());
            }
        }
    }
}

/// 注册模拟 Java 事件回调 (Desktop 端)
fn register_java_event_callback(window: &DeviceInfoWindow) {
    let ww = window.as_weak();
    window.on_simulate_java_event(move || {
        handle_java_event(&ww, "toast", "Hello from Java! (模拟)");
    });
}

/// 注册所有窗口操作回调
fn register_window_callbacks(window: &DeviceInfoWindow) {
    // 获取设备信息
    let ww = window.as_weak();
    window.on_get_device_info(move || {
        if let Some(w) = ww.upgrade() {
            let info = get_device_info();
            w.set_device_info(info.into());
            w.set_status_text("设备信息已获取".into());
        }
    });

    // 获取电池信息
    let ww = window.as_weak();
    window.on_get_battery_info(move || {
        if let Some(w) = ww.upgrade() {
            let info = get_battery_info();
            w.set_battery_info(info.into());
            w.set_status_text("电池信息已获取".into());
        }
    });

    // 获取网络信息
    let ww = window.as_weak();
    window.on_get_network_info(move || {
        if let Some(w) = ww.upgrade() {
            let info = get_network_info();
            w.set_network_info(info.into());
            w.set_status_text("网络信息已获取".into());
        }
    });

    // 弹出 Toast
    let ww = window.as_weak();
    window.on_show_toast(move || {
        if let Some(w) = ww.upgrade() {
            let result = show_toast("Hello from Slint!");
            w.set_status_text(result.into());
        }
    });
}
