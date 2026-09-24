// 第十六篇 — 播放合成结果
//
// 合成结果是 Rust 侧的 Vec<f32>(48kHz 单声道), 不落盘也能播:
// rodio 的 SamplesBuffer 直接把采样序列当 Source, 连 WAV 编码这一趟都省了。
//
// Sink/OutputStream 非 Send, 跟第十三篇一样用 thread_local 锁在主线程。

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

fn play_samples(samples: &[f32]) -> Result<f32, String> {
    SINK.with(|cell| {
        if cell.borrow().is_none() {
            let (stream, handle) =
                OutputStream::try_default().map_err(|e| format!("初始化音频输出失败: {e}"))?;
            let sink = Sink::try_new(&handle).map_err(|e| format!("创建播放器失败: {e}"))?;
            STREAM.with(|st| *st.borrow_mut() = Some(stream));
            *cell.borrow_mut() = Some(sink);
        }
        let borrow = cell.borrow();
        let sink = borrow.as_ref().unwrap();
        sink.clear();
        sink.append(SamplesBuffer::new(1, SAMPLE_RATE, samples.to_vec()));
        Ok(samples.len() as f32 / SAMPLE_RATE as f32)
    })
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
