// 第十九篇 — 麦克风采集 + 声压级 DSP
//
// 数据流:
//   cpal 输入回调(独立线程) ──push 采样──▶ Arc<Mutex<Vec<f32>>> 累加缓冲
//   Slint Timer 每 50ms ──抽干缓冲──▶ 算 RMS/dB ──▶ 更新 LevelState + 手绘历史曲线
//
// 关键点:
// 1. cpal 输入流支持 f32/i16/u16, 统一归一化到 [-1, 1] 的 f32 再处理
// 2. dBFS = 20*log10(rms), rms∈[0,1] → dBFS∈(-∞,0]; 静音给 -100 下限
// 3. 校准偏移让 0 dBFS 映射到某个 SPL 值(默认 100), 读数即近似 dB SPL
// 4. 历史曲线在 Rust 端逐像素画进 SharedPixelBuffer<RGB8>, 交给 Slint 的 image 属性

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};

// slint::include_modules!() 在 lib.rs 根作用域生成 MainWindow / NoiseModel
use crate::{MainWindow, NoiseModel};

/// 显示量程(与 Slint 左侧刻度、画面纵轴一致): -10..120 dB
pub const DB_MIN: f32 = -10.0;
pub const DB_MAX: f32 = 120.0;
/// 历史曲线像素宽度 = 列数(每列 1px, 50ms/列 → 480 列 ≈ 24 秒)
pub const HISTORY_LEN: usize = 480;
/// 历史曲线像素高度
const CANVAS_H: u32 = 180;

/// 一轮测量内的统计 + 历史缓冲。
#[derive(Default)]
pub struct LevelState {
    /// 平滑后的当前 dB(指数滑动平均, 抗抖)
    pub current_db: f32,
    /// 会话峰值 dB
    pub peak_db: f32,
    /// 会话最低 dB
    pub min_db: f32,
    /// 会话 dB 累加(用于求平均)
    pub sum_db: f32,
    /// 会话平均 dB
    pub avg_db: f32,
    /// 已采样的帧数(用于平均)
    pub n: u32,
    /// 最近的 dB 历史(用于画曲线)
    pub history: VecDeque<f32>,
}

// cpal 输入流非 Send(cpal stream 绑定创建线程), 用 thread_local 钉在主线程/拥有者线程。
thread_local! {
    static STREAM: RefCell<Option<cpal::Stream>> = const { RefCell::new(None) };
}

/// 打开默认麦克风, 建输入流并开始采集。设备信息回写 `info`(机型/采样率/声道)。
pub fn start(
    buf: &Arc<Mutex<Vec<f32>>>,
    running: &Arc<AtomicBool>,
    info: &mut String,
) -> Result<(), String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "找不到麦克风输入设备, 请检查系统录音设备与权限".to_string())?;
    *info = device.name().unwrap_or_else(|_| "未知设备".into());

    let config = device
        .default_input_config()
        .map_err(|e| format!("读取默认输入配置失败: {e}"))?;
    let sample_format = config.sample_format();
    let stream_config: cpal::StreamConfig = config.into();
    info.push_str(&format!(
        " · {}Hz · {}ch",
        stream_config.sample_rate.0, stream_config.channels
    ));

    let buf_c = buf.clone();
    let running_c = running.clone();
    // 输入流错误(设备被拔、权限回收等)只打日志, 不 panic
    let err_fn = |e: cpal::StreamError| eprintln!("cpal 输入流错误: {e}");

    let stream = match sample_format {
        cpal::SampleFormat::F32 => device
            .build_input_stream(
                &stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    push::<f32>(data, &buf_c, &running_c, |s| s)
                },
                err_fn,
                None,
            )
            .map_err(|e| format!("创建 F32 输入流失败: {e}"))?,
        cpal::SampleFormat::I16 => device
            .build_input_stream(
                &stream_config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    push::<i16>(data, &buf_c, &running_c, |s| s as f32 / 32768.0)
                },
                err_fn,
                None,
            )
            .map_err(|e| format!("创建 I16 输入流失败: {e}"))?,
        cpal::SampleFormat::U16 => device
            .build_input_stream(
                &stream_config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    push::<u16>(data, &buf_c, &running_c, |s| s as f32 / 32768.0 - 1.0)
                },
                err_fn,
                None,
            )
            .map_err(|e| format!("创建 U16 输入流失败: {e}"))?,
        other => return Err(format!("不支持的采样格式: {other:?}")),
    };

    stream
        .play()
        .map_err(|e| format!("启动输入流失败: {e}"))?;
    running.store(true, Ordering::SeqCst);
    // 存住流(句柄活着 = 持续采集; 这里 drop 即停止)
    STREAM.with(|s| *s.borrow_mut() = Some(stream));
    Ok(())
}

/// 把一段采样归一化后追加进累加缓冲。`conv` 把原始样本转成 [-1,1] 的 f32。
fn push<T: Copy>(
    data: &[T],
    buf: &Arc<Mutex<Vec<f32>>>,
    running: &Arc<AtomicBool>,
    conv: impl Fn(T) -> f32,
) {
    if !running.load(Ordering::SeqCst) {
        return;
    }
    let mut b = buf.lock().unwrap();
    b.reserve(data.len());
    for &s in data {
        b.push(conv(s));
    }
}

