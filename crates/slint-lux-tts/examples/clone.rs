//! CLI: 语音克隆(不依赖 UI, 用来快速验证模型与管线)
//!
//! 用法:
//!   LUX_TTS_MODEL_DIR=<模型目录> cargo run --release -p slint-lux-tts --example clone -- \
//!     <ref.wav> <参考音频转写> <要说的文本> <output.wav> [--model int8|fp16|fp32] [--steps 8] [--threads 4]
//!
//! 模型目录里需要: tokens.txt / text_encoder.onnx / fm_decoder.onnx / vocos.onnx
//! (int8 变体为 text_encoder_int8.onnx / fm_decoder_int8.onnx)

use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        eprintln!(
            "用法: {} <ref.wav> <参考音频转写> <要说的文本> <output.wav> [--model int8] [--steps 8] [--threads 4]",
            args[0]
        );
        std::process::exit(2);
    }
    let ref_path = PathBuf::from(&args[1]);
    let transcript = &args[2];
    let text = &args[3];
    let out_path = PathBuf::from(&args[4]);

    let mut model = "fp32".to_string();
    let mut opts = slint_lux_tts::GenOpts::default();
    let mut threads = 4usize;
    let mut i = 5;
    while i < args.len() {
        match args[i].as_str() {
            "--model" => {
                model = args.get(i + 1).cloned().unwrap_or_default();
                i += 2;
            }
            "--steps" => {
                opts.num_steps = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(8);
                i += 2;
            }
            "--seed" => {
                opts.seed = args.get(i + 1).and_then(|s| s.parse().ok());
                i += 2;
            }
            "--threads" => {
                threads = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(4);
                i += 2;
            }
            other => {
                eprintln!("未知参数: {other}");
                i += 1;
            }
        }
    }

    let model_dir = std::env::var("LUX_TTS_MODEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(PathBuf::from)
                .map(|h| h.join(".local/share/slint-demo/models/lux-tts"))
                .unwrap_or_else(|_| PathBuf::from("models/lux-tts"))
        });
    println!("模型目录: {}", model_dir.display());

    // 精度变体: 文件后缀与 --model 一一对应(官方扁平布局)
    let precision = match model.as_str() {
        "int8" => Some("int8"),
        "fp16" => Some("fp16"),
        _ => None,
    };

    let t0 = std::time::Instant::now();
    let tts = slint_lux_tts::LuxTTS::load_precision(&model_dir, threads, precision)?;
    println!("模型加载: {:.1}s", t0.elapsed().as_secs_f32());

    let t0 = std::time::Instant::now();
    let prompt = tts.encode_prompt_file(&ref_path, transcript)?;
    println!(
        "prompt: {} 帧({:.2}s 有效语音), {} tokens, rms={:.3}, 耗时 {:.2}s",
        prompt.features_len,
        prompt.speech_secs,
        prompt.tokens.len(),
        prompt.rms,
        t0.elapsed().as_secs_f32()
    );

    let t0 = std::time::Instant::now();
    let audio = tts.generate(text, &prompt, &opts)?;
    let dur = audio.len() as f32 / 48000.0;
    let wall = t0.elapsed().as_secs_f32();
    println!("合成: {dur:.2}s 音频 / 耗时 {wall:.1}s → RTF {:.2}", wall / dur);

    slint_lux_tts::write_wav_48k(&out_path, &audio)?;
    println!("已写出: {}", out_path.display());
    Ok(())
}
