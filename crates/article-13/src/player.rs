// 第十三篇, Slint开发的第一个音乐播放器 — 核心播放逻辑
//
// 包含: PlaybackState, TappingSource, 播放控制, 进度追踪, 波形捕获

use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rodio::{Decoder, OutputStream, Sink, Source};
use slint::{ModelRc, Model, SharedString, Timer, TimerMode, VecModel, ComponentHandle};
use slint_file_picker::{pick_file, FileFilter, PickResult};
use slint_fs::PlatformPath;

use crate::PlayerModel;

// ===== 音频状态 (主线程独占, 因为 Sink 非 Send) =====

thread_local! {
    static STREAM: std::cell::RefCell<Option<OutputStream>> = const { std::cell::RefCell::new(None) };
    static SINK: std::cell::RefCell<Option<Sink>> = const { std::cell::RefCell::new(None) };
}

pub fn ensure_audio() -> Result<(), String> {
    SINK.with(|s| {
        if s.borrow().is_none() {
            let (stream, handle) = OutputStream::try_default()
                .map_err(|e| format!("初始化音频输出失败: {e}"))?;
            let sink = Sink::try_new(&handle)
                .map_err(|e| format!("创建音频播放器失败: {e}"))?;
            STREAM.with(|st| *st.borrow_mut() = Some(stream));
            *s.borrow_mut() = Some(sink);
        }
        Ok(())
    })
}

// ===== 当前曲目信息 (Arc<Mutex> 跨回调共享) =====

pub struct PlaybackState {
    pub current_path: String,
    pub total_duration: f32,
    pub start_time: Option<Instant>,
    pub paused_elapsed: f32,
}

impl PlaybackState {
    pub fn new() -> Self {
        Self {
            current_path: String::new(),
            total_duration: 0.0,
            start_time: None,
            paused_elapsed: 0.0,
        }
    }

    pub fn mark_started(&mut self) {
        self.start_time = Some(Instant::now());
    }

    pub fn mark_paused(&mut self) {
        if let Some(start) = self.start_time {
            self.paused_elapsed += start.elapsed().as_secs_f32();
            self.start_time = None;
        }
    }

    pub fn current_elapsed(&self) -> f32 {
        match self.start_time {
            Some(start) => self.paused_elapsed + start.elapsed().as_secs_f32(),
            None => self.paused_elapsed,
        }
    }

    pub fn reset(&mut self) {
        self.start_time = None;
        self.paused_elapsed = 0.0;
    }
}

// ===== 实时波形: 捕获播放采样的 Tap Source =====

const REALTIME_BUFFER_SIZE: usize = 80;

lazy_static::lazy_static! {
    static ref REALTIME_SAMPLES: Mutex<VecDeque<i16>> =
        Mutex::new(VecDeque::with_capacity(REALTIME_BUFFER_SIZE));
}

pub fn samples_to_waveform() -> Vec<f32> {
    let buf = REALTIME_SAMPLES.lock().unwrap_or_else(|e| e.into_inner());
    let mut result = Vec::with_capacity(REALTIME_BUFFER_SIZE);
    for sample in buf.iter() {
        let normalized = (*sample as f32 / 32768.0).abs().min(1.0);
        result.push(normalized);
    }
    while result.len() < REALTIME_BUFFER_SIZE {
        result.push(0.0);
    }
    result
}

pub struct TappingSource<S> {
    inner: S,
}

impl<S: Iterator<Item = i16>> TappingSource<S> {
    pub fn new(inner: S) -> Self {
        if let Ok(mut buf) = REALTIME_SAMPLES.lock() {
            buf.clear();
        }
        Self { inner }
    }
}

impl<S: Iterator<Item = i16>> Iterator for TappingSource<S> {
    type Item = i16;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next();
        if let Some(s) = sample {
            if let Ok(mut buf) = REALTIME_SAMPLES.lock() {
                if buf.len() >= REALTIME_BUFFER_SIZE {
                    buf.pop_front();
                }
                buf.push_back(s);
            }
        }
        sample
    }
}

impl<S: Iterator<Item = i16> + Source> Source for TappingSource<S> {
    fn current_frame_len(&self) -> Option<usize> { self.inner.current_frame_len() }
    fn channels(&self) -> u16 { self.inner.channels() }
    fn sample_rate(&self) -> u32 { self.inner.sample_rate() }
    fn total_duration(&self) -> Option<Duration> { self.inner.total_duration() }
}

