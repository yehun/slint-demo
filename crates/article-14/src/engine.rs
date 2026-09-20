// 第十四篇, Slint开发的第一个视频播放器 — ffmpeg 解码引擎
//
// 架构 (参考 yehun-slint yehun-plugin/ffmpeg):
//   Audio Thread:  demux → decode audio → fill ring buffer
//   Video Thread:  consume video packets → decode → A/V sync → frame callback
//   cpal Callback: drain ring buffer → speaker + advance clock
//
// 本文件包含: AudioClock, VideoDecoder, AudioDecoder, PlaybackEngine

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ffmpeg_next::{codec, format, media, Rational, software::resampling, software::scaling};
use parking_lot::Mutex;
use ringbuf::{HeapRb, Rb};

// ================================================================
// 常量 (参考 yehun-slint ffmpeg/src/player/engine.rs)
// ================================================================

const MAX_VIDEO_LAG: f64 = 0.100;
const MAX_FRAME_WAIT: f64 = 0.033;
const AUDIO_FRESH_MS: u64 = 80;
const MAX_SLEEP_STEP: f64 = 0.020;
const AUDIO_BUFFER_TARGET: f64 = 0.200;
const AUDIO_BUFFER_MAX: f64 = 3.000;  // 3秒缓冲, 防止音频线程解码过快导致溢出
const DIAG_INTERVAL: u64 = 300;

// ================================================================
// 错误类型
// ================================================================

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("media open failed")]
    MediaOpen,
    #[error("no video stream")]
    NoVideoStream,
    #[error("decoder init: {0}")]
    DecoderInit(String),
    #[error("frame processing: {0}")]
    FrameProcessing(String),
    #[error("stopped")]
    Stopped,
    #[error("thread spawn: {0}")]
    ThreadSpawn(String),
    #[error("ffmpeg: {0}")]
    FFmpeg(String),
}

pub type EngineResult<T> = Result<T, EngineError>;

// ================================================================
// 音频时钟 (参考 yehun-slint ffmpeg/src/decoder/clock.rs)
//
// 由 cpal 输出回调驱动 —— 只有扬声器真正取走的样本才会让指针前进。
// 这是整个播放器里唯一"用户可感知位置"的权威来源。
// ================================================================

pub struct AudioClock {
    played_frames: AtomicU64,
    sample_rate: AtomicU64,
    base_seconds: AtomicU64,
    active: AtomicBool,
    last_advance_nanos: AtomicU64,
}

/// 全局单调时钟基准, 用于 is_fresh 判定
static CLOCK_REF: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

fn monotonic_nanos() -> u64 {
    let base = CLOCK_REF.get_or_init(Instant::now);
    base.elapsed().as_nanos() as u64
}

impl AudioClock {
    pub fn new(sample_rate: u32) -> Arc<Self> {
        Arc::new(Self {
            played_frames: AtomicU64::new(0),
            sample_rate: AtomicU64::new(sample_rate as u64),
            base_seconds: AtomicU64::new(0),
            active: AtomicBool::new(false),
            last_advance_nanos: AtomicU64::new(0),
        })
    }

    /// cpal 回调调用: 推进已播放帧数(必须是帧, 不是样本!)
    pub fn advance(&self, frames: usize) {
        if frames == 0 { return; }
        self.active.store(true, Ordering::Release);
        self.played_frames.fetch_add(frames as u64, Ordering::Release);
        self.last_advance_nanos.store(monotonic_nanos(), Ordering::Release);
    }

    /// cpal 回调调用: 标记设备流已经在跑
    pub fn mark_running(&self) {
        self.active.store(true, Ordering::Release);
    }

    /// 音频时钟是否"此刻可信": 最近 threshold 内真的推进过
    pub fn is_fresh(&self, threshold: Duration) -> bool {
        if !self.active.load(Ordering::Acquire) { return false; }
        let last = self.last_advance_nanos.load(Ordering::Acquire);
        if last == 0 { return false; }
        monotonic_nanos().saturating_sub(last) <= threshold.as_nanos() as u64
    }

    /// 当前播放位置(媒体时间轴秒数)。设备未启动时返回 None
    pub fn position_seconds(&self) -> Option<f64> {
        if !self.active.load(Ordering::Acquire) { return None; }
        let rate = self.sample_rate.load(Ordering::Relaxed).max(1) as f64;
        let base = f64::from_bits(self.base_seconds.load(Ordering::Relaxed));
        Some(base + self.played_frames.load(Ordering::Acquire) as f64 / rate)
    }

    /// seek / 重新开始时重置基准
    pub fn reset(&self, base_seconds: f64) {
        self.played_frames.store(0, Ordering::Release);
        self.base_seconds.store(base_seconds.to_bits(), Ordering::Release);
        self.last_advance_nanos.store(monotonic_nanos(), Ordering::Release);
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::Relaxed) as u32
    }
}

// ================================================================
// 视频解码器 (参考 yehun-slint ffmpeg/src/decoder/video.rs)
// ================================================================

struct VideoDecoder {
    decoder: codec::decoder::Video,
    scaler: scaling::Context,
}

impl VideoDecoder {
    fn new(stream: &format::stream::Stream) -> Result<Self, EngineError> {
        Self::new_with_params(&stream.parameters())
    }

