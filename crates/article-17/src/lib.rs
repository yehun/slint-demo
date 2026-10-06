// 第十七篇, Slint OCR 文字识别
//
// 核心知识点:
// 1. 本地 OCR = onnxruntime 跑 PP-OCRv4 的 det/cls/rec 三件套(全在 CPU 上)
// 2. 三个 ONNX 模型全部本地推理, 不联网、不上传图片
// 3. Slint 主线程只管 UI: 模型/识别都在工作线程, 结果经 invoke_from_event_loop 回写
// 4. 检测框由 Rust 画在原图上, 整张预览图回传给 Slint 的 Image 元素
//
// 入口:
//   desktop_main —— Windows / Linux / macOS
//   android_main —— Android (cargo apk2, 需自备编译好的 libonnxruntime)

slint::include_modules!();

mod ocr;
mod state;

use std::sync::Arc;

use slint::ComponentHandle;

pub fn desktop_main() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .try_init();

    let app = MainWindow::new().expect("创建窗口失败");
    let state = Arc::new(state::AppState::new(app.as_weak()));

    ocr::bind(&app, state.clone());
    ocr::auto_discover_model(&state);

    app.run().expect("运行窗口失败");
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    slint::android::init(app).expect("init android");

    let window = MainWindow::new().expect("创建窗口失败");
    let state = Arc::new(state::AppState::new(window.as_weak()));
    ocr::bind(&window, state.clone());
    ocr::auto_discover_model(&state);

    window.run().expect("运行窗口失败");
}
