// 第十六篇 — 模型加载 / 参考音频 / 合成 / 保存
//
// 全部耗时动作都在 std::thread 里跑:
//   - 加载模型: text_encoder / fm_decoder / vocos 三个 ONNX 灌进 ort, 最快也要好几秒
//   - 合成:     文本 G2P → text_encoder → fm_decoder(锚点 ODE 流匹配) → vocos 双路声码器 → 48kHz
// 完成后用 AppState::ui 把结果写回 Slint 属性。
// 免转写: 只需参考音频(抽音色)+ 要合成的话, 无需参考文本。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use slint::ComponentHandle;
use slint_file_picker::{pick_file, FileFilter, PickResult};
use slint_tts::{LuxTTS, GenOpts};

use crate::state::AppState;
use crate::{CloneModel, MainWindow};

/// 推理线程数(ORT intra-op)
const THREADS: usize = 4;

/// 模型目录发现顺序:
///   1. 环境变量 LUX_TTS_MODEL_DIR / CLONE_TTS_MODEL_DIR
///   2. ~/.local/share/slint-demo/models/lux-tts
///   3. 工作目录下的 models/lux-tts
///   4. ~/.local/share/yehun-slint/models/lux-tts(旧位置, 兜底)
/// 权重文件仓库不分发(见 scripts/fetch-model.sh)
fn default_model_dir() -> Option<PathBuf> {
    for var in ["LUX_TTS_MODEL_DIR", "CLONE_TTS_MODEL_DIR"] {
        if let Ok(dir) = std::env::var(var) {
            let p = PathBuf::from(dir);
            if p.is_dir() {
                return Some(p);
            }
        }
    }
    let home_models = std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".local/share/slint-demo/models/lux-tts"));
    if let Some(p) = home_models {
        if p.is_dir() {
            return Some(p);
        }
    }
    let local = PathBuf::from("models/lux-tts");
    if local.is_dir() {
        return Some(local);
    }
    let legacy = std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".local/share/yehun-slint/models/lux-tts"));
    if let Some(p) = legacy {
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

/// 加载 LuxTTS: 优先 int8(更小更快), 缺 int8 权重时回退 fp32。
fn load_engine(dir: &Path, threads: usize) -> anyhow::Result<LuxTTS> {
    match LuxTTS::load_precision(dir, threads, Some("int8")) {
        Ok(e) => Ok(e),
        Err(_) => LuxTTS::load(dir, threads),
    }
}

pub fn bind(app: &MainWindow, state: Arc<AppState>) {
    let s = state.clone();
    app.global::<CloneModel>().on_load_model(move || {
        if s.is_busy() {
            return;
        }
        // 自动发现不到就让用户指路: 选模型目录里的 tokens.txt, 取它的父目录
        match default_model_dir() {
            Some(dir) => load_model(s.clone(), dir),
            None => pick_model_dir(s.clone()),
        }
    });

    let s = state.clone();
    app.global::<CloneModel>().on_pick_reference(move || {
        if s.is_busy() {
            return;
        }
        pick_reference(s.clone());
    });

    let s = state.clone();
    app.global::<CloneModel>().on_synthesize(move || {
        if s.is_busy() {
            return;
        }
        synthesize(s.clone());
    });

    let s = state.clone();
    app.global::<CloneModel>().on_save(move || save(s.clone()));
}

/// 启动时自动发现模型目录(有就直接加载, 没有就留一句提示)
pub fn auto_discover_model(state: &Arc<AppState>) {
    match default_model_dir() {
        Some(dir) => {
            let shown = dir.clone();
            state.ui(move |app| {
                app.global::<CloneModel>()
                    .set_model_status(format!("发现模型目录: {}", shown.display()).into());
            });
            load_model(state.clone(), dir);
        }
        None => state.ui(|app| {
            app.global::<CloneModel>().set_model_status(
                "未找到模型目录。设置 LUX_TTS_MODEL_DIR, 或点“加载模型”手动选择。".into(),
            );
        }),
    }
}

fn pick_model_dir(state: Arc<AppState>) {
    state.set_status("请选择模型目录里的 tokens.txt");
    pick_file(
        vec![
            FileFilter::new("模型词表").extension("txt").mime("text/plain"),
            FileFilter::new("ONNX 模型").extension("onnx").mime("*/*"),
            FileFilter::new("所有文件").mime("*/*"),
        ],
        move |result| match result {
            PickResult::Picked(p) => {
                let path = PathBuf::from(p.to_string());
                let dir = if path.is_dir() {
                    path
                } else {
                    path.parent().map(Path::to_path_buf).unwrap_or(path)
                };
                load_model(state.clone(), dir);
            }
            PickResult::Cancelled => {}
            PickResult::Error(e) => state.set_status(format!("选择失败: {e}")),
        },
    );
}

fn load_model(state: Arc<AppState>, dir: PathBuf) {
    *state.model_dir.lock().unwrap() = Some(dir.clone());
    state.set_busy(true);
    state.set_status("正在加载模型…");
    let loading = dir.clone();
    state.ui(move |app| {
        app.global::<CloneModel>()
            .set_model_status(format!("加载中: {}", loading.display()).into());
    });

    std::thread::spawn(move || {
        let dir_disp = dir.display().to_string();
        // 包一层 catch_unwind: ort 在找不到动态库时会直接 panic(而非返回 Err),
        // 若直接崩溃在后台线程, UI 会永远停在"加载中"。这里把它转成可见的失败提示。
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            load_engine(&dir, THREADS).map(Arc::new)
        }));
        match result {
            Ok(Ok(engine)) => {
                *state.engine.lock().unwrap() = Some(engine);
                state.set_busy(false);
                state.set_status("模型已就绪, 选一段参考音频吧");
                state.ui(move |app| {
                    let m = app.global::<CloneModel>();
                    m.set_model_ready(true);
                    m.set_model_status(format!("已加载: {dir_disp}").into());
                });
            }
            Ok(Err(e)) => {
                state.set_busy(false);
                state.set_status(format!("加载失败: {e}"));
                state.ui(move |app| {
                    app.global::<CloneModel>()
                        .set_model_status(format!("加载失败: {e}").into());
                });
            }
            Err(_) => {
                state.set_busy(false);
                state.set_status(
                    "加载崩溃: 推理库初始化失败, 请确认已安装 onnxruntime 或设置 ORT_DYLIB_PATH",
                );
                state.ui(move |app| {
                    app.global::<CloneModel>().set_model_status(
                        "加载崩溃: 未找到 onnxruntime 动态库(onnxruntime.so), 请安装后重试".into(),
                    );
                });
            }
        }
    });
}

