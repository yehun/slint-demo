//! LuxTTS ONNX 推理: text_encoder → fm_decoder(锚点 ODE) → vocos 双路 → 交叉合并。
//!
//! 与 Python 参考实现(luxtts-onnx)逐步对齐:
//! speed×1.3 / MIN_GEN_FRAMES 边缘填充守卫 / speech_condition 零填充 /
//! 锚点 ODE 更新 / prompt 切除 / [B,100,T]/FEAT_SCALE 转置 / 峰值 clamp / 音量匹配。

use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result, anyhow, bail};
use ndarray::arr0;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

use crate::audio;
use crate::mel;
use crate::randn::Gauss;
use crate::tokenizer::Tokenizer;
use crate::vocoder;

pub const TARGET_RMS_PROMPT: f32 = 0.01;
pub const TARGET_RMS_OUT: f32 = 0.1;
const MIN_GEN_FRAMES: usize = 16; // ~170ms @ hop=256, sr=24000
/// 参考音频音量低于此值(RMS,约 −30dB)就视为"过轻",不再做等比音量匹配。
///
/// 手机录音音量偏小、或声纹库留存的原始录音可能只有 rms 0.005(正常语音 ≥0.05)。
/// 这种参考下,模型生成的音频本身就很轻(实测 rms≈0.018),再做"输出不响于参考"
/// 的等比衰减只会让它更轻 —— 实测输出 rms 0.005(≈−46dB),用户反馈"第一个字
/// 听不到",其实是整段都几乎没声。
const MIN_OUTPUT_RMS: f32 = 0.03;
/// 参考过轻时把输出归一到目标音量的**最大放大倍数**(约 +18dB)。
/// 限制它是为了避免把模型输出里的底噪一起放大成嘶声。
const MAX_REFERENCE_BOOST: f32 = 8.0;
/// 语速校准的生成帧数**下限**(≈0.6s)。
///
/// 旧下限是 `MIN_GEN_FRAMES + 8` = 24 帧(0.26s),对"你好"这种两字词远远不够:
/// 实测目标停在 26 帧、生成 0.29s,模型连第一个字都来不及发出来(ASR 输出为空);
/// 十来个字的短句也会丢掉开头(缺"今天"、"大家好"被吞)。
///
/// 只影响"本来预测就短"的场景 —— 文本一长,target 由 `ids.len() × 6.6` 主导,
/// 下限不起作用,所以长文本(含端到端对拍基准)的输出完全不变。
const MIN_TARGET_FRAMES: f32 = 56.0;

pub struct GenOpts {
    pub num_steps: usize,
    pub t_shift: f32,
    pub guidance_scale: f32,
    pub speed: f32,
    pub seed: Option<u64>,
    /// 长文本分段合成的 token 上限(attention O(frames²), 过长会 OOM; 0=不分段)
    pub chunk_max_tokens: usize,
    /// 自适应语速校准: 预测时长低于自然语速时自动放慢(修复短文本语速过快/发音塌缩)
    pub auto_speed: bool,
}

impl Default for GenOpts {
    fn default() -> Self {
        Self {
            num_steps: 8,
            t_shift: 0.9,
            guidance_scale: 3.0,
            speed: 1.0,
            seed: None,
            chunk_max_tokens: 100,
            auto_speed: true,
        }
    }
}

pub struct Prompt {
    pub tokens: Vec<i64>,
    /// [T][100] log-mel ×FEAT_SCALE
    pub features: Vec<Vec<f32>>,
    pub features_len: i64,
    pub rms: f32,
    /// 参考音频中**有效语音**的时长(秒;裁掉首尾静音后、截断前的长度)。
    /// 供 UI/调用方判断素材质量(过短时克隆效果差)。
    pub speech_secs: f32,
}


/// 合成输出采样率(vocos 双路交叉合并后的结果)
const OUT_SAMPLE_RATE: u32 = 48000;

/// 裁掉合成音频**开头**的静音。
///
/// 参考音频首部带静音时,模型会在生成的开头先走一段静音过渡(实测 0.25~0.40s),
/// 听感就是"第一个字没出声"。这段静音的**来源**动不了:prompt 一个字都不能裁
/// (裁了会改 text_encoder 的时长预测,语速当场崩,见 `encode_prompt`);而把
/// 首部静音"换成语音"更糟 —— 模型会把它当成要延续的内容读出来(实测生成开头
/// 冒出参考文本里的字)。
///
/// 但它本来就只是一段静音,在**输出端**去掉没有任何副作用:不改生成过程、
/// 不改语速、不引入串音,只是让第一个字立刻出声。
///
/// 阈值取峰值的 1% 并保留 20ms 余量:轻辅音起头的字(如"丝""思")起始能量很低,
/// 判狠了会把它的起音一起切掉。整段近乎无声(找不到有声窗)时不动。
///
/// 返回裁掉的样本数(0 = 没裁),供诊断日志使用。
fn trim_head_silence(pcm: &mut Vec<f32>, sample_rate: u32) -> usize {
    let win = (sample_rate as usize / 100).max(1); // 10ms
    if pcm.len() < win * 20 {
        return 0;
    }
    let nwin = pcm.len() / win;
    let mut peak = 0.0f32;
    let mut rms = Vec::with_capacity(nwin);
    for i in 0..nwin {
        let seg = &pcm[i * win..(i + 1) * win];
        let mut sum = 0.0f64;
        for &s in seg {
            sum += (s as f64) * (s as f64);
        }
        let r = (sum / seg.len() as f64).sqrt() as f32;
        if r > peak {
            peak = r;
        }
        rms.push(r);
    }
    let th = peak * 0.01;
    if th <= 0.0 {
        return 0;
    }
    let Some(first) = rms.iter().position(|v| *v >= th) else {
        return 0;
    };
    if first == 0 {
        return 0; // 本来就立刻出声
    }
    let keep = (sample_rate as usize / 50).max(1); // 保留 20ms 余量
    let cut = (first * win).saturating_sub(keep);
    // 超过 1 秒的"静音"更像是合成异常,不敢乱裁
    if cut > 0 && cut <= sample_rate as usize && cut < pcm.len() {
        pcm.drain(..cut);
        return cut;
    }
    0
}

