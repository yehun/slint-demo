// 第二幕: 地基与三端初始化
slint::include_modules!();

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    slint::android::init(app).expect("Slint Android 初始化失败");
    let window = MainWindow::new().expect("创建主窗口失败");
    window.show().expect("显示窗口失败");
    slint::run_event_loop().expect("事件循环异常");
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub async fn main() {
    console_error_panic_hook::set_once();

    // 手动设置 winit 后端, 开启 spawn_event_loop (WASM 下非阻塞事件循环)
    // 否则 run_event_loop() 会调用阻塞的 run(), 导致 "Using exceptions for control flow"
    let backend = i_slint_backend_winit::Backend::builder()
        .with_spawn_event_loop(true)
        .build()
        .expect("创建后端失败");
    slint::platform::set_platform(Box::new(backend)).unwrap();

    // 创建组件并运行 (spawn 模式下 run_event_loop 不阻塞)
    let window = MainWindow::new().expect("创建主窗口失败");
    window.show().expect("显示窗口失败");
    slint::run_event_loop().expect("事件循环异常");
}