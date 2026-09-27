// 第十六篇, Slint 语音克隆
//
// 核心知识点:
// 1. 零样本语音克隆 = 参考音频编码成 prompt + 文本 G2P 成音素 → 流匹配解码 → 声码器
// 2. 三个 ONNX 模型全部本地推理, 不联网、不上传声音
// 3. Slint 主线程只管 UI: 模型/合成都在工作线程, 结果经 invoke_from_event_loop 回写
// 4. 合成结果是内存里的 Vec<f32>, rodio 的 SamplesBuffer 直接播, 保存时才编码 WAV
//
// 入口:
//   desktop_main —— Windows / Linux / macOS
//   android_main —— Android (cargo apk2, 需自备编译好的 libonnxruntime)

slint::include_modules!();

mod clone;
mod player;
mod state;

use std::sync::Arc;
use std::time::Duration;

use slint::{ComponentHandle, Timer, TimerMode};
use state::AppState;

pub fn desktop_main() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .try_init();

    let app = MainWindow::new().expect("创建窗口失败");
    let state = Arc::new(AppState::new(app.as_weak()));

    clone::bind(&app, state.clone());
    player::bind(&app, state.clone());
    start_busy_progress(app.as_weak(), state.clone());
    clone::auto_discover_model(&state);

    app.run().expect("运行窗口失败");
}

/// 合成没法给出精确百分比(流匹配是固定步数迭代), 用循环进度条表示"还在跑"
fn start_busy_progress(weak: slint::Weak<MainWindow>, state: Arc<AppState>) {
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(120), move || {
        if !state.is_busy() {
            return;
        }
        if let Some(app) = weak.upgrade() {
            let m = app.global::<CloneModel>();
            let next = m.get_progress() + 0.08;
            m.set_progress(if next > 1.0 { 0.0 } else { next });
        }
    });
    // Timer 一被 drop 回调就停, 必须让它活过这个函数(thread_local 保管到进程结束)
    BUSY_TIMER.with(|cell| *cell.borrow_mut() = Some(timer));
}

thread_local! {
    static BUSY_TIMER: std::cell::RefCell<Option<Timer>> = const { std::cell::RefCell::new(None) };
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    slint::android::init(app).expect("init android");

    let window = MainWindow::new().expect("创建窗口失败");
    let state = Arc::new(AppState::new(window.as_weak()));
    clone::bind(&window, state.clone());
    player::bind(&window, state.clone());
    start_busy_progress(window.as_weak(), state.clone());
    clone::auto_discover_model(&state);

    window.run().expect("运行窗口失败");
}
