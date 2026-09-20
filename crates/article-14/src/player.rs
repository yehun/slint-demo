// 第十四篇: 视频播放器 — 公共 API + UI 事件集成
//
// 连接 Slint UI 和 ffmpeg 解码引擎:
//   - VideoPlayer: 对外播放控制 API
//   - setup_video_player: 启动事件处理 (engine → UI)
//   - select_and_play_video: 文件选择器

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use slint::{ComponentHandle, Image, SharedPixelBuffer};

use slint_file_picker::{pick_file, FileFilter, PickResult};
use slint_fs::PlatformPath;

pub use crate::engine::PlayerStatus;
pub use crate::engine::PlayerState;
pub use crate::engine::reset_audio_clock;

use crate::engine::EngineEvent;

/// 播放器事件 (engine → UI)
/// 注意: Frame 不带 timestamp, 进度条由音频时钟驱动(参照 yehun-slint)
#[derive(Debug, Clone)]
pub enum PlayerEvent {
    Frame { data: Vec<u8>, width: u32, height: u32 },
    Status(PlayerStatus),
    Duration(f32),
    Error(String),
}

impl From<EngineEvent> for PlayerEvent {
    fn from(e: EngineEvent) -> Self {
        match e {
            EngineEvent::Frame { data, width, height, .. } => {
                PlayerEvent::Frame { data, width, height }
            }
            EngineEvent::Status(s) => PlayerEvent::Status(s),
            EngineEvent::Duration(d) => PlayerEvent::Duration(d),
            EngineEvent::Error(msg) => PlayerEvent::Error(msg),
        }
    }
}

/// 视频播放器公共 API
pub struct VideoPlayer {
    state: crate::engine::PlayerState,
}

impl VideoPlayer {
    pub fn new() -> Self {
        Self { state: crate::engine::PlayerState::new() }
    }

    /// 播放指定文件(参照 yehun-slint: 接受 PlatformFile)
    pub fn play(&mut self, file: slint_fs::PlatformFile) -> Result<(), String> {
        log::info!("[PLAYER] play() fd={:?}", file.as_raw_fd());
        if self.state.is_playing() {
            log::info!("[PLAYER] already playing, skip");
            return Ok(());
        }
        self.state.set_status(PlayerStatus::Loading);

        let state = self.state.clone();
        log::info!("[PLAYER] spawning playback thread");
        let handle = std::thread::Builder::new()
            .name("video-playback".into())
            .spawn(move || {
                log::info!("[PLAYER] thread started, calling engine");
                if let Err(e) = crate::engine::PlaybackEngine::start(file, state) {
                    log::error!("[PLAYER] engine error: {}", e);
                }
            })
            .map_err(|e| format!("thread spawn: {e}"))?;

        self.state.set_thread(handle);
        Ok(())
    }

    pub fn toggle(&mut self) {
        let s = self.state.status();
        if s == PlayerStatus::Playing || s == PlayerStatus::Loading {
            self.state.set_status(PlayerStatus::Paused);
        } else if s == PlayerStatus::Paused {
            self.state.set_status(PlayerStatus::Playing);
        }
    }

    pub fn stop(&mut self) {
        self.state.set_status(PlayerStatus::Stopped);
        self.state.join_thread();
    }

    pub fn seek(&mut self, pos: f32) {
        self.state.set_seek(pos);
    }

    pub fn set_volume(&mut self, v: f32) {
        self.state.set_volume(v);
    }

    pub fn status(&self) -> PlayerStatus {
        self.state.status()
    }

    pub fn state(&self) -> &crate::engine::PlayerState {
        &self.state
    }
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        self.state.set_status(PlayerStatus::Stopped);
        self.state.join_thread();
    }
}

fn rgb_to_image_buffer(data: &[u8], w: u32, h: u32) -> Result<SharedPixelBuffer<slint::Rgb8Pixel>, String> {
    let expected = (w * h * 3) as usize;
    if data.len() < expected {
        return Err("data too short".to_string());
    }
    let mut buf = SharedPixelBuffer::new(w, h);
    buf.make_mut_bytes().copy_from_slice(&data[..expected]);
    Ok(buf)
}