/// 停止采集: 落下 running 标志并丢弃流句柄。
pub fn stop(running: &Arc<AtomicBool>) {
    running.store(false, Ordering::SeqCst);
    STREAM.with(|s| *s.borrow_mut() = None);
}

/// 定时采样: 抽干缓冲 → 算 dB → 更新统计 → 手绘历史曲线 → 写回 Slint 属性。
/// 本函数在 Slint 事件循环里调用, 可直接写属性。
pub fn tick(buf: &Arc<Mutex<Vec<f32>>>, level: &Arc<Mutex<LevelState>>, app: &MainWindow) {
    // 抽干这段时间累积的全部采样
    let chunk = {
        let mut b = buf.lock().unwrap();
        std::mem::take(&mut *b)
    };
    if chunk.is_empty() {
        return;
    }

    // RMS = sqrt(mean(sample^2)); 用 f64 累加防爆精
    let sum_sq = chunk
        .iter()
        .map(|&s| {
            let v = s as f64;
            v * v
        })
        .sum::<f64>();
    let rms = (sum_sq / chunk.len() as f64).sqrt() as f32;

    // dBFS: 静音(rms 极小)给 -100 下限, 满量程(1.0)为 0 dB
    let dbfs = if rms > 1e-7 {
        20.0 * rms.max(1e-7).log10()
    } else {
        -100.0
    }
    .clamp(-100.0, 0.0);

    // 校准偏移: 0 dBFS ≈ calib-offset dB SPL(默认 100)
    let offset = app.global::<NoiseModel>().get_calib_offset();
    let db = dbfs + offset;

    // 更新统计, 并手绘历史曲线(都在锁内, 避免重复加锁)
    let (cur, peak, avg, minv, img) = {
        let mut st = level.lock().unwrap();
        // 指数滑动平均: 新值占 40%, 历史占 60%, 读数更稳
        let cur = st.current_db * 0.6 + db * 0.4;
        st.current_db = cur;
        if st.n == 0 {
            st.min_db = cur;
        } else {
            st.min_db = st.min_db.min(cur);
        }
        st.peak_db = st.peak_db.max(cur);
        st.sum_db += cur;
        st.n += 1;
        st.avg_db = st.sum_db / st.n as f32;

        st.history.push_back(cur);
        while st.history.len() > HISTORY_LEN {
            st.history.pop_front();
        }

        let img = draw_history(&st);
        (cur, st.peak_db, st.avg_db, st.min_db, img)
    };

    let g = app.global::<NoiseModel>();
    g.set_db_value(cur);
    g.set_db_text(format!("{cur:.1}").into());
    g.set_level_pct(((cur - DB_MIN) / (DB_MAX - DB_MIN) * 100.0).clamp(0.0, 100.0));
    g.set_peak_text(format!("{peak:.1}").into());
    g.set_avg_text(format!("{avg:.1}").into());
    g.set_min_text(format!("{minv:.1}").into());
    g.set_history(img);
}

/// dB → 画布纵坐标(0 在底部, 越高 dB 越靠上)。
fn db_to_y(db: f32, h: u32) -> u32 {
    let t = ((db - DB_MIN) / (DB_MAX - DB_MIN)).clamp(0.0, 1.0);
    let y = (h as f32 - t * h as f32) as i32;
    y.clamp(0, h as i32 - 1) as u32
}

/// 在 Rust 端手绘历史曲线(填充面积 + 顶部描边 + 横向网格), 返回 Slint Image。
pub fn draw_history(st: &LevelState) -> Image {
    let w = HISTORY_LEN as u32;
    let h = CANVAS_H;
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(w, h);
    {
        let px = buf.make_mut_slice();
        // 背景
        for p in px.iter_mut() {
            *p = Rgba8Pixel {
                r: 250,
                g: 250,
                b: 252,
                a: 255,
            };
        }
        // 横向网格: 0 / 30 / 60 / 90 / 120 dB
        for &db in &[0.0_f32, 30.0, 60.0, 90.0, 120.0] {
            let y = db_to_y(db, h) as usize;
            for x in 0..w as usize {
                px[y * w as usize + x] = Rgba8Pixel {
                    r: 226,
                    g: 230,
                    b: 235,
                    a: 255,
                };
            }
        }
        // 面积填充 + 顶部描边(每列一根竖线, 顶部像素加深)
        let len = st.history.len();
        for i in 0..len {
            let x = i;
            let y = db_to_y(st.history[i], h).clamp(0, h - 1) as usize;
            for yy in y..h as usize {
                px[yy * w as usize + x] = Rgba8Pixel {
                    r: 187,
                    g: 222,
                    b: 251,
                    a: 255,
                };
            }
            px[y * w as usize + x] = Rgba8Pixel {
                r: 21,
                g: 101,
                b: 192,
                a: 255,
            };
        }
    }
    Image::from_rgba8(buf)
}