/// 开头能量爬升的最大补偿倍数(约 +26dB)。
///
/// 实测:参考音频起音柔和时,生成的开头首段只有峰值的 5%,要 80ms 才爬上来,
/// 听感就是"第一个字没发音";按 20× 补偿后首段到 43%、ASR 也能完整识别出开头。
/// 倍数不能再大 —— 再往上抬,开头那点底噪就该听见了。
/// (下限由 [`ONSET_MIN_SIGNAL`] 兜:几乎无声的开头不放大。)
const MAX_ONSET_BOOST: f32 = 20.0;
/// 首段能量低于峰值的该比例时,视为静音/底噪而非"弱起音",不放大
/// (静音交给 `trim_head_silence` 处理)
const ONSET_MIN_SIGNAL: f32 = 0.005;

/// 补偿开头那段"能量爬升"。
///
/// 现象:生成的音频第一个字起音很轻甚至听不见 —— 实测开头 20ms 只有峰值的
/// 5%,要 80ms 才爬到峰值,听感就是"首字没发音"(后面的字正常)。
/// 这是模型在复刻参考音频的起音方式:参考若本身是柔和/气声起音,生成的开头
/// 也会弱(实测同一段文本、同一个 seed,换一个起音干脆的参考,开头立刻变成
/// 峰值的 65% 以上)。
///
/// 关键:这段采样点是**有效语音**,只是幅度低 —— 所以**放大**而不是裁掉。
/// 裁掉会真的丢掉第一个字(试过,更糟)。
///
/// 只在"开头 150ms 内就能达到峰值一半"时介入:爬升过慢说明开头另有问题
/// (或者根本不是起音),不该贸然抬增益。
fn compensate_weak_onset(pcm: &mut [f32], sample_rate: u32) {
    let win = (sample_rate as usize / 100).max(1); // 10ms
    let nwin = pcm.len() / win;
    if nwin < 20 {
        return;
    }
    let rms: Vec<f32> = (0..nwin)
        .map(|i| {
            let seg = &pcm[i * win..(i + 1) * win];
            (seg.iter().map(|v| v * v).sum::<f32>() / seg.len() as f32).sqrt()
        })
        .collect();
    let peak = rms.iter().cloned().fold(0.0f32, f32::max);
    if peak <= 0.0 {
        return;
    }
    // 首次达到峰值一半的位置(以窗计)
    let Some(onset) = rms.iter().position(|v| *v >= 0.5 * peak) else {
        return;
    };
    let max_onset_win = (150 / 10).max(1); // 150ms
    if onset == 0 || onset > max_onset_win {
        return; // 已经干脆起音,或爬升异常
    }
    // 首段几乎无声 → 是静音或底噪,不是"弱起音",放大只会得到更响的噪声
    if rms[0] < ONSET_MIN_SIGNAL * peak {
        return;
    }
    let g0 = (0.5 * peak / rms[0]).min(MAX_ONSET_BOOST);
    if g0 <= 1.05 {
        return; // 首段本来就够响
    }
    let span = onset * win;
    for (i, v) in pcm.iter_mut().take(span).enumerate() {
        // 增益从 g0 线性回到 1.0,避免出现突兀的台阶
        let t = i as f32 / span as f32;
        let g = g0 + (1.0 - g0) * t;
        *v = (*v * g).clamp(-1.0, 1.0);
    }
    log::info!(
        "[clone-tts] 开头起音偏弱(首段仅为峰值的 {:.0}%),已按 {g0:.1}× 补偿 {}ms",
        rms[0] / peak * 100.0,
        span * 1000 / sample_rate as usize,
    );
}