// ===== 播放指定曲目 =====

pub fn play_track_at(main_window: &crate::MainWindow, index: i32, state: Arc<Mutex<PlaybackState>>) {
    let model = main_window.global::<PlayerModel>();
    let paths = model.get_playlist_paths();
    let titles = model.get_playlist_titles();
    let mut durations: Vec<f32> = model.get_playlist_durations().iter().collect();

    if index < 0 || index >= paths.row_count() as i32 { return; }

    let idx = index as usize;
    let path: String = paths.iter().nth(idx).map(|s| s.to_string()).unwrap_or_default();
    let title: String = titles.iter().nth(idx).map(|s| s.to_string()).unwrap_or_default();

    if path.is_empty() { return; }

    if ensure_audio().is_err() { return; }

    let file = match PlatformPath::new(&path).read_file() {
        Ok(f) => f,
        Err(e) => {
            main_window.global::<PlayerModel>()
                .set_status(format!("打开失败: {e}").into());
            return;
        }
    };

    let decoder = match Decoder::new(std::io::BufReader::new(file)) {
        Ok(d) => d,
        Err(e) => {
            main_window.global::<PlayerModel>()
                .set_status(format!("解码失败 (格式不支持?): {e}").into());
            return;
        }
    };

    let total_dur = decoder.total_duration()
        .map(|d| d.as_secs_f32())
        .unwrap_or(0.0);

    if idx < durations.len() {
        durations[idx] = total_dur;
    }

    let tapping = TappingSource::new(decoder);

    SINK.with(|s| {
        if let Some(sink) = s.borrow().as_ref() {
            sink.stop();
            sink.append(tapping);
            sink.play();
        }
    });

    if let Ok(mut st) = state.lock() {
        st.current_path = path;
        st.total_duration = total_dur;
        st.reset();
        st.mark_started();
    }

    model.set_title(title.clone().into());
    model.set_artist("本地音频".into());
    model.set_has_track(true);
    model.set_is_playing(true);
    model.set_current_index(index);
    model.set_total_time(total_dur);
    model.set_current_time(0.0);
    model.set_progress(0.0);
    model.set_status(format!("正在播放: {title}").into());

    model.set_playlist_durations(ModelRc::from(Rc::new(VecModel::from(durations))));
}

// ===== 播放控制回调 =====

