//! slint-lux-tts: 基于 LuxTTS 权重的本地零样本语音克隆。
//!
//! 架构: 参考音频+转写 → prompt(24kHz 100-mel) ; 文本 → jieba+声调变调 G2P →
//! text_encoder → fm_decoder(8 步锚点 ODE, CFG 3.0) → vocos(48k+24k 双路) →
//! Linkwitz-Riley 12kHz 交叉合并 → 48kHz 输出。
//!
//! 三个 ONNX 模型(text_encoder / fm_decoder / vocos)体积大, 不随仓库分发,
//! 由调用方放进模型目录后传给 [`LuxTTS::load`]。G2P 数据表(data/)由 pypinyin
//! 生成后随 crate 内嵌。

pub mod audio;
pub mod en_normalizer;
pub mod inference;
pub mod mel;
pub mod normalizer;
pub mod pinyin;
pub mod randn;
pub mod tokenizer;
pub mod vocoder;

pub use audio::{decode_file, encode_wav_bytes, write_wav_48k};
pub use inference::{GenOpts, LuxTTS, Prompt};
pub use tokenizer::Tokenizer;