fn pick_reference(state: Arc<AppState>) {
    pick_file(
        vec![
            FileFilter::new("音频文件")
                .extension("wav")
                .extension("mp3")
                .extension("flac")
                .extension("m4a")
                .extension("ogg")
                .mime("audio/*"),
            FileFilter::new("所有文件").mime("*/*"),
        ],
        move |result| match result {
            PickResult::Picked(p) => apply_reference(state.clone(), PathBuf::from(p.to_string())),
            PickResult::Cancelled => {}
            PickResult::Error(e) => state.set_status(format!("选择失败: {e}")),
        },
    );
}

fn apply_reference(state: Arc<AppState>, path: PathBuf) {
    // 读时长/采样率只为给 UI 一句提示(真正的 mel 在合成时才算)
    let info = match slint_tts::decode_file(&path) {
        Ok((samples, sr)) => {
            let secs = samples.len() as f32 / sr as f32;
            format!("{secs:.1}s · {sr}Hz · {} 采样点", samples.len())
        }
        Err(e) => format!("无法解析音频: {e}"),
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());

    *state.ref_path.lock().unwrap() = Some(path);
    state.ui(move |app| {
        let m = app.global::<CloneModel>();
        m.set_ref_name(name.into());
        m.set_ref_info(info.into());
    });
    state.set_status("参考音频已选择, 直接写要合成的话(无需转写)");
    state.refresh_ref_ready();
}

