// 第十四篇, Slint开发的第一个视频播放器
//
// 核心知识点:
// 1. ffmpeg-next — avformat 解复用 + avcodec 解码 + sws_scale (YUV→RGB24)
// 2. cpal — 纯 Rust 音频输出, 回调驱动 AudioClock
// 3. A/V 同步 — 音频时钟为主, 视频帧 PTS 与时钟比较: 领先则等/落后则丢
// 4. ringbuf — 无锁环形缓冲, audio thread 写/cpal 回调读
// 5. SharedPixelBuffer — Rust→Slint 的视频帧传递
//
// 架构 (三线程):
//   Audio Thread: demux → audio decode → fill ring buffer
//   Video Thread: consume video packets → decode → A/V sync → UI
//   cpal Callback: pop ring buffer → speaker + advance clock

slint::include_modules!();

mod engine;
mod player;

use std::path::PathBuf;
use std::time::Duration;

use slint::{ComponentHandle, Timer, TimerMode};

use player::VideoPlayer;

thread_local! {
    static PLAYER: std::cell::RefCell<Option<VideoPlayer>> =
        const { std::cell::RefCell::new(None) };
    static CURRENT_FILE: std::cell::RefCell<String> =
        const { std::cell::RefCell::new(String::new()) };
}

fn ensure_player() -> VideoPlayer { VideoPlayer::new() }

fn play_current_file(main_window: &MainWindow) {
    let model = main_window.global::<PlayerModel>();
    let path_str = model.get_file_path().to_string();

    if path_str.is_empty() {
        model.set_status_text("select video first".into());
        return;
    }

    // 使用 PlatformPath(支持 Android content URI + 本地路径)
    let platform_path = slint_fs::PlatformPath::new(&path_str);

    // 读取文件(参照 yehun-slint: PlatformPath → PlatformFile → fd)
    let file = match platform_path.read_file() {
        Ok(f) => f,
        Err(e) => {
            model.set_status_text(format!("文件打开失败: {e}").into());
            return;
        }
    };

    // 检查是否是新文件
    let is_new_file = CURRENT_FILE.with(|f| {
        let mut current = f.borrow_mut();
        if *current != path_str {
            *current = path_str.clone();
            true
        } else {
            false
        }
    });

    PLAYER.with(|p| {
        let mut borrow = p.borrow_mut();

        // 新文件: 停止旧播放, 重建播放器
        if is_new_file {
            if let Some(ref mut player) = *borrow {
                player.stop();
            }
            *borrow = Some(ensure_player());
        } else if borrow.is_none() {
            *borrow = Some(ensure_player());
        }

        let player = borrow.as_mut().unwrap();

        if is_new_file {
            // 新文件: 重置进度 + seek 状态 + 开始播放
            model.set_seek_position(0.0);
            model.set_current_time_f(0.0);
            model.set_duration(0.0);
            model.set_change_seek(false);
            player.stop();
            player::reset_audio_clock();
            match player.play(file) {
                Ok(_) => {
                    model.set_playing(true);
                    model.set_status_text("playing...".into());
                }
                Err(e) => {
                    model.set_status_text(format!("play error: {e}").into());
                }
            }
        } else {
            // 同文件: 切换暂停/播放
            match player.status() {
                player::PlayerStatus::Playing |
                player::PlayerStatus::Loading |
                player::PlayerStatus::SeekComplete => {
                    player.toggle();
                }
                player::PlayerStatus::Paused => {
                    player.toggle();
                }
                _ => {
                    player.stop();
                    match player.play(file) {
                        Ok(_) => {
                            model.set_playing(true);
                            model.set_status_text("playing...".into());
                        }
                        Err(e) => {
                            model.set_status_text(format!("play error: {e}").into());
                        }
                    }
                }
            }
        }
    });
}