    /// 从编解码器参数创建(用于 seek 后重建)
    fn new_with_params(params: &ffmpeg_next::codec::Parameters) -> Result<Self, EngineError> {
        let ctx = codec::Context::from_parameters(params.clone())
            .map_err(|e| EngineError::DecoderInit(e.to_string()))?;
        let mut decoder = ctx.decoder().video()
            .map_err(|e| EngineError::DecoderInit(e.to_string()))?;
        decoder.set_threading(codec::threading::Config {
            kind: codec::threading::Type::Frame,
            count: 0,
        });
        let scaler = scaling::Context::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            ffmpeg_next::format::Pixel::RGB24,
            decoder.width(),
            decoder.height(),
            scaling::Flags::BILINEAR,
        ).map_err(|e| EngineError::DecoderInit(e.to_string()))?;
        Ok(Self { decoder, scaler })
    }

    fn send_packet(&mut self, pkt: &ffmpeg_next::Packet) -> Result<(), ffmpeg_next::Error> {
        self.decoder.send_packet(pkt)
    }

    fn receive_frame(&mut self) -> Result<ffmpeg_next::frame::Video, ffmpeg_next::Error> {
        let mut frame = ffmpeg_next::frame::Video::empty();
        self.decoder.receive_frame(&mut frame)?;
        Ok(frame)
    }

    fn scale_to_rgb(&mut self, frame: &ffmpeg_next::frame::Video) -> Result<ffmpeg_next::frame::Video, ffmpeg_next::Error> {
        let mut rgb = ffmpeg_next::frame::Video::empty();
        self.scaler.run(frame, &mut rgb)?;
        Ok(rgb)
    }

    fn flush(&mut self) {
        self.decoder.flush()
    }

    fn clear(&mut self) {
        while self.receive_frame().is_ok() {}
    }
}

// ================================================================
// 音频解码器 (参考 yehun-slint ffmpeg/src/decoder/audio.rs)
// ================================================================

struct AudioDecoder {
    decoder: codec::decoder::Audio,
    resampler: resampling::Context,
    buffer: Arc<Mutex<HeapRb<f32>>>,
    out_channels: usize,
}

impl AudioDecoder {
    fn new(
        stream: &format::stream::Stream,
        buffer: Arc<Mutex<HeapRb<f32>>>,
        out_sample_rate: u32,
        out_channels: usize,
    ) -> Result<Self, ffmpeg_next::Error> {
        let ctx = codec::Context::from_parameters(stream.parameters())?;
        let decoder = ctx.decoder().audio()?;

        // 统一重采样成 f32 packed + 设备采样率 + 设备声道
        // 注意: 第4个参数必须是 Sample::F32(Packed), 传 fltp 会输出平面格式,
        // 下游按交错读会越界 → 刺耳电流声 (yehun-slint 踩过)
        use ffmpeg_next::util::channel_layout::ChannelLayout;
        use ffmpeg_next::util::format::sample::Type as SampleType;
        use ffmpeg_next::util::format::Sample;

        let resampler = resampling::Context::get(
            decoder.format(),
            decoder.channel_layout(),
            decoder.rate(),
            Sample::F32(SampleType::Packed),
            if out_channels <= 1 { ChannelLayout::MONO } else { ChannelLayout::STEREO },
            out_sample_rate,
        )?;

        Ok(Self { decoder, resampler, buffer, out_channels })
    }

    fn send_packet(&mut self, pkt: &ffmpeg_next::Packet) -> Result<(), ffmpeg_next::Error> {
        self.decoder.send_packet(pkt)
    }

    fn receive_frame(&mut self) -> Result<ffmpeg_next::frame::Audio, ffmpeg_next::Error> {
        let mut frame = ffmpeg_next::frame::Audio::empty();
        self.decoder.receive_frame(&mut frame)?;
        Ok(frame)
    }

    /// 解码 → 重采样 → 写入 ring buffer (带音量)
    fn process_frame(&mut self, frame: &ffmpeg_next::frame::Audio, volume: f32) {
        use ffmpeg_next::util::format::sample::Type as SampleType;
        use ffmpeg_next::util::format::Sample;

        let mut resampled = ffmpeg_next::frame::Audio::empty();
        if self.resampler.run(frame, &mut resampled).is_err() { return; }

        // 防御: 确保是 packed f32
        if resampled.format() != Sample::F32(SampleType::Packed) { return; }

        let ch = self.out_channels;
        let total = resampled.samples() * ch;
        if total == 0 { return; }

        let src = unsafe {
            std::slice::from_raw_parts(resampled.data(0).as_ptr() as *const f32, total)
        };

        let mut buf = self.buffer.lock();
        for &sample in src {
            if buf.push(sample * volume).is_err() { break; }
        }
    }

    fn flush(&mut self) {
        self.decoder.flush();
    }

    fn clear(&mut self) {
        while self.receive_frame().is_ok() {}
    }
}

// ================================================================
// 播放状态 (跨线程共享)
// ================================================================

#[derive(Clone)]
pub struct PlayerState {
    inner: Arc<StateInner>,
}

