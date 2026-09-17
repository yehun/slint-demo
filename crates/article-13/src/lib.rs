// 第十三篇, Slint开发的第一个音乐播放器
//
// 核心知识点:
// 1. rodio — 纯 Rust 音频播放: OutputStream + Sink 模型
// 2. slint::Timer — 轮询 Instant::elapsed() 更新进度
// 3. slint-file-picker — 复用第十一幕公共 crate, 音频文件多选
// 4. TappingSource — 泛型 Source 包装器, 实时捕获播放采样驱动波形
// 5. lucide-slint — SVG 图标库

slint::include_modules!();

mod player;

pub use player::{
    ensure_audio, play_track_at, samples_to_waveform, setup_playback_callbacks,
    setup_timer, PlaybackState, TappingSource,
};

#[cfg(feature = "desktop")]
pub fn desktop_main() {
    let main_window = MainWindow::new().expect("创建窗口失败");

    let state = std::sync::Arc::new(std::sync::Mutex::new(PlaybackState::new()));

    setup_playback_callbacks(&main_window, state.clone());
    setup_timer(&main_window, state.clone());

    main_window.show().expect("显示窗口失败");
    slint::run_event_loop().expect("事件循环异常");
}

// ===== Android 入口 (必须在 crate root, Android runtime 才能找到) =====

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub fn android_main(app: slint::android::AndroidApp) {
    slint::android::init(app).expect("Slint Android 初始化失败");

    let main_window = MainWindow::new().expect("创建窗口失败");

    let state = std::sync::Arc::new(std::sync::Mutex::new(PlaybackState::new()));
    setup_playback_callbacks(&main_window, state.clone());
    setup_timer(&main_window, state.clone());

    main_window.show().expect("显示窗口失败");
    slint::run_event_loop().expect("事件循环异常");
}