/// 启动事件处理: engine → channel → Slint UI
/// 进度条由音频时钟驱动(参照 yehun-slint ffmpeg.rs)
pub fn setup_video_player(mw: &crate::MainWindow) {
    let (tx, rx) = mpsc::channel::<EngineEvent>();
    crate::engine::init_event_channel(tx);

    let ww = mw.as_weak();

    std::thread::Builder::new()
        .name("video-events".into())
        .spawn(move || {
            let mut last_frame = std::time::Instant::now();
            loop {
                match rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(event) => {
                        let ev: PlayerEvent = event.into();
                        match ev {
                            PlayerEvent::Frame { data, width, height } => {
                                // 节流: 每 33ms 最多上传一帧到 UI。
                                // 进度/时间由 Slint 计时器从音频时钟统一更新(见 setup_progress_timer),
                                // 这里只负责把解码后的画面推给 UI, 避免双更新源互相覆盖。
                                let now = std::time::Instant::now();
                                if now.duration_since(last_frame) > Duration::from_millis(33) {
                                    last_frame = now;
                                    if let Ok(buf) = rgb_to_image_buffer(&data, width, height) {
                                        let w3 = ww.clone();
                                        let _ = slint::invoke_from_event_loop(move || {
                                            let Some(w) = w3.upgrade() else { return };
                                            w.global::<crate::PlayerModel>()
                                                .set_video_frame(Image::from_rgb8(buf));
                                        });
                                    }
                                }
                            }
                            PlayerEvent::Status(s) => {
                                let w3 = ww.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    let Some(w) = w3.upgrade() else { return };
                                    let m = w.global::<crate::PlayerModel>();
                                    match s {
                                        PlayerStatus::Playing => {
                                            m.set_playing(true);
                                            m.set_status_text("playing".into());
                                        }
                                        PlayerStatus::Paused => {
                                            m.set_playing(false);
                                            m.set_status_text("paused".into());
                                        }
                                        PlayerStatus::Stopped => {
                                            m.set_playing(false);
                                            m.set_status_text("stopped".into());
                                        }
                                        PlayerStatus::Finished => {
                                            m.set_playing(false);
                                            m.set_status_text("finished".into());
                                        }
                                        PlayerStatus::SeekComplete => {
                                            // seek 完成, 恢复进度更新(参照 yehun-slint)
                                            m.set_change_seek(false);
                                        }
                                        _ => {}
                                    }
                                });
                            }
                            PlayerEvent::Duration(d) => {
                                let w3 = ww.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    let Some(w) = w3.upgrade() else { return };
                                    w.global::<crate::PlayerModel>().set_duration(d);
                                });
                            }
                            PlayerEvent::Error(msg) => {
                                log::error!("{}", msg);
                            }
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .expect("spawn video-events");
}

/// 弹出视频文件选择器, 选完后自动开始播放
pub fn select_and_play_video(mw: &crate::MainWindow) {
    let ww = mw.as_weak();
    pick_file(
        vec![
            // 只允许视频文件(Android SAF 按 MIME 过滤)
            FileFilter::new("视频文件")
                .extension("mp4").extension("mkv")
                .extension("avi").extension("mov")
                .extension("webm").extension("flv")
                .extension("3gp").extension("ts")
                .mime("video/*"),
        ],
        move |r| {
            let p = match r {
                PickResult::Picked(p) => p.to_string(),
                PickResult::Cancelled => return,
                PickResult::Error(_) => return,
            };
            let _ = slint::invoke_from_event_loop(move || {
                let Some(w) = ww.upgrade() else { return };
                let m = w.global::<crate::PlayerModel>();
                let title = PlatformPath::new(&p)
                    .file_name()
                    .unwrap_or_else(|_| "unknown".to_string());
                m.set_file_title(title.into());
                m.set_status_text("loading...".into());
                m.set_file_path(p.into());
                // 触发播放: 调用模型的 on_play 回调
                log::info!("[PLAYER] invoking play callback");
                m.invoke_play();
                log::info!("[PLAYER] invoke_play returned");
            });
        },
    );
}
