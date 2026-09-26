// 第十六篇 — 播放合成结果
//
// 合成结果是 Rust 侧的 Vec<f32>(48kHz 单声道)。播放时把它按与 write_wav_48k 完全相同的
// 方式量化成 i16 PCM, 再用 rodio 的 SamplesBuffer<i16> 播放 —— 这条 i16 路径和外部播放器
// 打开 .wav(同样是 i16 PCM16)走的是一致的格式协商, 避免直接用 SamplesBuffer<f32> 在部分
// ALSA/PipeWire 环境下"有数据但设备不出声"的坑。
//
// OutputStream/Sink 非 Send(cpal stream 绑定创建线程), 用 thread_local 钉在主线程。
// 播放线程由 Sink 内部托管, 只要 OutputStream 还活着(主线程 thread_local 持有)就能出声。

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;

use rodio::buffer::SamplesBuffer;
use rodio::{OutputStream, Sink};
use slint::ComponentHandle;

use crate::state::AppState;
use crate::{CloneModel, MainWindow};

const SAMPLE_RATE: u32 = 48_000;

thread_local! {
    static STREAM: RefCell<Option<OutputStream>> = const { RefCell::new(None) };
    static SINK: RefCell<Option<Sink>> = const { RefCell::new(None) };
}

pub fn bind(app: &MainWindow, state: Arc<AppState>) {
    let s = state.clone();
    app.global::<CloneModel>().on_play(move || {
        let Some(samples) = s.last.lock().unwrap().clone() else {
            return;
        };
        match play_samples(&samples) {
            Ok(secs) => {
                s.ui(move |app| app.global::<CloneModel>().set_playing(true));
                watch_end(s.clone(), secs);
            }
            Err(e) => s.set_status(format!("播放失败: {e}")),
        }
    });

    let s = state.clone();
    app.global::<CloneModel>().on_stop_play(move || {
        stop();
        s.ui(|app| app.global::<CloneModel>().set_playing(false));
    });
}

/// f32 → i16 PCM, 与 audio::write_wav_48k 完全一致(峰值裁剪到 [-1,1])
fn to_pcm16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0).round() as i16)
        .collect()
}

fn play_samples(samples: &[f32]) -> Result<f32, String> {
    let pcm = to_pcm16(samples);
    SINK.with(|cell| -> Result<(), String> {
        if cell.borrow().is_none() {
            let (stream, handle) = OutputStream::try_default().map_err(|e| {
                format!("初始化音频输出失败: {e}（请确认默认音频设备可用 / 已安装 alsa 或 pulseaudio）")
            })?;
            let sink = Sink::try_new(&handle).map_err(|e| format!("创建播放器失败: {e}"))?;
            sink.set_volume(1.0);
            STREAM.with(|st| *st.borrow_mut() = Some(stream));
            *cell.borrow_mut() = Some(sink);
        }
        let borrow = cell.borrow();
        let sink = borrow.as_ref().unwrap();
        sink.clear();
        sink.append(SamplesBuffer::new(1, SAMPLE_RATE, pcm));
        sink.play(); // 防御性: 确保处于播放态
        Ok(())
    })?;
    Ok(samples.len() as f32 / SAMPLE_RATE as f32)
}

/// 播放时长已知: 睡到点再把 playing 置回 false(代数不匹配说明用户已经又播了一次)
fn watch_end(state: Arc<AppState>, secs: f32) {
    let gen_id = {
        let mut g = state.play_gen.lock().unwrap();
        *g += 1;
        *g
    };
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs_f32(secs + 0.15));
        if *state.play_gen.lock().unwrap() == gen_id {
            state.ui(|app| app.global::<CloneModel>().set_playing(false));
        }
    });
}

fn stop() {
    let _ = SINK.with(|cell| {
        if let Some(sink) = cell.borrow().as_ref() {
            sink.clear();
        }
    });
}