struct StateInner {
    status: Mutex<PlayerStatus>,
    volume: Mutex<f32>,
    seek: Mutex<Option<f32>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PlayerStatus {
    Idle, Loading, Playing, Paused, SeekComplete, Stopped, Finished,
}

impl PlayerState {
    pub fn new() -> Self {
        Self { inner: Arc::new(StateInner {
            status: Mutex::new(PlayerStatus::Idle),
            volume: Mutex::new(1.0),
            seek: Mutex::new(None),
            thread: Mutex::new(None),
        })}
    }

    pub fn status(&self) -> PlayerStatus { *self.inner.status.lock() }
    pub fn set_status(&self, s: PlayerStatus) { *self.inner.status.lock() = s; }
    pub fn get_volume(&self) -> f32 { *self.inner.volume.lock() }
    pub fn set_volume(&self, v: f32) { *self.inner.volume.lock() = v.clamp(0.0, 1.0) }
    pub fn is_playing(&self) -> bool { matches!(self.status(), PlayerStatus::Playing | PlayerStatus::Loading) }
    pub fn is_stop(&self) -> bool { matches!(self.status(), PlayerStatus::Idle | PlayerStatus::Stopped | PlayerStatus::Finished) }
    pub fn take_seek(&self) -> Option<f32> { self.inner.seek.lock().take() }
    pub fn set_seek(&self, pos: f32) { *self.inner.seek.lock() = Some(pos); }

    pub fn set_thread(&self, h: JoinHandle<()>) {
        *self.inner.thread.lock() = Some(h);
    }

    pub fn join_thread(&self) {
        if let Some(h) = self.inner.thread.lock().take() {
            let _ = h.join();
        }
    }
}

// ================================================================
// 引擎事件
// ================================================================

#[derive(Debug, Clone)]
pub enum EngineEvent {
    Frame { data: Vec<u8>, width: u32, height: u32, timestamp: f64 },
    Status(PlayerStatus),
    Duration(f32),
    Error(String),
}

type EventSender = mpsc::Sender<EngineEvent>;
static EVENT_TX: std::sync::OnceLock<EventSender> = std::sync::OnceLock::new();
/// 全局音频时钟(供 UI 事件线程读取进度)
static AUDIO_CLOCK: std::sync::Mutex<Option<Arc<AudioClock>>> = std::sync::Mutex::new(None);

pub fn init_event_channel(tx: EventSender) { let _ = EVENT_TX.set(tx); }
pub fn init_audio_clock(clock: Arc<AudioClock>) {
    *AUDIO_CLOCK.lock().unwrap() = Some(clock);
}
pub fn reset_audio_clock() {
    *AUDIO_CLOCK.lock().unwrap() = None;
}
pub fn get_audio_clock_position() -> Option<f32> {
    AUDIO_CLOCK.lock().unwrap().as_ref().and_then(|c| c.position_seconds()).map(|p| p as f32)
}

fn emit(event: EngineEvent) {
    if let Some(tx) = EVENT_TX.get() { let _ = tx.send(event); }
}

// ================================================================
// cpal 音频输出 (线程局部, 参考 yehun-slint)
// ================================================================

thread_local! {
    static AUDIO_STREAM: std::cell::RefCell<Option<cpal::Stream>> =
        const { std::cell::RefCell::new(None) };
}

/// 暂停/恢复 cpal 音频流。必须在创建该流的线程(主播放线程)上调用。
/// 暂停后扬声器立刻无声, 且音频时钟停止前进(时钟只在真正播放样本时 advance),
/// 从而恢复后进度(A/V)不会错位。
fn pause_audio_stream() {
    use cpal::traits::StreamTrait;
    AUDIO_STREAM.with(|slot| {
        if let Some(s) = slot.borrow().as_ref() {
            let _ = s.pause();
        }
    });
}

fn resume_audio_stream() {
    use cpal::traits::StreamTrait;
    AUDIO_STREAM.with(|slot| {
        if let Some(s) = slot.borrow().as_ref() {
            // cpal 0.16 没有 resume(): 恢复即再次 play()(底层 Start 已暂停的流)
            let _ = s.play();
        }
    });
}

fn create_audio_output(
    sample_rate: u32,
    channels: usize,
    buffer: Arc<Mutex<HeapRb<f32>>>,
    clock: Arc<AudioClock>,
) -> Result<(cpal::Stream, u32), Box<dyn std::error::Error + Send + Sync>> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("no output device")?;

    // 尝试使用设备默认配置(更兼容)
    let default_config = device.default_output_config().map_err(|e| format!("default config: {e}"))?;
    log::info!("[AUDIO] device default config: {:?} {}ch {}Hz",
        default_config.sample_format(), default_config.channels(), default_config.sample_rate().0);

    // 我们只走 f32 路径, 如果设备不支持 f32 则报错
    if default_config.sample_format() != cpal::SampleFormat::F32 {
        return Err(format!("device {:?} not f32", default_config.sample_format()).into());
    }

    let ch = default_config.channels() as usize;
    let actual_rate = default_config.sample_rate().0;
    let config = cpal::StreamConfig {
        channels: default_config.channels(),
        sample_rate: default_config.sample_rate(),
        buffer_size: cpal::BufferSize::Fixed(1024),
    };

