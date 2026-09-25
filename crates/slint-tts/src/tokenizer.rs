//! EmiliaTokenizer 移植: 语言分段 + 中文 G2P(jieba+变调) + 英文 G2P(espeak-ng) + 词表映射。
//!
//! 流程对齐 Python 参考实现:
//!   preprocess_text(标点映射) → get_segment(zh/en/pinyin/tag/other) → 分语言 G2P → token→id

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, bail};
use jieba_rs::Jieba;

use crate::normalizer;
use crate::pinyin::PinyinTables;

pub struct Tokenizer {
    token2id: HashMap<String, i64>,
    pinyin: PinyinTables,
    jieba: Jieba,
    espeak_available: bool,
}

/// 拼写兜底: espeak-ng 不可用时, 英文字母按字母名 IPA 读出
const LETTER_IPA: &[(&str, &str)] = &[
    ("a", "eɪ"), ("b", "biː"), ("c", "siː"), ("d", "diː"), ("e", "iː"), ("f", "ɛf"),
    ("g", "dʒiː"), ("h", "eɪtʃ"), ("i", "aɪ"), ("j", "dʒeɪ"), ("k", "keɪ"), ("l", "ɛl"),
    ("m", "ɛm"), ("n", "ɛn"), ("o", "oʊ"), ("p", "piː"), ("q", "kjuː"), ("r", "ɑːɹ"),
    ("s", "ɛs"), ("t", "tiː"), ("u", "juː"), ("v", "viː"), ("w", "dˈʌbəlj'uː"),
    ("x", "ɛks"), ("y", "waɪ"), ("z", "ziː"),
];

fn is_chinese(c: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&c)
}

fn is_alphabet(c: char) -> bool {
    c.is_ascii_alphabetic()
}

/// 中文标点 → 英文标点(Emilia map_punctuations)
fn map_punctuations(text: &str) -> String {
    text.replace('，', ",")
        .replace('。', ".")
        .replace('！', "!")
        .replace('？', "?")
        .replace('；', ";")
        .replace('：', ":")
        .replace('、', ",")
        .replace('‘', "'")
        .replace('“', "\"")
        .replace('”', "\"")
        .replace('’', "'")
        .replace('⋯', "…")
        .replace("···", "…")
        .replace("・・・", "…")
        .replace("...", "…")
}

