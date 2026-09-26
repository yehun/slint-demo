//! slint-tts: 基于 LuxTTS(ZipVoice 蒸馏)权重的本地零样本语音克隆(UI 功能名 clone-tts)。
//!
//! 架构: 参考音频 → prompt(24kHz 100-mel, 免转写); 文本 → jieba+声调变调 G2P →
//! text_encoder → fm_decoder(8 步锚点 ODE, CFG 3.0) → vocos(48k+24k 双路) →
//! Linkwitz-Riley 12kHz 交叉合并 → 48kHz 输出。
//!
//! G2P 数据表(data/)由 scripts/gen_fixtures.py 从 pypinyin 生成; 对拍夹具
//! (tests/fixtures/)与 Python 参考实现逐项对齐(tokens/randn/mel/端到端种子波形)。

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