pub fn setup_playback_callbacks(main_window: &crate::MainWindow, state: Arc<Mutex<PlaybackState>>) {
    // --- 播放/暂停 ---
    let win_weak = main_window.as_weak();
    let state_clone = state.clone();
    let model = main_window.global::<PlayerModel>();
    model.on_play_pause(move || {
        if ensure_audio().is_err() { return; }
        let Some(win) = win_weak.upgrade() else { return };
        let model = win.global::<PlayerModel>();
        SINK.with(|s| {
            if let Some(sink) = s.borrow().as_ref() {
                if sink.is_paused() {
                    sink.play();
                    model.set_is_playing(true);
                    if let Ok(mut st) = state_clone.lock() {
                        st.mark_started();
                    }
                } else if sink.len() > 0 {
                    sink.pause();
                    model.set_is_playing(false);
                    if let Ok(mut st) = state_clone.lock() {
                        st.mark_paused();
                    }
                } else {
                    let idx = model.get_current_index();
                    if idx >= 0 {
                        drop(model);
                        play_track_at(&win, idx, state_clone.clone());
                    }
                }
            }
        });
    });

    // --- 停止 ---
    let state_clone = state.clone();
    let model = main_window.global::<PlayerModel>();
    model.on_stop(move || {
        SINK.with(|s| {
            if let Some(sink) = s.borrow().as_ref() {
                sink.stop();
            }
        });
        if let Ok(mut st) = state_clone.lock() {
            st.reset();
        }
    });

    // --- 上一首 ---
    let win_weak = main_window.as_weak();
    let state_clone = state.clone();
    let model = main_window.global::<PlayerModel>();
    model.on_previous(move || {
        let Some(win) = win_weak.upgrade() else { return };
        let model = win.global::<PlayerModel>();
        let idx = model.get_current_index();
        let count = model.get_playlist_paths().row_count() as i32;
        if count == 0 { return };
        let prev = if idx <= 0 { count - 1 } else { idx - 1 };
        play_track_at(&win, prev, state_clone.clone());
    });

    // --- 下一首 ---
    let win_weak = main_window.as_weak();
    let state_clone = state.clone();
    let model = main_window.global::<PlayerModel>();
    model.on_next(move || {
        let Some(win) = win_weak.upgrade() else { return };
        let model = win.global::<PlayerModel>();
        let idx = model.get_current_index();
        let count = model.get_playlist_paths().row_count() as i32;
        if count == 0 { return };
        let next = if idx >= count - 1 { 0 } else { idx + 1 };
        play_track_at(&win, next, state_clone.clone());
    });

    // --- 指定播放 ---
    let win_weak = main_window.as_weak();
    let state_clone = state.clone();
    let model = main_window.global::<PlayerModel>();
    model.on_play_at(move |index| {
        let Some(win) = win_weak.upgrade() else { return };
        play_track_at(&win, index, state_clone.clone());
    });

    // --- 拖动中: 仅更新视觉位置 ---
    let state_for_preview = state.clone();
    let win_weak_preview = main_window.as_weak();
    let model = main_window.global::<PlayerModel>();
    model.on_seek_preview(move |p| {
        let target = if let Ok(mut st) = state_for_preview.lock() {
            let t = p * st.total_duration;
            st.paused_elapsed = t;
            st.start_time = None;
            t
        } else {
            return;
        };
        if let Some(win) = win_weak_preview.upgrade() {
            let model = win.global::<PlayerModel>();
            model.set_current_time(target);
            if model.get_total_time() > 0.0 {
                model.set_progress(target / model.get_total_time());
            }
        }
    });

    // --- 释放后: 真实 seek ---
    let state_for_seek = state.clone();
    let win_weak_seek = main_window.as_weak();
    let model = main_window.global::<PlayerModel>();
    model.on_seek(move |p| {
        let target_secs = if let Ok(st) = state_for_seek.lock() {
            p * st.total_duration
        } else {
            return;
        };

        let path = if let Ok(st) = state_for_seek.lock() {
            st.current_path.clone()
        } else {
            return;
        };

        if path.is_empty() { return; }

        let file = match PlatformPath::new(&path).read_file() {
            Ok(f) => f,
            Err(_) => return,
        };
        let decoder = match Decoder::new(std::io::BufReader::new(file)) {
            Ok(d) => d,
            Err(_) => return,
        };
        let total_dur = decoder.total_duration().map(|d| d.as_secs_f32()).unwrap_or(0.0);

        let to_skip = Duration::from_secs_f32(target_secs);
        let skipped = decoder.skip_duration(to_skip);
        let tapping = TappingSource::new(skipped);

        SINK.with(|s| {
            if let Some(sink) = s.borrow().as_ref() {
                sink.stop();
                sink.append(tapping);
                sink.play();
            }
        });

        if let Ok(mut st) = state_for_seek.lock() {
            st.total_duration = total_dur;
            st.paused_elapsed = target_secs;
            st.start_time = Some(Instant::now());
        }

        if let Some(win) = win_weak_seek.upgrade() {
            let model = win.global::<PlayerModel>();
            model.set_current_time(target_secs);
            if total_dur > 0.0 {
                model.set_progress(target_secs / total_dur);
            }
        }
    });

    // --- 音量 ---
    let win_weak = main_window.as_weak();
    let model = main_window.global::<PlayerModel>();
    model.on_set_volume(move |v| {
        let clamped = v.clamp(0.0, 1.0);
        if let Some(win) = win_weak.upgrade() {
            win.global::<PlayerModel>().set_volume(clamped);
        }
        SINK.with(|s| {
            if let Some(sink) = s.borrow().as_ref() {
                sink.set_volume(clamped);
            }
        });
    });

    // --- 添加曲目 (pick_file: desktop rfd / Android SAF, 两端都支持) ---
    let win_weak = main_window.as_weak();
    let state_clone = state.clone();
    let model = main_window.global::<PlayerModel>();
    model.on_add_tracks(move || {
        let Some(win) = win_weak.upgrade() else { return };

        win.global::<PlayerModel>()
            .set_status("正在打开文件选择器...".into());

        let win_weak = win_weak.clone();
        let state_inner = state_clone.clone();
        pick_file(
            vec![
                FileFilter::new("音频文件")
                    .extension("mp3")
                    .extension("wav")
                    .extension("flac")
                    .extension("ogg")
                    .mime("audio/*"),
                FileFilter::new("所有文件").mime("*/*"),
            ],
            move |result| {
                // ---- 后台线程: 只取路径, 不做 IO ----
                let path = match result {
                    PickResult::Picked(p) => p.to_string(),
                    PickResult::Cancelled => {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(w) = win_weak.upgrade() {
                                w.global::<PlayerModel>()
                                    .set_status("已取消选择".into());
                            }
                        });
                        return;
                    }
                    PickResult::Error(e) => {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(w) = win_weak.upgrade() {
                                w.global::<PlayerModel>()
                                    .set_status(format!("选择失败: {e}").into());
                            }
                        });
                        return;
                    }
                };

                // ---- 切回主线程: 添加曲目 + 自动播放 ----
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(win) = win_weak.upgrade() else { return };
                    let model = win.global::<PlayerModel>();

                    let title = PlatformPath::new(&path)
                        .file_name()
                        .unwrap_or_else(|_| "未知曲目".to_string());

                    let mut titles: Vec<SharedString> = model.get_playlist_titles().iter().collect();
                    let mut paths: Vec<SharedString> = model.get_playlist_paths().iter().collect();
                    let mut durations: Vec<f32> = model.get_playlist_durations().iter().collect();

                    let new_index = titles.len() as i32;

                    titles.push(title.clone().into());
                    paths.push(path.into());
                    durations.push(0.0);

                    model.set_playlist_titles(ModelRc::from(Rc::new(VecModel::from(titles))));
                    model.set_playlist_paths(ModelRc::from(Rc::new(VecModel::from(paths))));
                    model.set_playlist_durations(ModelRc::from(Rc::new(VecModel::from(durations))));

                    if !model.get_is_playing() || model.get_current_index() < 0 {
                        play_track_at(&win, new_index, state_inner.clone());
                    } else {
                        model.set_status(format!("已添加: {title}").into());
                    }
                });
            },
        );
    });
}

