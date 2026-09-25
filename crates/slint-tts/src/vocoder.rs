//! 48kHz/24kHz 双路 Vocos 输出的 Linkwitz-Riley 交叉合并。
//!
//! 对齐 Python crossover_merge: 24k 路上采样到 48k,
//! FFT 域 4 阶 Butterworth 平方(=LR4) 幅频分割, 24k 提供低频 / 48k 提供高频。
//!
//! 上采样用频域零填充(理想带限插值): 24k 信号的 rfft(≤12kHz) 映射到 48k 频率网格
//! (bin 间距相同, k48 = 2×k24, 幅度 ×2), 12kHz 以上补零 — 与 soxr 的 2 倍上采样等效。

use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

pub const CROSSOVER_HZ: f64 = 12000.0;
pub const SR_48K: f64 = 48000.0;

pub fn merge(audio_48k: &[f32], audio_24k: &[f32]) -> Vec<f32> {
    let n24 = audio_24k.len();
    let n = audio_48k.len().min(n24 * 2);
    // 输出长度为偶数, 保证 48k 频率网格与 24k 网格整数对齐
    let n = n - (n % 2);
    if n == 0 {
        return Vec::new();
    }
    let n24_use = n / 2;

    let mut planner = FftPlanner::<f32>::new();
    let fwd = planner.plan_fft_forward(n);
    let fwd24 = planner.plan_fft_forward(n24_use);
    let inv = planner.plan_fft_inverse(n);

    // 48k 路: 完整频谱(rfft 实信号 → 共轭对称), 只修改 0..=n/2 半边
    let mut spec48: Vec<Complex<f32>> = audio_48k[..n]
        .iter()
        .map(|&x| Complex::new(x, 0.0))
        .collect();
    fwd.process(&mut spec48);

    // 24k 路: rfft 后映射到 48k 网格(幅度 ×2), 高于 12kHz 补零
    let mut s24: Vec<Complex<f32>> = audio_24k[..n24_use]
        .iter()
        .map(|&x| Complex::new(x, 0.0))
        .collect();
    fwd24.process(&mut s24);
    let half_bins = s24.len(); // n24_use/2 + 1

    for k in 0..=n / 2 {
        let freq = k as f64 * SR_48K / n as f64;
        // 4 阶 Butterworth 平方 = Linkwitz-Riley
        let ratio = (freq / CROSSOVER_HZ).min(1e6);
        let butter_sq = 1.0 / (1.0 + ratio.powi(8));
        let low_gain = butter_sq.sqrt() as f32;
        let high_gain = (1.0 - butter_sq).sqrt() as f32;
        let low = if k < half_bins {
            s24[k] * 2.0
        } else {
            Complex::new(0.0, 0.0)
        };
        let v = low * low_gain + spec48[k] * high_gain;
        spec48[k] = v;
        if k > 0 && k < n / 2 {
            // 共轭对称重建上半谱
            spec48[n - k] = v.conj();
        }
    }
    inv.process(&mut spec48);
    let scale = 1.0 / n as f32;
    spec48[..n].iter().map(|c| c.re * scale).collect()
}
