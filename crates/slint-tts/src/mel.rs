//! 100 维 log-mel 特征提取(对齐 librosa: htk=True, norm=None, power=1, center=True)。

use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

pub const N_MELS: usize = 100;
pub const N_FFT: usize = 1024;
pub const HOP: usize = 256;
pub const FEAT_SCALE: f32 = 0.1;
pub const SAMPLE_RATE: u32 = 24000;

/// HTK mel 滤波器组(norm=None: 三角窗不做面积归一), [bin][mel]
fn mel_filterbank() -> Vec<Vec<f32>> {
    let hz_to_mel = |f: f64| 2595.0 * (1.0 + f / 700.0).log10();
    let mel_to_hz = |m: f64| 700.0 * (10.0f64.powf(m / 2595.0) - 1.0);

    let fmax = SAMPLE_RATE as f64 / 2.0;
    let mel_min = hz_to_mel(0.0);
    let mel_max = hz_to_mel(fmax);
    // n_mels+2 个三角拐点(均匀 mel 间隔)
    let mel_pts: Vec<f64> = (0..N_MELS + 2)
        .map(|i| mel_min + (mel_max - mel_min) * i as f64 / (N_MELS + 1) as f64)
        .collect();
    let hz_pts: Vec<f64> = mel_pts.iter().map(|&m| mel_to_hz(m)).collect();
    let n_bins = N_FFT / 2 + 1;

    let mut fb = vec![vec![0.0f32; N_MELS]; n_bins];
    for (k, row) in fb.iter_mut().enumerate() {
        let f = k as f64 * SAMPLE_RATE as f64 / N_FFT as f64;
        for m in 0..N_MELS {
            let (low, center, high) = (hz_pts[m], hz_pts[m + 1], hz_pts[m + 2]);
            let w = if f > low && f < high {
                if f <= center {
                    (f - low) / (center - low)
                } else {
                    (high - f) / (high - center)
                }
            } else {
                0.0
            };
            row[m] = w as f32;
        }
    }
    fb
}

/// audio(24kHz mono f32) → [T][100] log-mel ×FEAT_SCALE
pub fn extract(audio: &[f32]) -> Vec<Vec<f32>> {
    let fb = mel_filterbank();
    let n_bins = N_FFT / 2 + 1;
    let n = audio.len();

    // center=True: 两侧零填充 n_fft/2(librosa ≥0.10 stft 默认 pad_mode='constant')
    let pad = N_FFT / 2;
    let mut padded: Vec<f32> = vec![0.0; n + 2 * pad];
    padded[pad..pad + n].copy_from_slice(audio);

    let frames = 1 + n / HOP;
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(N_FFT);
    let window: Vec<f32> = (0..N_FFT)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / N_FFT as f32).cos())
        .collect();

    let mut buf: Vec<Complex<f32>> = vec![Complex::new(0.0, 0.0); N_FFT];
    let mut out = Vec::with_capacity(frames);
    for t in 0..frames {
        let start = t * HOP;
        for (k, w) in window.iter().enumerate() {
            buf[k] = Complex::new(padded[start + k] * w, 0.0);
        }
        fft.process(&mut buf);
        // mel = filterbank · magnitude(power=1)
        let mut mel = vec![0.0f32; N_MELS];
        for (k, c) in buf.iter().take(n_bins).enumerate() {
            let mag = c.norm();
            if mag == 0.0 {
                continue;
            }
            for (m, &w) in fb[k].iter().enumerate() {
                if w != 0.0 {
                    mel[m] += w * mag;
                }
            }
        }
        for m in mel.iter_mut() {
            *m = m.max(1e-7).ln() * FEAT_SCALE;
        }
        out.push(mel);
    }
    out
}