// ===== Timer 进度追踪 =====

pub fn setup_timer(main_window: &crate::MainWindow, state: Arc<Mutex<PlaybackState>>) {
    let win_weak = main_window.as_weak();

    // 进度 Timer (250ms)
    let timer = Box::leak(Box::new(Timer::default()));
    timer.start(TimerMode::Repeated, Duration::from_millis(250), move || {
        let Some(win) = win_weak.upgrade() else { return };
        let model = win.global::<PlayerModel>();

        if model.get_is_dragging() { return; }

        SINK.with(|s| {
            if let Some(sink) = s.borrow().as_ref() {
                if let Ok(st) = state.lock() {
                    let total = st.total_duration;
                    let current = st.current_elapsed();

                    model.set_current_time(current);

                    if total > 0.0 {
                        let progress = (current / total).clamp(0.0, 1.0);
                        model.set_progress(progress);
                    }

                    if sink.empty() && model.get_is_playing() && current > 0.5 {
                        model.set_is_playing(false);
                        model.set_progress(0.0);
                        model.set_current_time(0.0);
                        model.set_status("播放结束".into());

                        let idx = model.get_current_index();
                        let count = model.get_playlist_paths().row_count() as i32;
                        if count > 1 {
                            let next = if idx >= count - 1 { 0 } else { idx + 1 };
                            let win_weak2 = win_weak.clone();
                            let state2 = state.clone();
                            slint::Timer::single_shot(Duration::from_millis(300), move || {
                                if let Some(w) = win_weak2.upgrade() {
                                    play_track_at(&w, next, state2);
                                }
                            });
                        }
                    }
                }
            }
        });
    });

    // 波形动画 Timer (50ms)
    let win_weak_anim = main_window.as_weak();
    let anim_timer = Box::leak(Box::new(Timer::default()));
    anim_timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
        let Some(win) = win_weak_anim.upgrade() else { return };
        let model = win.global::<PlayerModel>();
        if model.get_is_playing() {
            let waveform = samples_to_waveform();
            model.set_waveform_data(ModelRc::from(Rc::new(VecModel::from(waveform))));
        }
    });
}