    // 用于回调日志计数(避免每帧都打日志)
    let cb_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let cb_count_cb = cb_count.clone();
    let clock_cb = clock.clone();

    let stream = device.build_output_stream(
        &config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let usable = data.len() - data.len() % ch;
            let mut buf = buffer.lock();
            let mut played = 0;
            let mut underrun = false;
            for sample in data[..usable].iter_mut() {
                match buf.pop() {
                    Some(s) => { *sample = s; played += 1; }
                    None => { *sample = 0.0; underrun = true; }
                }
            }
            let buf_len_after = buf.len();
            drop(buf);
            for sample in data[usable..].iter_mut() { *sample = 0.0; }
            clock_cb.mark_running();
            // 必须按帧推进, 不能按样本!
            clock_cb.advance(played / ch);
            // 每 100 次回调打一次日志
            let cnt = cb_count_cb.fetch_add(1, Ordering::Relaxed);
            if cnt % 100 == 0 || (underrun && cnt % 10 == 0) {
                log::info!("[AUDIO] cb #{} played={}/{} buf_remain={} underrun={}",
                    cnt, played, data.len(), buf_len_after, underrun);
            }
        },
        |err| log::info!("[AUDIO] stream error: {}", err),
        None,
    )?;

    stream.play()?;
    log::info!("[AUDIO] stream playing OK");
    Ok((stream, actual_rate))
}

// ================================================================
// Seek 命令 (参考 yehun-slint ffmpeg/src/player/audio_producer.rs ProducerCommand)
// ================================================================

#[derive(Debug, Clone)]
enum SeekCommand {
    Seek(f32),  // 跳转到指定秒数
}

// ================================================================
// PlaybackEngine — 主播放引擎
// (参考 yehun-slint ffmpeg/src/player/engine.rs)
// ================================================================

pub struct PlaybackEngine;

