// 第五幕: Slint基于JNI对接Android API交互
slint::include_modules!();

// 条件编译: 根据平台选择实现模块
#[cfg(target_os = "android")]
mod android;
#[cfg(not(target_os = "android"))]
#[allow(dead_code)]
mod desktop;

// 导出平台相关的函数
#[cfg(target_os = "android")]
use android::{get_battery_info, get_device_info, get_network_info, show_toast};
#[cfg(not(target_os = "android"))]
use desktop::{get_battery_info, get_device_info, get_network_info, show_toast};

// ===== 反向 JNI: Java 调用 Rust =====
// Java 侧定义 NativeBridge.onEventFromJava(String name, String value),
// 通过 JNI 通知 Rust 端更新 UI.

/// 处理来自 Java 侧的事件, 更新 Slint UI
#[allow(dead_code)]
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

/// JNI 导出函数: Java 侧 NativeBridge.onEventFromJava(String, String) 会调用此函数
///
/// 函数名格式: Java_包名_类名_方法名 (包名中 . 替换为 _)
/// 对应 Java: com.example.article_05.NativeBridge.onEventFromJava(String name, String value)
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
fn Java_com_example_article_05_NativeBridge_onEventFromJava(
    mut unowned_env: jni::EnvUnowned,
    _class: jni::objects::JClass,
    name: jni::objects::JString,
    value: jni::objects::JString,
) {
    // EnvUnowned 是 FFI 安全的裸指针包装
    // with_env() 接收闭包, 在闭包内提供完整的 Env 访问
    let _ = unowned_env.with_env(|env| {
        let name: String = env.cast_local::<jni::objects::JString>(name)
            .map(|s| s.to_string())
            .unwrap_or_default();
        let value: String = env.cast_local::<jni::objects::JString>(value)
            .map(|s| s.to_string())
            .unwrap_or_default();

        eprintln!("[article-05] Java→Rust 事件: name={}, value={}", name, value);

        // 注意: 此函数运行在 Java 调用线程, 更新 UI 需要通过 invoke_from_event_loop 回到主线程
        // 这里简化处理, 仅打印日志. 实际 UI 更新见 register_java_event_callback

        Ok::<(), jni::errors::Error>(())
    });
}

/// 注册 Java 事件回调 (供 Rust 侧测试反向调用流程)
#[allow(dead_code)]
#[cfg(target_os = "android")]
fn register_java_event_callback(window: &DeviceInfoWindow) {
    let ww = window.as_weak();
    window.on_simulate_java_event(move || {
        // 模拟 Java 侧调用 Rust (演示反向 JNI 的数据流)
        handle_java_event(&ww, "toast", "Hello from Java!");
    });
}

#[allow(dead_code)]
#[cfg(not(target_os = "android"))]
fn register_java_event_callback(window: &DeviceInfoWindow) {
    let ww = window.as_weak();
    window.on_simulate_java_event(move || {
        // Desktop 端模拟 Java 调用
        handle_java_event(&ww, "toast", "Hello from Java! (模拟)");
    });
}

// ===== Android 入口 =====

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    // 1. 先保存 AndroidApp 引用供 JNI 调用使用
    android::init(app.clone());

    // 2. 初始化 Slint Android 平台
    slint::android::init(app).expect("Slint Android 初始化失败");

    // 3. 创建窗口并注册回调
    let window = DeviceInfoWindow::new().expect("创建窗口失败");
    register_window_callbacks(&window);
    register_java_event_callback(&window);

    // 4. 显示窗口
    window.show().expect("显示窗口失败");

    // 5. 延迟到主线程事件循环中请求权限
    // android_main 运行在 native 线程 (无 Looper), Toast/权限请求必须回主线程
    slint::invoke_from_event_loop(|| {
        android::permission::ensure_permissions();
    }).expect("invoke_from_event_loop failed");

    // 6. 运行事件循环
    slint::run_event_loop().expect("事件循环异常");
}

/// 注册所有窗口操作回调 (Android)
#[cfg(target_os = "android")]
fn register_window_callbacks(window: &DeviceInfoWindow) {
    register_platform_callbacks(window);
}

/// 平台通用的回调注册
#[allow(dead_code)]
fn register_platform_callbacks(window: &DeviceInfoWindow) {
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
