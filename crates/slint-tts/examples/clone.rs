//! CLI: LuxTTS 语音克隆
//!
//! 用法:
//!   cargo run --release -p slint-tts --example clone -- \
//!     <ref.wav> <合成文本> <output.wav> [--model int8|fp32] [--steps 8] [--seed N]
//!
//! 免转写: 只需参考音频 + 要合成的话, 无需参考文本。
//! 模型目录: ~/.local/share/yehun-slint/models/lux-tts/ (CLONE_TTS_MODEL 选择精度变体)

use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!(
            "用法: {} <ref.wav> <合成文本> <output.wav> [--model int8] [--steps 8] [--seed N] [--threads N]",
            args[0]
        );
        std::process::exit(2);
    }
    let ref_path = PathBuf::from(&args[1]);
    let text = &args[2];
    let out_path = PathBuf::from(&args[3]);

    let mut model = "fp32".to_string();
    let mut opts = slint_tts::GenOpts::default();
    let mut threads = 2usize;
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
                threads = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(2);
                i += 2;
            }
            "--chunk" => {
                opts.chunk_max_tokens = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(30);
                i += 2;
            }
            other => {
                eprintln!("未知参数: {other}");
                i += 1;
            }
        }
    }

    let model_dir = std::env::var("CLONE_TTS_MODEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs_fallback()
        });
    let model_dir = model_dir.join(&model);
    // tokens.txt 与 text_encoder/fm_decoder/vocos 在精度子目录(历史布局)或根目录均可
    let model_dir = if model_dir.join("tokens.txt").exists() {
        model_dir
    } else {
        model_dir.parent().unwrap_or(&model_dir).to_path_buf()
    };
    println!("模型目录: {}", model_dir.display());

    let t0 = std::time::Instant::now();
    // 精度变体直接映射到 _int8/_fp16 后缀文件(官方扁平布局), 默认 fp32
    let precision = match model.as_str() {
        "int8" => Some("int8"),
        "fp16" => Some("fp16"),
        _ => None,
    };
    let tts = slint_tts::LuxTTS::load_precision(&model_dir, threads, precision)?;
    println!("模型加载: {:.1}s", t0.elapsed().as_secs_f32());

    let t0 = std::time::Instant::now();
    let prompt = tts.encode_prompt_file(&ref_path)?;
    println!(
        "prompt: {} 帧({:.2}s), {} tokens, rms={:.3}, {:.1}s",
        prompt.features.len(),
        prompt.features_len as f32 * 256.0 / 24000.0,
        prompt.tokens.len(),
        prompt.rms,
        t0.elapsed().as_secs_f32()
    );

    let t0 = std::time::Instant::now();
    let audio = tts.generate(text, &prompt, &opts)?;
    let dur = audio.len() as f32 / 48000.0;
    let wall = t0.elapsed().as_secs_f32();
    println!("合成: {dur:.2}s 音频, {wall:.1}s, RTF {:.2}", wall / dur);

    slint_tts::audio::write_wav_48k(&out_path, &audio)?;
    println!("已写出: {}", out_path.display());
    Ok(())
}

fn dirs_fallback() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        let p = PathBuf::from(home)
            .join(".local/share/yehun-slint/models/lux-tts");
        if p.exists() {
            return p;
        }
    }
    PathBuf::from(".")
}