/// 测量**有效语音**的样本数(按短窗能量去掉首尾静音后的长度)。
///
/// ⚠️ 这个值**只用来提示素材质量**(见 [`Prompt::speech_secs`]),绝不能拿它去
/// 裁剪真正喂给模型的音频 —— 裁剪会缩短 prompt,而 text_encoder 的时长预测
/// 依赖 prompt 长度,结果是生成音频变短、语速变快、尾部字被吃掉。
///
/// 阈值自适应:max(peak × 2%, 1e-5),与 speech 插件的 `trim_silence` 同思路 ——
/// 绝对阈值在不同录音设备上会失准,用本段自身的峰值做参照更稳。
fn speech_span(samples: &[f32], sample_rate: u32) -> usize {
    let win = (sample_rate as usize * 30 / 1000).max(1); // 30ms
    if samples.len() <= win * 4 {
        return samples.len();
    }
    let nwin = samples.len() / win;
    let mut peak = 0.0f32;
    let mut rms = Vec::with_capacity(nwin);
    for i in 0..nwin {
        let seg = &samples[i * win..(i + 1) * win];
        let mut sum = 0.0f64;
        for &s in seg {
            sum += (s as f64) * (s as f64);
        }
        let r = (sum / seg.len() as f64).sqrt() as f32;
        if r > peak {
            peak = r;
        }
        rms.push(r);
    }
    let th = (peak * 0.02).max(1e-5);
    let (Some(first), Some(last)) = (
        rms.iter().position(|v| *v >= th),
        rms.iter().rposition(|v| *v >= th),
    ) else {
        return 0; // 全静音
    };
    // 首尾窗口都有声 ⇒ 整段都是语音,直接给全长(避免整窗对齐少算掉尾部余量)
    if first == 0 && last == nwin - 1 {
        return samples.len();
    }
    let start = first.saturating_sub(1) * win;
    let end = ((last + 2).min(nwin)) * win;
    end.min(samples.len()).saturating_sub(start)
}

struct Sessions {
    te: Mutex<Session>,
    fm: Mutex<Session>,
    vocos: Mutex<Session>,
}

pub struct LuxTTS {
    sessions: Sessions,
    pub tokenizer: Tokenizer,
    feat_dim: usize,
}

/// 桌面端 onnxruntime 动态库探测(ORT_DYLIB_PATH / exe 目录 / exe 目录/lib、/libs / ./lib、./libs), Android 编译期链接跳过。
///
/// 只初始化一次:ort 的 init 是可重入的,但**失败**会每次都重新探测一遍路径
/// 并重复报错(加载大模型时这条错误会刷屏)。语音识别插件与它同进程时也可能
/// 各调一次,缓存结果更省事。
pub fn ensure_ort_library() -> Result<()> {
    #[cfg(target_os = "android")]
    {
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        use std::sync::OnceLock;
        static INIT: OnceLock<Result<(), String>> = OnceLock::new();
        INIT.get_or_init(|| {
            if let Some(p) = find_ort_library_path() {
                ort::init_from(&p)
                    .map(|_| ())
                    .map_err(|e| format!("加载 onnxruntime 库 {p:?} 失败: {e}"))
            } else {
                Err("未找到 onnxruntime 动态库, 请设置 ORT_DYLIB_PATH 或放入程序目录/lib".to_string())
            }
        })
        .clone()
        .map_err(|e| anyhow!("{e}"))
    }
}

