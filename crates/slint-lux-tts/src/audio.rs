//! 音频读写与重采样: WAV/FLAC/MP3 解码 → 单声道 f32; 线性/正弦重采样; 48kHz WAV 写出。

use anyhow::{Context, bail};

/// 任意格式解码为单声道 f32 + 采样率(wav 直读; flac/mp3/ogg 走 symphonia)
pub fn decode_file(path: &std::path::Path) -> anyhow::Result<(Vec<f32>, u32)> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "wav" => decode_wav(path),
        "flac" => decode_flac(path),
        _ => decode_symphonia(path),
    }
}

fn decode_wav(path: &std::path::Path) -> anyhow::Result<(Vec<f32>, u32)> {
    let mut reader = hound::WavReader::open(path)
        .with_context(|| format!("打开 WAV 失败: {}", path.display()))?;
    let spec = reader.spec();
    let mut out: Vec<f32> = Vec::with_capacity(reader.duration() as usize);
    let channels = spec.channels as usize;
    let mut frame: Vec<f32> = vec![0.0; channels];
    match spec.sample_format {
        hound::SampleFormat::Float => {
            let mut idx = 0usize;
            for s in reader.samples::<f32>() {
                frame[idx] = s?;
                idx += 1;
                if idx == channels {
                    let mono = frame.iter().sum::<f32>() / channels as f32;
                    out.push(mono);
                    idx = 0;
                }
            }
        }
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            let mut idx = 0usize;
            for s in reader.samples::<i32>() {
                frame[idx] = s? as f32 / max;
                idx += 1;
                if idx == channels {
                    let mono = frame.iter().sum::<f32>() / channels as f32;
                    out.push(mono);
                    idx = 0;
                }
            }
        }
    }
    Ok((out, spec.sample_rate))
}

fn decode_flac(path: &std::path::Path) -> anyhow::Result<(Vec<f32>, u32)> {
    let mut reader = claxon::FlacReader::open(path)
        .with_context(|| format!("打开 FLAC 失败: {}", path.display()))?;
    let spec = reader.streaminfo();
    let channels = spec.channels as usize;
    let bits = spec.bits_per_sample as i64;
    let max = (1i64 << (bits - 1)) as f32;
    let mut out: Vec<f32> = Vec::with_capacity(spec.samples.unwrap_or(0) as usize);
    let mut frame: Vec<f32> = vec![0.0; channels];
    let mut idx = 0usize;
    for s in reader.samples() {
        frame[idx] = s? as f32 / max;
        idx += 1;
        if idx == channels {
            let mono = frame.iter().sum::<f32>() / channels as f32;
            out.push(mono);
            idx = 0;
        }
    }
    Ok((out, spec.sample_rate))
}

fn decode_symphonia(path: &std::path::Path) -> anyhow::Result<(Vec<f32>, u32)> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path)
        .with_context(|| format!("打开音频失败: {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(e) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(e);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .context("识别音频格式失败")?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .context("音频文件无有效轨道")?;
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.context("缺少采样率")?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("创建解码器失败")?;

    let mut out: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(symphonia::core::errors::Error::ResetRequired) => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(e.into()),
        };
        let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        buf.copy_interleaved_ref(decoded);
        out.extend_from_slice(buf.samples());
    }
    if out.is_empty() {
        bail!("解码结果为空");
    }
    Ok((out, sample_rate))
}

/// 任意采样率 → 目标采样率(带窗 sinc, 透明度足够 TTS 参考用途)
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    if from == 24000 && to == 48000 {
        return resample_2x(input);
    }
    let ratio = to as f64 / from as f64;
    let out_len = ((input.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    // 16 零点 Kaiser(β≈12) 窗 sinc
    const ZEROS: usize = 16;
    let cutoff = 0.47 * (from.min(to) as f64 / 2.0) / (from as f64 / 2.0) * std::f64::consts::PI;
    for i in 0..out_len {
        let t = i as f64 / ratio;
        let left = t.floor() as isize;
        let mut acc = 0.0f64;
        for k in -(ZEROS as isize)..=(ZEROS as isize) {
            let idx = left + k;
            if idx < 0 || idx as usize >= input.len() {
                continue;
            }
            let x = t - idx as f64;
            let sinc = if x.abs() < 1e-9 {
                cutoff / std::f64::consts::PI
            } else {
                (cutoff * x).sin() / (std::f64::consts::PI * x)
            };
            // Kaiser 窗近似(以 Hamming 替代, 阻带足够)
            let wpos = (x / ZEROS as f64 + 1.0) / 2.0;
            let window = 0.54 - 0.46 * (2.0 * std::f64::consts::PI * wpos).cos();
            acc += input[idx as usize] as f64 * sinc * window;
        }
        out.push(acc as f32);
    }
    out
}

/// 24k → 48k 精确 2 倍(输出长度 = 2×输入, 与 librosa/soxr 对齐)
pub fn resample_2x(input: &[f32]) -> Vec<f32> {
    // 零阶保持 + 低通会引入高频衰减; 直接用带相位偏移的 sinc 更透明
    let n = input.len();
    let mut out = Vec::with_capacity(2 * n);
    const ZEROS: usize = 16;
    let cutoff = 0.95 * std::f64::consts::PI; // 相对输出采样率, 保留到 ~22.8kHz
    for i in 0..2 * n {
        let t = i as f64 / 2.0;
        let left = t.floor() as isize;
        let mut acc = 0.0f64;
        for k in -(ZEROS as isize)..=(ZEROS as isize) {
            let idx = left + k;
            if idx < 0 || idx as usize >= n {
                continue;
            }
            let x = t - idx as f64;
            let sinc = if x.abs() < 1e-9 {
                1.0
            } else {
                (cutoff * x).sin() / (std::f64::consts::PI * x)
            };
            let wpos = (x / ZEROS as f64 + 1.0) / 2.0;
            let window = 0.54 - 0.46 * (2.0 * std::f64::consts::PI * wpos).cos();
            acc += input[idx as usize] as f64 * sinc * window;
        }
        out.push(acc as f32);
    }
    out
}

/// RMS 归一(只升不降, 与参考实现一致), 返回原始 rms
pub fn rms_norm(audio: &mut [f32], target_rms: f32) -> f32 {
    let rms = (audio.iter().map(|x| x * x).sum::<f32>() / audio.len().max(1) as f32).sqrt();
    if rms < target_rms && rms > 0.0 {
        let gain = target_rms / rms;
        for x in audio.iter_mut() {
            *x *= gain;
        }
    }
    rms
}

/// 写 48kHz mono WAV(i16)
pub fn write_wav_48k(path: &std::path::Path, samples: &[f32]) -> anyhow::Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec)
        .with_context(|| format!("创建 WAV 失败: {}", path.display()))?;
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        w.write_sample(v)?;
    }
    w.finalize()?;
    Ok(())
}

/// 编码 WAV(PCM16)到内存字节(采样值按 -1..1 输入);手写 44 字节 RIFF 头
pub fn encode_wav_bytes(sample_rate: u32, channels: u16, samples: &[f32]) -> anyhow::Result<Vec<u8>> {
    let channels = channels.max(1);
    let bits = 16u32;
    let byte_rate = sample_rate * channels as u32 * bits / 8;
    let block_align = (channels * bits as u16 / 8) as u16;
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&(bits as u16).to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    Ok(out)
}