impl PlaybackEngine {
    pub fn start(
        file: slint_fs::PlatformFile,
        state: PlayerState,
    ) -> EngineResult<()> {
        log::info!("[ENGINE] start file fd={:?}", file.as_raw_fd());
        ffmpeg_next::init().map_err(|e| EngineError::DecoderInit(e.to_string()))?;
        ffmpeg_next::util::log::set_level(ffmpeg_next::util::log::Level::Error);

        // 打开输入文件 — 使用 fd 方式(参照 yehun-slint)
        // Android content URI 不能直接传给 ffmpeg, 必须用 fd
        let fd = file.as_raw_fd();
        let mut dict = ffmpeg_next::Dictionary::new();
        dict.set("fd", &fd.to_string());
        let mut input = format::input_with_dictionary("fd:", dict)
            .map_err(|_| EngineError::MediaOpen)?;

        // 查找视频流
        let video_stream = input.streams().best(media::Type::Video)
            .ok_or(EngineError::NoVideoStream)?;
        let video_index = video_stream.index();
        let video_tb = video_stream.time_base();

        // 帧间隔
        let frame_interval = {
            let rate = video_stream.avg_frame_rate();
            let fps = rate.numerator() as f64 / rate.denominator().max(1) as f64;
            if fps.is_finite() && fps > 1.0 && fps < 240.0 { 1.0 / fps } else { 1.0 / 24.0 }
        };

        // 时长
        let duration_secs = if input.duration() > 0 {
            input.duration() as f64 / f64::from(ffmpeg_next::ffi::AV_TIME_BASE)
        } else {
            video_stream.duration().max(0) as f64 * f64::from(video_stream.time_base())
        };
        emit(EngineEvent::Duration(duration_secs as f32));

        // 保存视频编解码器参数(用于 seek 后重建解码器)
        // 保存视频编解码器参数(用于 seek 后重建解码器)
        let video_params = video_stream.parameters();

        // 初始化视频解码器
        let mut video_decoder = VideoDecoder::new_with_params(&video_params)?;

        // 音频流
        let audio_stream = input.streams().best(media::Type::Audio);
        let audio_index = audio_stream.as_ref().map(|s| s.index());
        let has_audio = audio_stream.is_some();
        log::info!("[AUDIO] has_audio={} audio_idx={:?}", has_audio, audio_index);

        // 音频基础设施
        let mut sample_rate = 44100u32;
        let mut out_channels = 2usize;
        let ring_capacity = (AUDIO_BUFFER_MAX as f32 * sample_rate as f32 * out_channels as f32) as usize;
        let audio_buffer = Arc::new(Mutex::new(HeapRb::<f32>::new(ring_capacity)));
        let audio_clock = AudioClock::new(sample_rate);
        init_audio_clock(audio_clock.clone()); // 供 UI 读取进度

        // 尝试创建 cpal 输出流(线程局部存储)
        if has_audio {
            AUDIO_STREAM.with(|slot| {
                if slot.borrow().is_none() {
                    match create_audio_output(sample_rate, out_channels, audio_buffer.clone(), audio_clock.clone()) {
                        Ok((stream, actual_rate)) => {
                            sample_rate = actual_rate;
                            log::info!("[AUDIO] cpal OK rate={} ch={}", sample_rate, out_channels);
                            *slot.borrow_mut() = Some(stream);
                        }
                        Err(e) => {
                            log::info!("[AUDIO] cpal FAILED: {}", e);
                        }
                    }
                }
            });
        }

        let audio_decoder = if let Some(ref s) = audio_stream {
            log::info!("[AUDIO] creating decoder: rate={} ch={}", sample_rate, out_channels);
            Some(AudioDecoder::new(s, audio_buffer.clone(), sample_rate, out_channels)
                .map_err(|e| EngineError::DecoderInit(e.to_string()))?)
        } else {
            None
        };

        // 视频包 channel (producer → consumer)
        let (video_tx, video_rx) = mpsc::channel::<ffmpeg_next::Packet>();
        // seek 命令通道
        let (seek_tx, seek_rx) = mpsc::channel::<SeekCommand>();

        let video_tb_for_producer = video_tb;  // clone for audio_producer

        // 启动 audio producer 线程
        let video_idx = video_index;
        let producer_handle = if has_audio {
            let audio_buffer2 = audio_buffer.clone();
            let audio_clock2 = audio_clock.clone();
            let state2 = state.clone();
            let ai = audio_index.unwrap();
            let adec = audio_decoder.unwrap();
            Some(thread::Builder::new()
                .name("audio-producer".into())
                .spawn(move || {
                    audio_producer(input, ai, video_idx, adec, audio_buffer2, audio_clock2,
                        state2, video_tx, seek_rx, video_tb_for_producer);
                })
                .map_err(|e| EngineError::ThreadSpawn(e.to_string()))?)
        } else {
            let state2 = state.clone();
            Some(thread::Builder::new()
                .name("demux-only".into())
                .spawn(move || {
                    for (stream, packet) in input.packets() {
                        if state2.is_stop() { break; }
                        if stream.index() == video_idx {
                            if video_tx.send(packet).is_err() { break; }
                        }
                    }
                })
                .map_err(|e| EngineError::ThreadSpawn(e.to_string()))?)
        };

        // 主视频循环
        state.set_status(PlayerStatus::Playing);
        emit(EngineEvent::Status(PlayerStatus::Playing));

        let mut playback_start: Option<Instant> = None;
        let mut start_pts: f64 = 0.0;
        let mut last_shown_pts: f64 = -1.0;
        let mut frame_count: u64 = 0;
        let mut dropped: u64 = 0;
        // 播放中 seek 后: 跳过 [关键帧, 目标前一帧] 的显示(只解码建链), 直接跳到目标位置,
        // 避免把 GOP 中间帧按正常步速播放出来。暂停 seek 不走这条路径(由暂停块单独渲染预览)。
        let mut seek_skip_until: Option<f64> = None;
        let mut frame_buffer: Vec<u8> = Vec::new();

        loop {
            // 处理 seek (参照 yehun-slint ffmpeg/src/player/engine.rs perform_seek)
            if let Some(pos) = state.take_seek() {
                log::info!("[SEEK] seeking to {:.1}s", pos);
                playback_start = None;
                start_pts = 0.0;
                last_shown_pts = -1.0;
                dropped = 0;
                // 标记: 播放中 seek 后, 播放循环需跳过关键帧→目标前一帧(只解码建链不显示)
                seek_skip_until = Some(pos as f64);

                // 1. 刷新视频解码器(丢弃旧位置残留帧) — 不重建! 参照 yehun-slint
                video_decoder.flush();
                video_decoder.clear();

                // 2. 通知 audio producer 执行 seek
                if seek_tx.send(SeekCommand::Seek(pos)).is_err() {
                    log::warn!("[SEEK] audio producer gone, seek ignored");
                    continue;
                }

                // 3. 排空通道里 producer 在收到 seek 命令之前已转发的旧视频包
                while video_rx.try_recv().is_ok() {}

                emit(EngineEvent::Status(PlayerStatus::SeekComplete));
                log::info!("[SEEK] done");
            }

            if state.is_stop() { break; }

            // 暂停
            if !state.is_playing() {
                state.set_status(PlayerStatus::Paused);
                emit(EngineEvent::Status(PlayerStatus::Paused));
                // 立刻暂停 cpal 音频流: 停止出声 + 冻结音频时钟(时钟只在真正播放样本时前进)
                pause_audio_stream();
                // 进入暂停: 排空播放阶段残留的视频包(属于暂停瞬间位置; 画面已由上一帧持有, 无需再渲染)
                while video_rx.try_recv().is_ok() {}
                while !state.is_playing() && !state.is_stop() {
                    if let Some(pos) = state.take_seek() {
                        log::info!("[SEEK] paused-seek to {:.1}s", pos);
                        // 排空 seek 命令生效前可能已到达的旧视频包(避免被当成目标帧解码)
                        while video_rx.try_recv().is_ok() {}
                        if seek_tx.send(SeekCommand::Seek(pos)).is_err() {
                            log::warn!("[SEEK] audio producer gone, paused-seek ignored");
                        } else {
                            // 阻塞等待 producer 转发 [关键帧..目标帧]:
                            //   1) 先 flush + clear 旧解码状态(丢弃暂停前残留的 B 帧);
                            //   2) 用 recv_timeout 等待——producer 异步处理 seek, 立刻 try_recv
                            //      时 channel 还是空的(竞态), 这正是之前预览永远取不到帧的根因;
                            //   3) 解码整条链(关键帧 + delta), 但只把最后一帧(目标位置画面)上屏,
                            //      避免中间帧闪烁。
                            video_decoder.flush();
                            video_decoder.clear();
                            let deadline = Instant::now() + Duration::from_millis(1500);
                            let mut got = false;
                            let mut pw = 0u32;
                            let mut ph = 0u32;
                            let mut ppts = 0.0f64;
                            'preview: while Instant::now() < deadline {
                                // 阻塞取首个包(关键帧), 再非阻塞排空紧随其后的一批(delta)
                                let first = match video_rx.recv_timeout(Duration::from_millis(150)) {
                                    Ok(p) => p,
                                    Err(mpsc::RecvTimeoutError::Timeout) => {
                                        if got { break 'preview; }
                                        continue;
                                    }
                                    Err(mpsc::RecvTimeoutError::Disconnected) => break 'preview,
                                };
                                let mut batch = Vec::with_capacity(8);
                                batch.push(first);
                                while let Ok(p) = video_rx.try_recv() { batch.push(p); }
                                for pkt in batch {
                                    if video_decoder.send_packet(&pkt).is_ok() {
                                        while let Ok(f) = video_decoder.receive_frame() {
                                            let pts = f.timestamp().or_else(|| f.pts())
                                                .map(|t| t as f64 * f64::from(video_tb))
                                                .unwrap_or(0.0);
                                            if let Ok(rgb) = video_decoder.scale_to_rgb(&f) {
                                                let w = rgb.width();
                                                let h = rgb.height();
                                                let row_bytes = w as usize * 3;
                                                let total = row_bytes * h as usize;
                                                frame_buffer.clear();
                                                let src = rgb.data(0);
                                                let stride = rgb.stride(0);
                                                if stride == row_bytes {
                                                    frame_buffer.extend_from_slice(&src[..total]);
                                                } else {
                                                    frame_buffer.reserve(total);
                                                    for y in 0..h as usize {
                                                        let s = y * stride;
                                                        frame_buffer.extend_from_slice(&src[s..s + row_bytes]);
                                                    }
                                                }
                                                pw = w; ph = h; ppts = pts;
                                                got = true;
                                            }
                                        }
                                    }
                                }
                                if Instant::now() >= deadline { break 'preview; }
                            }
                            if got {
                                last_shown_pts = ppts;
                                emit(EngineEvent::Frame {
                                    data: frame_buffer.clone(),
                                    width: pw,
                                    height: ph,
                                    timestamp: ppts,
                                });
                            }
                            emit(EngineEvent::Status(PlayerStatus::SeekComplete));
                        }
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                if state.is_stop() { break; }
                state.set_status(PlayerStatus::Playing);
                emit(EngineEvent::Status(PlayerStatus::Playing));
                // 恢复: 重新建立墙钟基线 + 恢复音频流
                resume_audio_stream();
                playback_start = Some(Instant::now());
                start_pts = last_shown_pts.max(0.0);
            }

            // 取视频包
            let packet = match video_rx.recv() {
                Ok(p) => p,
                Err(_) => break,
            };

            if video_decoder.send_packet(&packet).is_err() {
                log::warn!("[VIDEO] send_packet failed");
                continue;
            }

            let mut frames_decoded = 0;
            while let Ok(frame) = video_decoder.receive_frame() {
                frames_decoded += 1;
                if state.is_stop() || !state.is_playing() { break; }

                // 计算 PTS
                let pts = frame.timestamp()
                    .or_else(|| frame.pts())
                    .map(|t| t as f64 * f64::from(video_tb))
                    .unwrap_or(0.0);

                // 播放中 seek 后: 跳过关键帧→目标前一帧(仅解码建链不显示), 直接跳到目标位置。
                // 用 20ms 余量, 避免目标帧因取整比 pos 略小而被误跳过(相邻帧通常 >20ms 间隔)。
                if let Some(target) = seek_skip_until {
                    if pts < target - 0.02 {
                        continue;
                    } else {
                        seek_skip_until = None;
                    }
                }

                // A/V 同步: 计算等待时间
                let wait = compute_video_wait(
                    pts, &audio_clock, has_audio, frame_interval,
                    &mut playback_start, &mut start_pts,
                );

                if wait > 0.0 {
                    // 分片睡眠, 及时响应暂停/seek
                    let deadline = Instant::now() + Duration::from_secs_f64(wait);
                    loop {
                        if state.is_stop() || !state.is_playing() { break; }
                        let now = Instant::now();
                        if now >= deadline { break; }
                        let remain = (deadline - now).as_secs_f64();
                        thread::sleep(Duration::from_secs_f64(remain.min(MAX_SLEEP_STEP)));
                    }
                } else if wait <= -1.0 {
                    // 落后太多, 丢帧
                    dropped += 1;
                    continue;
                }

                // YUV → RGB24
                let rgb = match video_decoder.scale_to_rgb(&frame) {
                    Ok(r) => r,
                    Err(_) => continue,
                };

                let w = rgb.width();
                let h = rgb.height();
                let row_bytes = w as usize * 3;
                let total_bytes = row_bytes * h as usize;

                frame_buffer.clear();
                let src = rgb.data(0);
                let stride = rgb.stride(0);

                if stride == row_bytes {
                    frame_buffer.extend_from_slice(&src[..total_bytes]);
                } else {
                    frame_buffer.reserve(total_bytes);
                    for y in 0..h as usize {
                        let start = y * stride;
                        frame_buffer.extend_from_slice(&src[start..start + row_bytes]);
                    }
                }

                last_shown_pts = pts;
                frame_count += 1;

                // 发送帧到 UI
                emit(EngineEvent::Frame {
                    data: frame_buffer.clone(),
                    width: w,
                    height: h,
                    timestamp: pts,
                });
            }
        }

        // EOF
        if let Some(h) = producer_handle { let _ = h.join(); }
        state.set_status(PlayerStatus::Finished);
        emit(EngineEvent::Status(PlayerStatus::Finished));
        Ok(())
    }
}

// ================================================================
// A/V 同步: 计算视频帧该等多久
// (参考 yehun-slint ffmpeg/src/player/engine.rs compute_video_wait)
// ================================================================

fn compute_video_wait(
    video_pts: f64,
    audio_clock: &AudioClock,
    has_audio: bool,
    _frame_interval: f64,
    playback_start: &mut Option<Instant>,
    start_pts: &mut f64,  // ← 改为可变引用
) -> f64 {
    // 建立基线 (参照 yehun-slint: start_pts 设为当前帧 PTS)
    if playback_start.is_none() {
        *playback_start = Some(Instant::now());
        *start_pts = video_pts;  // ← 关键: 设为当前帧 PTS, 不是 0!
    }

    if has_audio {
        let wall_wait = if let Some(start) = playback_start {
            let desired = *start + Duration::from_secs_f64(video_pts - *start_pts);
            let now = Instant::now();
            if desired > now {
                (desired - now).as_secs_f64()
            } else {
                -now.duration_since(desired).as_secs_f64()
            }
        } else {
            0.0
        };

        let audio_time = audio_clock.position_seconds();
        let audio_trusted = audio_time
            .map(|_| audio_clock.is_fresh(Duration::from_millis(AUDIO_FRESH_MS)))
            .unwrap_or(false);

        if audio_trusted {
            let audio_pos = audio_time.unwrap();
            let diff = video_pts - audio_pos;

            // 同步诊断日志(默认 Info 级别下不输出, 需要时调高日志级)
            if (video_pts * 30.0) as u64 % 30 == 0 {
                log::debug!("[SYNC] vpts={:.3} apts={:.3} diff={:.3} wall_wait={:.3}",
                    video_pts, audio_pos, diff, wall_wait);
            }

            // 视频落后太多: 丢帧
            if diff < -MAX_VIDEO_LAG {
                return -1.0;
            }
            // 视频领先: 等到点, 但夹在 MAX_FRAME_WAIT 内
            if diff > MAX_VIDEO_LAG {
                return wall_wait.max(diff.min(MAX_FRAME_WAIT));
            }
            // 同步范围内: 走墙钟实时步速
            return wall_wait.max(0.0);
        }

        // 音频未启动 / 欠载: 墙钟步速
        return wall_wait.max(0.0);
    }

    // 无音频: 纯墙钟
    if let Some(start) = playback_start {
        let desired = *start + Duration::from_secs_f64(video_pts - *start_pts);
        let now = Instant::now();
        if desired > now {
            return (desired - now).as_secs_f64();
        }
        let lateness = now.duration_since(desired).as_secs_f64();
        if lateness > MAX_VIDEO_LAG {
            return -1.0;
        }
    }
    0.0
}

// ================================================================
// Audio Producer 线程
// (参考 yehun-slint ffmpeg/src/player/audio_producer.rs)
// ================================================================

fn audio_producer(
    mut input: format::context::Input,
    audio_idx: usize,
    video_idx: usize,
    mut audio_decoder: AudioDecoder,
    audio_buffer: Arc<Mutex<HeapRb<f32>>>,
    audio_clock: Arc<AudioClock>,
    state: PlayerState,
    video_tx: mpsc::Sender<ffmpeg_next::Packet>,
    mut seek_rx: mpsc::Receiver<SeekCommand>,
    video_time_base: Rational,
) {
    let audio_tb = input.stream(audio_idx)
        .map(|s| s.time_base())
        .unwrap_or(Rational(1, 44100));
    let mut clock_anchored = false;
    let mut audio_frames = 0u64;
    let sample_rate = audio_clock.sample_rate() as f64;
    let ch = 2usize;
    // 背压: 当缓冲区超过 1.5 秒时等待
    let target_samples = (1.5 * sample_rate * ch as f64) as usize;

    log::info!("[AUDIO] thread started audio_idx={} target_buf={}", audio_idx, target_samples);

    // 使用 input.packets() 迭代器读包
    let mut packets = input.packets();
    loop {
        if state.is_stop() { break; }

        // 非阻塞检查 seek 命令(暂停/播放均处理: 暂停时拖动进度条也要立即生效)
        if let Ok(cmd) = seek_rx.try_recv() {
            match cmd {
                SeekCommand::Seek(pos) => {
                    let time_base = f64::from(video_time_base);
                    if time_base > 0.0 {
                        let target_pts = (pos as f64 / time_base).round() as i64;
                        let min_pts = ((pos - 2.0) as f64 / time_base).round() as i64;
                        // 释放迭代器后才能 seek
                        std::mem::drop(packets);
                        // 使用 avformat_seek_file 直接 seek(参照 yehun-slint)
                        let flags = ffmpeg_next::ffi::AVSEEK_FLAG_BACKWARD as i32;
                        let ret = unsafe {
                            ffmpeg_next::ffi::avformat_seek_file(
                                input.as_mut_ptr(),
                                video_idx as i32,
                                min_pts, target_pts, target_pts, flags,
                            )
                        };
                        if ret < 0 {
                            log::warn!("[SEEK] avformat_seek_file failed: {ret}");
                        } else {
                            log::info!("[SEEK] seek to {:.1}s OK (pts={})", pos, target_pts);
                            // 重置时钟 + 刷解码器 + 清 ring
                            audio_clock.reset(pos as f64);
                            clock_anchored = true;
                            audio_decoder.flush();
                            audio_decoder.clear();
                            audio_buffer.lock().clear();
                        }
                        // 重新创建迭代器
                        packets = input.packets();
                        // 跳过目标位置之前的视频帧, 但必须把关键帧及其后 delta 一并转发给主线程。
                        // 否则主线程解码器只拿到目标位置的非关键帧, 缺少参考帧 → 解码失败 →
                        // 暂停预览 / 播放中 seek 后画面黑屏或停旧帧。avformat_seek_file(BACKWARD)
                        // 已落到目标前最近的关键帧, packets 第一个视频包即该关键帧; 这里把
                        // [关键帧, 目标帧] 区间内所有视频包都转发, 主线程解码链恢复后即可正确上屏。
                        let mut current_pts = i64::MIN;
                        let mut skip_vpkts = 0;
                        let mut skip_apkts = 0;
                        const MAX_SKIP: usize = 1000;
                        while skip_vpkts + skip_apkts < MAX_SKIP && current_pts < target_pts {
                            match packets.next() {
                                Some((ref s, ref pkt)) => {
                                    if s.index() == video_idx {
                                        if let Some(pts) = pkt.pts() {
                                            current_pts = pts;
                                        }
                                        // 目标之前的视频包(含关键帧与中间 delta)也转发,
                                        // 供主线程重建解码链; 到达/越过目标则转发并停止。
                                        if video_tx.send(pkt.clone()).is_err() {
                                            log::warn!("[SEEK] video_tx closed");
                                        }
                                        skip_vpkts += 1;
                                        if current_pts >= target_pts {
                                            break;
                                        }
                                    } else if s.index() == audio_idx {
                                        // 音频包: 只记 PTS 重置时钟, 不喂不转发
                                        if let Some(pts) = pkt.pts() {
                                            let sec = pts as f64 * f64::from(audio_tb);
                                            audio_clock.reset(sec);
                                            clock_anchored = true;
                                        }
                                        skip_apkts += 1;
                                    }
                                }
                                None => break,
                            }
                        }
                        log::info!("[SEEK] skipped {} vpkts, {} apkts, current_pts={}", skip_vpkts, skip_apkts, current_pts);
                    }
                }
            }
        }

        // 暂停时: 不推进文件 / 不喂音频 / 不转发视频包到播放流。否则音频流已暂停,
        // 生产者继续解码会把音频堆进 ring(恢复后时钟错位), 并把视频包无限堆积在 channel,
        // 恢复瞬间这些"旧位置"视频包会因 A/V 失步被大量丢帧 → 画面卡住。
        // 注意: 上面的 seek 已在暂停时也处理完(含转发目标帧到 video_rx), 这里只 sleep 不读包。
        if !state.is_playing() {
            thread::sleep(Duration::from_millis(10));
            continue;
        }

        // 读一包
        let (stream, pkt) = match packets.next() {
            Some(pair) => pair,
            None => break,  // EOF
        };

        if stream.index() == audio_idx {
            // 锚定时钟（首个音频包）
            if let Some(pts) = pkt.pts() {
                let seconds = pts as f64 * f64::from(audio_tb);
                if !clock_anchored {
                    audio_clock.reset(seconds);
                    clock_anchored = true;
                    log::info!("[AUDIO] clock anchored at {:.3}s", seconds);
                }
            }
            // 解码 + 喂 ring
            if audio_decoder.send_packet(&pkt).is_ok() {
                while let Ok(frame) = audio_decoder.receive_frame() {
                    audio_decoder.process_frame(&frame, state.get_volume());
                    audio_frames += 1;
                }
            }
            // 背压: 缓冲区太满时等待 cpal 消费
            loop {
                let len = {
                    let guard = audio_buffer.lock();
                    guard.len()
                };
                if len <= target_samples { break; }
                std::thread::sleep(Duration::from_millis(5));
            }
        } else if stream.index() == video_idx {
            if video_tx.send(pkt).is_err() { break; }
        }
    }

    log::info!("[AUDIO] thread done, decoded {} audio frames", audio_frames);
    audio_decoder.flush();
}