#[cfg(not(target_os = "android"))]
fn find_ort_library_path() -> Option<std::path::PathBuf> {
    let name = if cfg!(target_os = "windows") {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    };
    if let Ok(p) = std::env::var("ORT_DYLIB_PATH") {
        let p = std::path::PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            dirs.push(dir.to_path_buf());
            dirs.push(dir.join("lib"));
            dirs.push(dir.join("libs"));
        }
    }
    dirs.push(std::path::PathBuf::from("./lib"));
    dirs.push(std::path::PathBuf::from("./libs"));
    dirs.push(std::path::PathBuf::from("./"));
    // 开发机约定: 自编译的 libonnxruntime.so 放在 ~/.local/lib (干净自包含, 避免系统 1.21 的
    // 退出 139 / schema 刷屏问题)。`make run-linux` 从这里启动二进制也能直接命中, 无需设 ORT_DYLIB_PATH。
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(std::path::PathBuf::from(home).join(".local/lib"));
    }
    for dir in dirs {
        let p = dir.join(name);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn build_session(path: &Path, threads: usize) -> Result<Session> {
    let builder = Session::builder().map_err(|e| anyhow!("创建 ONNX 会话失败: {e}"))?;
    let builder = builder
        .with_optimization_level(GraphOptimizationLevel::Level1)
        .map_err(|e| anyhow!("设置优化级别失败: {e}"))?;
    let mut builder = builder
        .with_intra_threads(threads.max(1))
        .map_err(|e| anyhow!("设置线程数失败: {e}"))?;
    builder
        .commit_from_file(path)
        .map_err(|e| anyhow!("加载 ONNX 模型失败 ({}): {e}", path.display()))
}

impl LuxTTS {
    pub fn load(model_dir: &Path, threads: usize) -> Result<Self> {
        Self::load_precision(model_dir, threads, None)
    }

    /// precision: None=fp32 官方命名(text_encoder.onnx/fm_decoder.onnx);
    /// Some("int8")=text_encoder_int8.onnx/fm_decoder_int8.onnx(体积/内存更小,CPU 更快)
    pub fn load_precision(model_dir: &Path, threads: usize, precision: Option<&str>) -> Result<Self> {
        ensure_ort_library()?;
        let (te_name, fm_name) = match precision {
            None | Some("") => ("text_encoder.onnx".to_string(), "fm_decoder.onnx".to_string()),
            Some(p) => (format!("text_encoder_{p}.onnx"), format!("fm_decoder_{p}.onnx")),
        };
        let te_path = model_dir.join(&te_name);
        let fm_path = model_dir.join(&fm_name);
        let vocos_path = model_dir.join("vocos.onnx");
        let tok_path = model_dir.join("tokens.txt");
        if !te_path.is_file() || !fm_path.is_file() {
            bail!("模型文件缺失: {} / {}", te_path.display(), fm_path.display());
        }
        log::info!("LuxTTS 权重: {} / {}", te_name, fm_name);
        let sessions = Sessions {
            te: Mutex::new(build_session(&te_path, threads)?),
            fm: Mutex::new(build_session(&fm_path, threads)?),
            vocos: Mutex::new(build_session(&vocos_path, threads)?),
        };
        let tokenizer = Tokenizer::load(&tok_path)?;
        // feat_dim 固定 100(与 fm_decoder 元数据一致)
        Ok(Self { sessions, tokenizer, feat_dim: mel::N_MELS })
    }

    /// 参考音频 → prompt(24kHz 单声道采样序列;超 15s 截断)。免转写: prompt 只含参考音频的 mel, 不含任何文本 token。
    ///
    /// ⚠️ **绝不要裁剪参考音频**。这里必须与 Python 参考实现
    /// `encode_prompt(duration=15.0)` 保持一致(超长只截前 15s)。
    ///
    /// 曾经试过"先裁首尾静音再取语音最密窗口"来提升克隆相似度,**结果把语速
    /// 搞坏了**:text_encoder 的时长预测依赖 prompt 长度 —— 同一段文本,参考
    /// 音频 10.2s(含静音)时生成 1.58s,把静音裁掉变成 6.7s 后只生成 1.34s。
    /// prompt 变短 → 生成变短 → 语速变快、尾部字被吃掉。宁可让 prompt 里带点
    /// 静音,也不能动它的长度。
    ///
    /// 静音只用来**测量**素材质量(见 [`Prompt::speech_secs`]),不参与推理。
    pub fn encode_prompt(
        &self,
        samples: &[f32],
        sample_rate: u32,
    ) -> Result<Prompt> {
        let mono = if sample_rate == mel::SAMPLE_RATE {
            samples.to_vec()
        } else {
            audio::resample(samples, sample_rate, mel::SAMPLE_RATE)
        };
        if mono.is_empty() {
            bail!("参考音频为空");
        }
        // 与参考实现 encode_prompt(duration=15.0) 一致:超长参考截断(attention 随帧数平方增长)
        const MAX_PROMPT_SECS: usize = 15;
        let max_samples = MAX_PROMPT_SECS * mel::SAMPLE_RATE as usize;

        // 只测量、不改动:有效语音时长供 UI 判断素材质量(太短则克隆效果差)。
        // speeches 不参与推理 —— 见本函数文档里关于"裁剪会改语速"的说明。
        let speech_secs = speech_span(&mono, mel::SAMPLE_RATE) as f32
            / mel::SAMPLE_RATE as f32;
        if speech_secs < 1.0 {
            log::warn!(
                "参考音频有效语音仅 {speech_secs:.2}s,克隆效果会明显变差(建议 ≥3s 干净语音)"
            );
        }

        let mut buf = if mono.len() > max_samples {
            log::info!(
                "参考音频 {:.1}s 超过 {MAX_PROMPT_SECS}s,截取前 {MAX_PROMPT_SECS}s",
                mono.len() as f32 / mel::SAMPLE_RATE as f32
            );
            mono[..max_samples].to_vec()
        } else {
            mono
        };
        let rms = audio::rms_norm(&mut buf, TARGET_RMS_PROMPT);
        let features = mel::extract(&buf);
        let features_len = features.len() as i64;
        // 免转写(无参考文本): 按参考音频时长估算字数, 填等长中性空格 token(只给长度线索, 不给内容)。
        // 音色条件来自参考音频的 mel(speech_condition), 与文本无关, 故无需转写也能保住音色。
        // 中文自然语速约 5 字/秒, 每字 ~2 token(与 split_text_segments 的估算口径一致)。
        let n = ((speech_secs * 5.0).max(1.0) as usize) * 2;
        let tokens = vec![3i64; n]; // token 3 == 空格(中性)
        if tokens.is_empty() {
            bail!("参考音频过短, 无法估算占位 token 长度");
        }
        Ok(Prompt { tokens, features, features_len, rms, speech_secs })
    }

    /// 从文件直接构建 prompt(与 [`Self::encode_prompt`] 同一条路径)
    pub fn encode_prompt_file(&self, path: &Path) -> Result<Prompt> {
        let (samples, sr) = audio::decode_file(path)?;
        self.encode_prompt(&samples, sr)
    }

    /// 合成(48kHz f32, 已 clamp 与音量匹配)
    pub fn generate(&self, text: &str, prompt: &Prompt, opts: &GenOpts) -> Result<Vec<f32>> {
        if self.tokenizer.text_to_ids(text).is_empty() {
            bail!("输入文本未产生任何 token");
        }
        // 分段: 优先按标点切自然句段(避免词内硬切), 无标点超长时才按估算 token 硬切;
        // attention 随总帧数平方增长, 超长文本必须分段防 OOM
        let segments = Self::split_text_segments(text, opts.chunk_max_tokens);
        let n_seg = segments.len();
        let mut audio_out: Vec<f32> = Vec::new();
        for (ci, seg) in segments.iter().enumerate() {
            let ids = self.tokenizer.text_to_ids(seg);
            if ids.is_empty() {
                continue;
            }
            let piece = self.generate_chunk(&ids, prompt, opts)
                .with_context(|| format!("分段 {ci}({seg}) 合成失败"))?;
            audio_out.extend_from_slice(&piece);
        }
        // 只在**整体开头**裁:段与段之间的停顿是正常的句间间隔,不该动
        let cut = trim_head_silence(&mut audio_out, OUT_SAMPLE_RATE);
        // 裁完静音后若开头仍是"弱起音"(第一个字被吞的感觉),把它补响
        compensate_weak_onset(&mut audio_out, OUT_SAMPLE_RATE);
        log::info!(
            "[clone-tts] 合成完成: 文本 {} 字 / {} 段 → 输出 {:.2}s, \
             开头裁静音 {:.0}ms (裁后仍偏短说明模型在该文本上生成不足)",
            text.chars().count(),
            n_seg,
            audio_out.len() as f32 / OUT_SAMPLE_RATE as f32,
            cut as f32 * 1000.0 / OUT_SAMPLE_RATE as f32,
        );
        Ok(audio_out)
    }

/// 按标点把文本切成合成段: 优先句末标点(。！？；), 逗号仅在接近上限时才切,
/// 无标点的超长文本按估算 token 硬切(此时无法保证词边界)。
/// 估算: CJK 字 ≈2 token, 其他字符 ≈1 token。
fn split_text_segments(text: &str, max_tokens: usize) -> Vec<String> {
    let max = if max_tokens == 0 { usize::MAX } else { max_tokens };
    // 硬切上限用 saturating_add:max 为 usize::MAX(不分段)时 `max + max/5`
    // 会回绕成一个小值,导致"不分段"被当成"每段都很短"。
    let hard = max.saturating_add(max / 5);
    let mut out: Vec<String> = Vec::new();
    let mut acc = String::new();
    // token 数**增量**累计(旧实现每个字符都 est(&acc) 重扫整段 → O(n²))
    let mut acc_tokens = 0usize;
    for ch in text.chars() {
        acc.push(ch);
        acc_tokens += if ('\u{3400}'..='\u{9fff}').contains(&ch) { 2 } else { 1 };
        let sentence_end = matches!(ch, '。' | '！' | '？' | '；' | '!' | '?' | ';');
        let clause_end = matches!(ch, '，' | '、' | ',' | '…' | ':');
        if (sentence_end && acc_tokens >= max / 3)
            || (clause_end && acc_tokens >= max)
            || acc_tokens >= hard
        {
            out.push(std::mem::take(&mut acc));
            acc_tokens = 0;
        }
    }
    if !acc.is_empty() {
        out.push(acc);
    }
    out
}

/// 单次 text_encoder 前向(语速校准会多次调用)
    fn run_text_encoder(&self, ids: &[i64], prompt: &Prompt, speed: f32) -> Result<(Vec<i64>, Vec<f32>)> {
        let tokens_np = Tensor::from_array((vec![1i64, ids.len() as i64], ids.to_vec()))?;
        let prompt_tokens =
            Tensor::from_array((vec![1i64, prompt.tokens.len() as i64], prompt.tokens.clone()))?;
        let feat_len = Tensor::from_array(arr0(prompt.features_len))?;
        let speed_t = Tensor::from_array(arr0(speed))?;
        let mut te = self.sessions.te.lock().unwrap();
        let outputs = te.run(ort::inputs![
            "tokens" => tokens_np,
            "prompt_tokens" => prompt_tokens,
            "prompt_features_len" => feat_len,
            "speed" => speed_t,
        ])?;
        let (shape, data) = outputs[0].try_extract_tensor::<f32>()?;
        Ok((shape.to_vec(), data.to_vec()))
    }

    fn generate_chunk(&self, ids: &[i64], prompt: &Prompt, opts: &GenOpts) -> Result<Vec<f32>> {
        // ---- text_encoder + 自适应语速校准 ----
        // ---- text_encoder + 自适应语速校准 ----
        // 时长模型对短文本/短分段会塌缩(输出被压缩到语速快/发音不全), 且 speed
        // 对时长杠杆巨大且非线性(实测 0.5→×6.2, 0.65→×3.8, 2.0→塌缩)。
        // 以 ~6.6 帧/token 为自然语速基准, 预测低于 0.75× 时二分搜索 speed
        // (每次仅重跑轻量 text_encoder)。
        let base_speed = opts.speed * 1.3f32;
        let (mut tc_shape, mut tc_data) = self.run_text_encoder(ids, prompt, base_speed)?;
        let prompt_t = prompt.features_len as usize;
        let mut used_speed = base_speed;
        // target 提到外面只为日志可见(它决定"要不要校准、校准到多长")
        let mut target = 0.0f32;
        if opts.auto_speed {
            target = (ids.len() as f32 * 6.6 / opts.speed.max(0.1)).max(MIN_TARGET_FRAMES);
            let mut gen_pred = tc_shape
                .get(1)
                .map(|f| (*f as usize).saturating_sub(prompt_t))
                .unwrap_or(0) as f32;
            // 免转写(placeholder)模式下时长预测偏短, 需要更宽松的触发阈值才能补偿;
            // 固定 0.95(比默认 0.75 更激进地触发语速搜索, 补齐因时长预测偏短而丢失的字)。
            let trigger: f32 = 0.95;
            if ids.len() >= 3 && gen_pred < trigger * target {
                let (mut lo, mut hi) = (0.2f32, base_speed);
                // best = (与目标的差, 预测生成帧数, 权重数据, speed)。
                // 带上 gen_f 是为了下面的"够长优先"判定。
                let mut best: Option<(f32, f32, (Vec<i64>, Vec<f32>), f32)> = Some((
                    (gen_pred - target).abs(),
                    gen_pred,
                    (tc_shape.clone(), tc_data.clone()),
                    base_speed,
                ));
                // "够长"门槛:低于它就说明这段文本会被挤着读完(语速过快、尾巴丢字)
                let enough = trigger * target;
                for _ in 0..4 {
                    let s = ((lo + hi) / 2.0).clamp(0.2, base_speed);
                    let (shape, data) = self.run_text_encoder(ids, prompt, s)?;
                    let gen_f = shape
                        .get(1)
                        .map(|f| (*f as usize).saturating_sub(prompt_t))
                        .unwrap_or(0) as f32;
                    // **优先"够长",再比谁更接近目标**。
                    //
                    // 旧逻辑只比"离目标最近",而 speed→帧数 的曲线是跳变的:
                    // 实测目标 264 帧时,speed 1.30 给 187 帧、1.025 给 375 帧 ——
                    // 375 更远(+111)却也更安全,旧逻辑却选中了 187(近但不够长),
                    // 结果 21 个字挤进 1.99s(10.6 字/秒),尾部字来不及发出来。
                    // 生成不足会让内容读不完,比生成偏长严重得多,所以先保够长。
                    let take = match best.as_ref() {
                        None => true,
                        Some((old_diff, old_gen, _, _)) => {
                            let (old_ok, new_ok) = (*old_gen >= enough, gen_f >= enough);
                            if new_ok != old_ok {
                                new_ok
                            } else {
                                (gen_f - target).abs() < *old_diff
                            }
                        }
                    };
                    if take {
                        best = Some((
                            (gen_f - target).abs(),
                            gen_f,
                            (shape.clone(), data.clone()),
                            s,
                        ));
                    }
                    log::debug!(
                        "[clone-tts] 语速试探 speed={s:.3} → 生成 {gen_f:.0} 帧 (目标 {target:.0}, 够长门槛 {enough:.0})"
                    );
                    if (0.75 * target..=1.8 * target).contains(&gen_f) {
                        break; // 命中自然语速带
                    }
                    if gen_f < target {
                        hi = s; // 仍太快 → 向更小 speed 搜
                    } else {
                        lo = s; // 过慢 → 向更大 speed 搜
                    }
                    gen_pred = gen_f;
                }
                if let Some((_, _, (shape, data), s)) = best {
                    tc_shape = shape;
                    tc_data = data;
                    used_speed = s;
                }
                log::info!(
                    "语速校准: 预测 {gen_pred:.0} 帧 < 目标 {target:.0}, speed {base_speed:.2}→{used_speed:.2}"
                );
            }
        }

        let mut num_frames = *tc_shape.get(1).unwrap_or(&0) as usize;
        if num_frames == 0 {
            bail!("text_encoder 输出 0 帧");
        }

        // 守卫: 预测帧数不足以容纳 prompt 时边缘填充
        let tc_padded;
        let tc_view: (&Vec<i64>, &Vec<f32>) = if num_frames < prompt_t + MIN_GEN_FRAMES {
            let pad = prompt_t + MIN_GEN_FRAMES - num_frames;
            log::warn!(
                "text_encoder 预测 {num_frames} 帧不足以容纳 prompt {prompt_t} 帧, 填充 {pad} 帧"
            );
            let mut shape = tc_shape.clone();
            shape[1] += pad as i64;
            let mut data = tc_data.clone();
            // 末帧复制 pad 次(shape[2]=100)
            let dim = *tc_shape.get(2).unwrap_or(&(mel::N_MELS as i64)) as usize;
            let last = &tc_data[tc_data.len() - dim..];
            for _ in 0..pad {
                data.extend_from_slice(last);
            }
            num_frames += pad;
            tc_padded = (shape, data);
            (&tc_padded.0, &tc_padded.1)
        } else {
            (&tc_shape, &tc_data)
        };

        // ODE 时间表: t_shift * t / (1 + (t_shift-1)*t)
        let mut timesteps = Vec::with_capacity(opts.num_steps + 1);
        for s in 0..=opts.num_steps {
            let t = s as f32 / opts.num_steps as f32;
            timesteps.push(opts.t_shift * t / (1.0 + (opts.t_shift - 1.0) * t));
        }

        // x = randn(1, num_frames, 100)
        let mut x = vec![0.0f32; num_frames * self.feat_dim];
        match opts.seed {
            Some(seed) => {
                let mut g = Gauss::new(seed);
                g.fill_f32(&mut x);
            }
            None => {
                let mut rng = Gauss::new(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos() as u64)
                        .unwrap_or(42),
                );
                rng.fill_f32(&mut x);
            }
        }

        // speech_condition: prompt 特征零填充/截断到 num_frames
        let mut speech_condition = vec![0.0f32; num_frames * self.feat_dim];
        let copy_t = (prompt.features.len() as usize).min(num_frames);
        for t in 0..copy_t {
            speech_condition[t * self.feat_dim..(t + 1) * self.feat_dim]
                .copy_from_slice(&prompt.features[t]);
        }

        // text_condition 与 speech_condition 在整个 ODE 期间都不变,只构造一次、
        // 每步用 `view()` 借用喂进去。
        //
        // 旧实现在每一步都 `.clone()` 两个张量再构造 Tensor —— 一块
        // num_frames×100 的 f32 在 800 帧时约 320KB,8 步就是 5MB 的纯拷贝,
        // 而它们的内容从头到尾一模一样。
        let tc_in = Tensor::from_array((tc_view.0.clone(), tc_view.1.clone()))?;
        let sc_in = Tensor::from_array((
            vec![1i64, num_frames as i64, self.feat_dim as i64],
            speech_condition,
        ))?;

        let mut x1_pred = vec![0.0f32; x.len()];
        let mut x0_pred = vec![0.0f32; x.len()];
        for step in 0..opts.num_steps {
            let t_cur = timesteps[step];
            let t_next = timesteps[step + 1];
            let v = {
                let t_in = Tensor::from_array(arr0(t_cur))?;
                let x_in = Tensor::from_array((vec![1i64, num_frames as i64, self.feat_dim as i64], x.clone()))?;
                let g_in = Tensor::from_array(arr0(opts.guidance_scale))?;
                let mut fm = self.sessions.fm.lock().unwrap();
                let outputs = fm.run(ort::inputs![
                    "t" => t_in,
                    "x" => x_in,
                    "text_condition" => tc_in.view(),
                    "speech_condition" => sc_in.view(),
                    "guidance_scale" => g_in,
                ])?;
                let (_, data) = outputs[0].try_extract_tensor::<f32>()?;
                data.to_vec()
            };

            // 锚点 ODE(原地复用 buffer)
            let inv = 1.0 - t_cur;
            for i in 0..x.len() {
                x1_pred[i] = x[i] + inv * v[i];
                x0_pred[i] = x[i] - t_cur * v[i];
            }
            if step == opts.num_steps - 1 {
                x.copy_from_slice(&x1_pred);
            } else {
                let w1 = t_next;
                let w0 = 1.0 - t_next;
                for i in 0..x.len() {
                    x[i] = w0 * x0_pred[i] + w1 * x1_pred[i];
                }
            }
        }

        // 切除 prompt 部分
        let prompt_len = (prompt.features_len as usize).min(num_frames - 1);
        let gen_frames = num_frames - prompt_len;
        let gen_slice = &x[prompt_len * self.feat_dim..];

        // vocos: 输入 [1, 100, T] / FEAT_SCALE
        let mut vocos_in = vec![0.0f32; gen_slice.len()];
        for t in 0..gen_frames {
            for m in 0..self.feat_dim {
                vocos_in[m * gen_frames + t] = gen_slice[t * self.feat_dim + m] / mel::FEAT_SCALE;
            }
        }
        let vocos_in_t = Tensor::from_array((
            vec![1i64, self.feat_dim as i64, gen_frames as i64],
            vocos_in,
        ))?;
        let (audio_48k, audio_24k) = {
            let mut vo = self.sessions.vocos.lock().unwrap();
            let outputs = vo.run(ort::inputs!["features" => vocos_in_t])?;
            let (s1, d1) = outputs[0].try_extract_tensor::<f32>()?;
            let (s2, d2) = outputs[1].try_extract_tensor::<f32>()?;
            ((s1.to_vec(), d1.to_vec()), (s2.to_vec(), d2.to_vec()))
        };
        let a48 = &audio_48k.1;
        let a24 = &audio_24k.1;
        if a48.is_empty() || a24.is_empty() {
            bail!("vocos 输出为空");
        }
        let mut merged = vocoder::merge(a48, a24);
        // clamp + 音量匹配(只降不升)
        for v in merged.iter_mut() {
            *v = v.clamp(-1.0, 1.0);
        }
        // ---- 音量处理(与参考实现"只降不升"的音量匹配分两条路)----
        if prompt.rms < MIN_OUTPUT_RMS {
            // 参考过轻:等比匹配已失去意义(只会把本来就轻的输出压得更轻),
            // 直接把输出自身归一到目标音量。
            let cur =
                (merged.iter().map(|v| v * v).sum::<f32>() / merged.len().max(1) as f32).sqrt();
            if cur > 1e-6 {
                let gain = (TARGET_RMS_OUT / cur).min(MAX_REFERENCE_BOOST);
                log::warn!(
                    "[clone-tts] 参考音频音量偏低(rms {:.4}, 正常语音 ≥0.05):\
                     输出按 {gain:.2}× 归一到目标音量(等比匹配本会衰减到 {:.3}×,几乎听不见)。\
                     建议换一段更响的参考音频。",
                    prompt.rms,
                    prompt.rms / TARGET_RMS_OUT
                );
                for v in merged.iter_mut() {
                    // 放大后重新 clamp:归一可能把个别样点推过 1.0
                    *v = (*v * gain).clamp(-1.0, 1.0);
                }
            }
        } else if prompt.rms < TARGET_RMS_OUT {
            // 参考音量偏低但可用:保持"输出不响于参考"(只降不升)
            let gain = prompt.rms / TARGET_RMS_OUT;
            for v in merged.iter_mut() {
                *v *= gain;
            }
        }
        // 一段合成的**完整诊断**:出现"丢字/语速不对"时,这一行就能判断是
        // 参考音频太短(参考帧数)、目标太低(target)、还是模型预测不足
        // (生成帧数远小于 target)。配合 `RUST_LOG=info` 使用。
        let sr = mel::SAMPLE_RATE as f32;
        log::info!(
            "[clone-tts] 分段合成: 文本 {} tokens | 参考 {:.2}s / {} 帧 / {} tokens | \
             总帧 {} = 参考 {} + 生成 {} | 目标 {:.0} 帧 | speed {:.2} | 输出 {:.2}s",
            ids.len(),
            prompt_t as f32 * mel::HOP as f32 / sr,
            prompt_t,
            prompt.tokens.len(),
            num_frames,
            prompt_len,
            gen_frames,
            target,
            used_speed,
            merged.len() as f32 / OUT_SAMPLE_RATE as f32,
        );
        Ok(merged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `make run-linux`(及任意开发机启动)应能从 ~/.local/lib 命中自编译的 libonnxruntime.so,
    /// 无需手动设 ORT_DYLIB_PATH。验证发现顺序在开发环境下可达。
    #[cfg(not(target_os = "android"))]
    #[test]
    fn discovers_ort_in_home_local_lib() {
        let found = find_ort_library_path().expect("未找到 libonnxruntime, 检查 ~/.local/lib");
        assert!(
            found.ends_with("libonnxruntime.so"),
            "发现的不是 libonnxruntime.so: {found:?}"
        );
        assert!(
            found.starts_with(std::path::Path::new(&std::env::var("HOME").unwrap()).join(".local/lib")),
            "开发机应优先 ~/.local/lib 的干净自包含构建, 而非系统 1.21: {found:?}"
        );
        assert!(found.exists(), "发现路径不存在: {found:?}");
    }

    /// 确定性"类语音"信号:能量均匀,不含静音
    fn speech(n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| 0.5 * ((i as f32) * 0.05).sin() + 0.2 * ((i as f32) * 0.31).sin())
            .collect()
    }

    /// 开头静音被裁掉(第一个字立即出声),且保留一小段余量避免切掉弱起音
    #[test]
    fn trims_leading_silence_but_keeps_a_margin() {
        let sr = 48000u32;
        let s = sr as usize;
        let mut v = vec![0.0f32; s / 2]; // 0.5s 开头静音
        v.extend_from_slice(&speech(s * 2));
        let orig_len = v.len();
        trim_head_silence(&mut v, sr);

        let cut = orig_len - v.len();
        assert!(cut > 0, "开头静音应被裁掉");
        // 0.5s 静音 - 20ms 余量 ≈ 0.48s;允许 10ms 窗对齐的误差
        let expect = s / 2 - s / 50;
        assert!(
            cut.abs_diff(expect) <= s / 100,
            "裁掉 {cut} 样本,期望约 {expect}"
        );
        // 末尾那 20ms 余量是**有意**保留的(保住弱起音),所以看 50ms 窗口
        assert!(
            v.iter().take(s / 20).any(|x| x.abs() > 0.01),
            "裁完开头应当很快有声"
        );
    }

    /// 本来就立刻出声的音频不能被削掉起音
    #[test]
    fn leaves_immediate_onset_untouched() {
        let sr = 48000u32;
        let mut v = speech(sr as usize * 2);
        let orig = v.clone();
        trim_head_silence(&mut v, sr);
        assert_eq!(v, orig, "开头即有声时不该动");
    }

    /// 全静音输入不裁(交给上游的"合成结果为空/异常"判断)
    #[test]
    fn all_silence_is_left_alone() {
        let sr = 48000u32;
        let mut v = vec![0.0f32; sr as usize];
        let orig = v.clone();
        trim_head_silence(&mut v, sr);
        assert_eq!(v, orig);
    }

    /// 弱起音被抬起来(第一个字能听见了)
    #[test]
    fn boosts_weak_onset() {
        let sr = 48000u32;
        let s = sr as usize;
        let win = s / 100;
        let mut v: Vec<f32> = (0..win).map(|i| 0.01 * ((i as f32) * 0.05).sin()).collect();
        v.extend(speech(s));
        compensate_weak_onset(&mut v, sr);
        let seg0 = (v[..win].iter().map(|x| x * x).sum::<f32>() / win as f32).sqrt();
        assert!(seg0 > 0.05, "首段应被抬起来,实际 {seg0}");
    }

    /// 起音本来就干脆 → 一个采样都不动
    #[test]
    fn leaves_strong_onset_untouched() {
        let sr = 48000u32;
        let mut v = speech(sr as usize * 2);
        let orig = v.clone();
        compensate_weak_onset(&mut v, sr);
        assert_eq!(v, orig);
    }

    /// 首段近乎无声时不放大(否则只是把底噪抬起来)
    #[test]
    fn does_not_boost_near_silence() {
        let sr = 48000u32;
        let s = sr as usize;
        let mut v = vec![0.0f32; s / 100];
        v.extend(speech(s));
        let orig = v.clone();
        compensate_weak_onset(&mut v, sr);
        assert_eq!(v, orig, "近乎无声的开头不该被放大");
    }

    /// 超长"静音"(>1s)视为合成异常,不裁
    #[test]
    fn does_not_cut_more_than_one_second() {
        let sr = 48000u32;
        let s = sr as usize;
        let mut v = vec![0.0f32; s * 2]; // 2s 静音
        v.extend_from_slice(&speech(s));
        let orig = v.clone();
        trim_head_silence(&mut v, sr);
        assert_eq!(v, orig, "超过 1s 的静音不敢裁");
    }
}