fn synthesize(state: Arc<AppState>) {
    let Some(dir) = state.model_dir.lock().unwrap().clone() else {
        state.set_status("请先加载模型");
        return;
    };
    let Some(ref_path) = state.ref_path.lock().unwrap().clone() else {
        state.set_status("请先选择参考音频");
        return;
    };

    // UI 上的输入(文本 / 语速 / 步数)在事件循环里读一次, 之后全交给工作线程
    let (text, speed, steps) = match state.weak_upgrade() {
        Some(app) => {
            let m = app.global::<CloneModel>();
            (
                m.get_synth_text().to_string(),
                m.get_speed(),
                m.get_steps(),
            )
        }
        None => return,
    };
    if text.trim().is_empty() {
        state.set_status("请输入要合成的文本");
        return;
    }

    state.set_busy(true);
    state.set_status("合成中: 文本编码 → 流匹配 ODE → 声码器…");

    std::thread::spawn(move || {
        let result = run(&state, &dir, &ref_path, &text, speed, steps);
        match result {
            Ok(samples) => {
                let secs = samples.len() as f32 / 48000.0;
                *state.last.lock().unwrap() = Some(samples);
                state.set_busy(false);
                state.set_status(format!("合成完成: {secs:.2}s"));
                state.ui(move |app| {
                    let m = app.global::<CloneModel>();
                    m.set_has_result(true);
                    m.set_result_info(
                        format!("48kHz 单声道 · {secs:.2}s · {} 字", text.chars().count()).into(),
                    );
                    m.set_progress(1.0);
                });
            }
            Err(e) => {
                state.set_busy(false);
                state.set_status(format!("合成失败: {e}"));
            }
        }
    });
}

fn run(
    state: &Arc<AppState>,
    dir: &Path,
    ref_path: &Path,
    text: &str,
    speed: f32,
    steps: i32,
) -> anyhow::Result<Vec<f32>> {
    // 引擎可能还没加载完(或上一次加载失败), 这里兜底加载一次
    let engine = {
        let guard = state.engine.lock().unwrap();
        match guard.as_ref() {
            Some(e) => e.clone(),
            None => {
                drop(guard);
                let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    load_engine(dir, THREADS)
                }));
                let e = match loaded {
                    Ok(Ok(tts)) => Arc::new(tts),
                    Ok(Err(e)) => return Err(e),
                    Err(_) => {
                        return Err(anyhow::anyhow!(
                            "模型推理库(onnxruntime)初始化失败, 请确认已安装 onnxruntime 或设置 ORT_DYLIB_PATH"
                        ))
                    }
                };
                *state.engine.lock().unwrap() = Some(e.clone());
                state.ui(|app| app.global::<CloneModel>().set_model_ready(true));
                e
            }
        }
    };

    let prompt = engine.encode_prompt_file(ref_path)?;
    log::info!(
        "参考音频有效语音 {:.2}s, prompt {} 帧",
        prompt.speech_secs,
        prompt.features_len
    );

    let opts = GenOpts {
        num_steps: steps.max(1) as usize,
        speed,
        ..Default::default()
    };
    engine.generate(text, &prompt, &opts)
}

fn save(state: Arc<AppState>) {
    let Some(samples) = state.last.lock().unwrap().clone() else {
        state.set_status("没有可保存的合成结果");
        return;
    };
    let dir = PathBuf::from("output");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        state.set_status(format!("创建输出目录失败: {e}"));
        return;
    }
    let name = format!(
        "clone-{}.wav",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    );
    let path = dir.join(name);
    match slint_tts::write_wav_48k(&path, &samples) {
        Ok(()) => state.set_status(format!("已保存: {}", path.display())),
        Err(e) => state.set_status(format!("保存失败: {e}")),
    }
}