// 进度/时间统一由音频时钟驱动: 每 100ms 读取音频时钟位置,
// 仅在用户未拖动进度条时写入 current-time-f / seek-position。
// 这样即使暂停、seek 后或帧率极低, 时间显示也始终正确且不会跳回 0。
fn setup_progress_timer(main_window: &MainWindow) {
    let win_weak = main_window.as_weak();
    let timer = Box::leak(Box::new(Timer::default()));
    timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
        let Some(win) = win_weak.upgrade() else { return };
        let model = win.global::<PlayerModel>();
        // 用户正在拖动时, 进度条由 UI 直接控制, 不要覆盖
        if model.get_change_seek() { return; }
        // 暂停/停止时不从时钟覆盖进度: 否则暂停期间拖动后的位置会被"冻结的旧时钟"覆盖回去,
        // 进度条与时间也才能正确停在用户设定的位置。
        if !model.get_playing() { return; }
        let dur = model.get_duration();
        if let Some(pos) = crate::engine::get_audio_clock_position() {
            model.set_current_time_f(pos);
            if dur > 0.0 {
                model.set_seek_position(pos.min(dur));
            }
        }
    });
}

fn setup_ui_callbacks(main_window: &MainWindow) {
    // play/pause
    let ww = main_window.as_weak();
    main_window.global::<PlayerModel>().on_play(move || {
        log::info!("[LIB] on_play callback triggered");
        let Some(w) = ww.upgrade() else { return };
        play_current_file(&w);
    });

    // select video (播放由 select_and_play_video 内部触发)
    let ww = main_window.as_weak();
    main_window.global::<PlayerModel>().on_select_video(move || {
        let Some(w) = ww.upgrade() else { return };
        player::select_and_play_video(&w);
    });

    // stop
    let ww = main_window.as_weak();
    main_window.global::<PlayerModel>().on_stop_play(move || {
        PLAYER.with(|p| {
            if let Some(pl) = p.borrow_mut().as_mut() { pl.stop(); }
        });
        CURRENT_FILE.with(|f| *f.borrow_mut() = String::new()); // 重置, 下次选同文件会重新播放
        let Some(w) = ww.upgrade() else { return };
        let m = w.global::<PlayerModel>();
        // 重置音频时钟为 None: 否则 100ms 进度计时器会用"残留的旧位置"把下面刚清成的 0 覆盖回去
        player::reset_audio_clock();
        m.set_playing(false);
        m.set_change_seek(false);
        m.set_seek_position(0.0);
        m.set_current_time_f(0.0);
        m.set_status_text("stopped".into());
    });

    // seek: change-seek 标志由 UI 控制(拖动时置 true, 释放时置 false),
    // 这里只把目标位置交给引擎。
    let ww_seek = main_window.as_weak();
    main_window.global::<PlayerModel>().on_seek_to(move |position| {
        let Some(w) = ww_seek.upgrade() else { return };
        PLAYER.with(|p| {
            if let Some(pl) = p.borrow_mut().as_mut() { pl.seek(position); }
        });
    });

    // volume
    main_window.global::<PlayerModel>().on_change_volume(move |volume| {
        PLAYER.with(|p| {
            if let Some(pl) = p.borrow_mut().as_mut() { pl.set_volume(volume); }
        });
    });
}

#[cfg(feature = "desktop")]
pub fn desktop_main() {
    // 初始化简单 logger (输出到 stderr)
    init_stderr_logger();
    log::info!("[LIB] desktop_main started");

    let main_window = MainWindow::new().expect("create window");

    player::setup_video_player(&main_window);
    setup_ui_callbacks(&main_window);
    setup_progress_timer(&main_window);

    main_window.show().expect("show window");
    slint::run_event_loop().expect("event loop");
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub fn android_main(app: slint::android::AndroidApp) {
    slint::android::init(app).expect("slint android init");

    let main_window = MainWindow::new().expect("create window");
    player::setup_video_player(&main_window);
    setup_ui_callbacks(&main_window);
    setup_progress_timer(&main_window);

    main_window.show().expect("show window");
    slint::run_event_loop().expect("event loop");
}

// 简单 stderr logger (无需额外依赖)
fn init_stderr_logger() {
    use log::Log;
    struct StderrLogger;
    impl Log for StderrLogger {
        fn enabled(&self, _: &log::Metadata) -> bool { true }
        fn log(&self, record: &log::Record) {
            eprintln!("[{}] {}", record.level(), record.args());
        }
        fn flush(&self) {}
    }
    let _ = log::set_boxed_logger(Box::new(StderrLogger));
    log::set_max_level(log::LevelFilter::Info);
}