/// `<pinyin>` / `[tag]` 部件: 与 Python `[<[].*?[>\]]` 一致
fn take_special(chars: &[char], start: usize) -> Option<(String, usize)> {
    let open = chars[start];
    if open != '<' && open != '[' {
        return None;
    }
    let close = if open == '<' { '>' } else { ']' };
    let mut i = start;
    while i < chars.len() {
        if chars[i] == close {
            return Some((chars[start..=i].iter().collect(), i + 1 - start));
        }
        i += 1;
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SegKind {
    Zh,
    En,
    Pinyin,
    Tag,
    Other,
}

#[derive(Debug)]
pub struct Segment {
    pub text: String,
    pub kind: SegKind,
}

/// 与 Python get_segment 一致的分段逻辑
pub fn get_segment(text: &str) -> Vec<Segment> {
    // 逐"部件"扫描: 特殊串(<..>/[..])或单字符
    let chars: Vec<char> = text.chars().collect();
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if let Some((s, adv)) = take_special(&chars, i) {
            parts.push(s);
            i += adv;
        } else {
            parts.push(chars[i].to_string());
            i += 1;
        }
    }

    // 分类
    let kinds: Vec<SegKind> = parts
        .iter()
        .map(|p| {
            let c = p.chars().next().unwrap();
            if is_chinese(c) || is_pinyin_part(p) {
                SegKind::Zh
            } else if is_alphabet(c) {
                SegKind::En
            } else {
                SegKind::Other
            }
        })
        .collect();

    // 合并: "other" 附加到当前段并在遇到语言字符时改类型
    let mut segs: Vec<(String, SegKind)> = Vec::new();
    let mut temp = String::new();
    let mut temp_lang: Option<SegKind> = None;
    for (p, k) in parts.into_iter().zip(kinds) {
        match temp_lang {
            None => {
                temp = p;
                temp_lang = Some(k);
            }
            Some(SegKind::Other) => {
                temp.push_str(&p);
                temp_lang = Some(k);
            }
            Some(cur) => {
                if k == cur || k == SegKind::Other {
                    temp.push_str(&p);
                } else {
                    segs.push((std::mem::take(&mut temp), cur));
                    temp = p;
                    temp_lang = Some(k);
                }
            }
        }
    }
    if !temp.is_empty() || temp_lang.is_some() {
        segs.push((temp, temp_lang.unwrap_or(SegKind::Other)));
    }

    // 拆分特殊部件(<pinyin>/[tag])
    let mut out = Vec::new();
    for (seg, lang) in segs {
        let schars: Vec<char> = seg.chars().collect();
        let mut j = 0;
        let mut buf = String::new();
        while j < schars.len() {
            if let Some((s, adv)) = take_special(&schars, j) {
                if !buf.is_empty() {
                    out.push(Segment { text: std::mem::take(&mut buf), kind: lang.clone() });
                }
                if is_pinyin_part(&s) {
                    out.push(Segment { text: s, kind: SegKind::Pinyin });
                } else if s.starts_with('[') && s.ends_with(']') {
                    out.push(Segment { text: s, kind: SegKind::Tag });
                } else {
                    out.push(Segment { text: s, kind: lang.clone() });
                }
                j += adv;
            } else {
                buf.push(schars[j]);
                j += 1;
            }
        }
        if !buf.is_empty() {
            out.push(Segment { text: buf, kind: lang });
        }
    }
    out
}

/// 整个部件形如 <有效tone3拼音>
fn is_pinyin_part(p: &str) -> bool {
    let Some(inner) = p.strip_prefix('<').and_then(|s| s.strip_suffix('>')) else {
        return false;
    };
    let chars: Vec<char> = inner.chars().collect();
    chars.len() >= 2
        && chars[..chars.len() - 1].iter().all(|c| c.is_alphabetic())
        && matches!(chars[chars.len() - 1], '1' | '2' | '3' | '4' | '5')
}

fn is_valid_tone3(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    chars.len() >= 2
        && chars[..chars.len() - 1].iter().all(|c| c.is_alphabetic())
        && matches!(chars[chars.len() - 1], '1' | '2' | '3' | '4' | '5')
}

/// 拼音音节 → (声母0, 韵母带调) token, 对齐 Python seperate_pinyin
fn split_pinyin(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let tone = chars[chars.len() - 1];
    let base: String = chars[..chars.len() - 1].iter().collect();
    // 声母表(与 pypinyin to_initials strict=False 一致的贪心匹配)
    const INITIALS: &[&str] = &["zh", "ch", "sh", "b", "p", "m", "f", "d", "t", "n", "l", "g", "k", "h", "j", "q", "x", "r", "z", "c", "s", "y", "w"];
    let mut initial: Option<String> = None;
    let mut final_part = base.as_str();
    for ini in INITIALS {
        if base.starts_with(ini) {
            // y/w 是韵母头(pypinyin: yi→i, wu→u, yang→iang...), 不作为声母
            if *ini == "y" || *ini == "w" {
                break;
            }
            initial = Some(format!("{ini}0"));
            final_part = &base[ini.len()..];
            break;
        }
    }
    let mut out = Vec::new();
    if let Some(i) = initial {
        out.push(i);
    }
    out.push(format!("{final_part}{tone}"));
    out
}

/// 调试辅助: 标点映射后文本
pub fn debug_map_punct(t: &str) -> String {
    map_punctuations(t)
}

/// 调试辅助: 英文段 phones
pub fn debug_en_phones(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for seg in get_segment(text) {
        if matches!(seg.kind, SegKind::En) {
            out.extend(en_phones_for_debug(&seg.text));
        }
    }
    out
}

fn en_phones_for_debug(text: &str) -> Vec<String> {
    let normalized = crate::en_normalizer::normalize(text);
    let ipa = espeak_ipa(&normalized).unwrap_or_default();
    ipa.split_whitespace()
        .flat_map(|w| w.chars().map(|c| c.to_string()).collect::<Vec<_>>())
        .collect()
}

impl Tokenizer {
    pub fn load(token_file: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(token_file)
            .with_context(|| format!("读取 tokens.txt 失败: {}", token_file.display()))?;
        let mut token2id = HashMap::with_capacity(512);
        for line in text.lines() {
            let Some((tok, id)) = line.split_once('\t') else {
                continue;
            };
            token2id.insert(tok.to_string(), id.parse::<i64>().unwrap_or(0));
        }
        if !token2id.contains_key("_") {
            bail!("tokens.txt 缺少 padding token '_'");
        }
        let pinyin = PinyinTables::load()?;
        let espeak_available = which_espeak();
        Ok(Self { token2id, pinyin, jieba: Jieba::new(), espeak_available })
    }

    pub fn pad_id(&self) -> i64 {
        self.token2id["_"]
    }

    pub fn vocab_size(&self) -> usize {
        self.token2id.len()
    }

    pub fn phones_to_ids(&self, phones: &[String]) -> Vec<i64> {
        phones.iter().filter_map(|t| self.token2id.get(t).copied()).collect()
    }

    pub fn text_to_phones(&self, text: &str) -> Vec<String> {
        let text = map_punctuations(text);
        let mut out = Vec::new();
        for seg in get_segment(&text) {
            match seg.kind {
                SegKind::Zh => out.extend(self.tokenize_zh(&seg.text)),
                SegKind::En => out.extend(self.tokenize_en(&seg.text)),
                SegKind::Pinyin => {
                    let inner = seg.text.trim_start_matches('<').trim_end_matches('>');
                    if is_valid_tone3(inner) {
                        out.extend(split_pinyin(inner));
                    } else {
                        log::warn!("<> 内不是合法拼音, 跳过: {seg:?}");
                    }
                }
                SegKind::Tag => out.push(seg.text),
                SegKind::Other => {
                    log::warn!("跳过未知语言分段: {:?}", seg.text);
                }
            }
        }
        out
    }

    pub fn text_to_ids(&self, text: &str) -> Vec<i64> {
        self.phones_to_ids(&self.text_to_phones(text))
    }

    /// 中文: 归一化 → jieba 分词 → 词组表 / 逐字+变调 → 拆声母韵母
    fn tokenize_zh(&self, text: &str) -> Vec<String> {
        let normalized = normalizer::transform(text);
        let mut out = Vec::new();
        for word in self.jieba.cut(&normalized, true) {
            if word.is_empty() {
                continue;
            }
            // 词组表直接命中(pypinyin 短语读音, 不叠加变调)
            if word.chars().all(is_chinese) {
                if let Some(toks) = self.pinyin.phrase(word) {
                    out.extend(toks.iter().cloned());
                    continue;
                }
            }
            // 逐字 + 变调; 无法读音的字符原样透传(标点等)
            match self.pinyin.per_char_sandhi(word) {
                Some(toks) => out.extend(toks),
                None => out.extend(word.chars().map(|c| c.to_string())),
            }
        }
        out
    }

    /// 英文: 数字归一化 → espeak-ng IPA → 逐字符 token + 标点/空格回填
    fn tokenize_en(&self, text: &str) -> Vec<String> {
        let normalized = crate::en_normalizer::normalize(text);
        let ipa = if self.espeak_available {
            match espeak_ipa(&normalized) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("espeak-ng 失败, 回退字母拼写: {e}");
                    letter_fallback(&normalized)
                }
            }
        } else {
            letter_fallback(&normalized)
        };

        // espeak 丢弃了标点; 源文本按 词/标点/空格 切分后交错回填
        // (数字已在归一化阶段展开为单词, 因此 espeak 词组与源词一一对应)
        let ipa_words: Vec<Vec<String>> = ipa
            .split_whitespace()
            .map(|w| w.chars().map(|c| c.to_string()).collect())
            .collect();

        enum SrcTok {
            Word(String),
            Space,
            Punct(String),
        }
        let mut src: Vec<SrcTok> = Vec::new();
        {
            let mut cur = String::new();
            for c in normalized.chars() {
                if c.is_alphanumeric() || c == '\'' || c == '-' {
                    cur.push(c);
                } else {
                    if !cur.is_empty() {
                        src.push(SrcTok::Word(std::mem::take(&mut cur)));
                    }
                    if c == ' ' {
                        src.push(SrcTok::Space);
                    } else {
                        src.push(SrcTok::Punct(c.to_string()));
                    }
                }
            }
            if !cur.is_empty() {
                src.push(SrcTok::Word(cur));
            }
            // piper 语义: 标点前的空格不存在(espeak 吞掉) → 丢弃后随标点的空格
            let mut i = 0;
            while i < src.len() {
                if matches!(src[i], SrcTok::Space)
                    && matches!(src.get(i + 1), Some(SrcTok::Punct(_)))
                {
                    src.remove(i);
                } else {
                    i += 1;
                }
            }
        }

        let mut out = Vec::new();
        let mut widx = 0;
        // piper/espeak 语义: 空白折叠; 句末终止符(.!?…)后的空格被吞掉
        for tok in src {
            match tok {
                SrcTok::Word(w) => match ipa_words.get(widx) {
                    Some(group) => {
                        out.extend(group.iter().cloned());
                        widx += 1;
                    }
                    None => {
                        // espeak 词组不足(分词/展开差异), 字母兜底
                        out.extend(w.chars().filter_map(|c| {
                            let l = c.to_ascii_lowercase();
                            LETTER_IPA
                                .iter()
                                .find(|(k, _)| k.chars().next() == Some(l))
                                .map(|(_, v)| v.to_string())
                        }));
                    }
                },
                SrcTok::Space => {
                    let drop = out.is_empty()
                        || out.last().is_some_and(|t| t == " ")
                        || out.last().is_some_and(|t| {
                            t == "." || t == "!" || t == "?" || t == "…"
                        });
                    if !drop {
                        out.push(" ".to_string());
                    }
                }
                SrcTok::Punct(p) => out.push(p),
            }
        }
        // espeak 有余词(展开差异)时追加, 避免丢音
        while widx < ipa_words.len() {
            out.extend(ipa_words[widx].iter().cloned());
            widx += 1;
        }
        out
    }
}

fn which_espeak() -> bool {
    Command::new("espeak-ng")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// espeak-ng → IPA 字符串(空白分隔词组, 内部无分隔)
fn espeak_ipa(text: &str) -> anyhow::Result<String> {
    let out = Command::new("espeak-ng")
        .args(["-q", "-v", "en-us", "--ipa", text])
        .output()
        .context("运行 espeak-ng 失败")?;
    if !out.status.success() {
        bail!("espeak-ng 退出码 {:?}", out.status.code());
    }
    let s = String::from_utf8_lossy(&out.stdout);
    // espeak 偶尔输出语言标记 (en), 去掉
    let mut clean = String::with_capacity(s.len());
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => clean.push(c),
            _ => {}
        }
    }
    Ok(clean)
}

fn letter_fallback(text: &str) -> String {
    let mut out = Vec::new();
    for c in text.chars() {
        if c.is_ascii_alphabetic() {
            let l = c.to_ascii_lowercase();
            if let Some((_, ipa)) = LETTER_IPA.iter().find(|(k, _)| k.chars().next() == Some(l)) {
                out.push(ipa.to_string());
            }
        } else if c == ' ' {
            out.push(" ".to_string());
        }
    }
    out.join(" ")
}
