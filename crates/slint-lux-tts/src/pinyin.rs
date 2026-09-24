//! 拼音表加载 + 声调变调(tone sandhi) + 音节→音素 token。
//!
//! 数据文件由 scripts/gen_fixtures.py 从 pypinyin 生成:
//! - char_phones.txt:   单字 → "声母0 韵母" token 序列(pypinyin TONE3 + neutral=5, 无变调)
//! - phrase_pinyin.txt: 词组 → token 序列(存 pypinyin 短语词典最终读音, 含多音字消歧;
//!                      只存与"逐字+变调"结果不同的条目, 避免词组命中后再叠加变调)
//!
//! 变调规则 1:1 移植自 pypinyin/contrib/tone_sandhi.py(含其"只统计末段连续三声、
//! 从列表头部开始改"的实现特性), 作用于未命中词组表的逐字拼读。

use std::collections::HashMap;

const CHAR_TABLE: &str = include_str!("../data/char_phones.txt");
const PHRASE_TABLE: &str = include_str!("../data/phrase_pinyin.txt");

/// 单字条目: 声母 token(含"0"后缀, 可空) + 韵母 token(末位是声调数字)
#[derive(Clone, Debug)]
pub struct CharEntry {
    pub initial: Option<String>,
    pub final_tok: String,
}

impl CharEntry {
    /// 输出 token 序列(声调变更后调用)
    pub fn tokens(&self) -> Vec<String> {
        match &self.initial {
            Some(i) => vec![i.clone(), self.final_tok.clone()],
            None => vec![self.final_tok.clone()],
        }
    }

    fn syllable(&self) -> String {
        let mut s = String::new();
        if let Some(i) = &self.initial {
            // 去掉声母的 "0" 后缀
            s.push_str(i.trim_end_matches('0'));
        }
        s.push_str(&self.final_tok);
        s
    }

    fn set_tone(&mut self, tone: char) {
        // 声调数字是末位 ASCII 字符, 等长替换
        let n = self.final_tok.len();
        if n > 0 {
            self.final_tok.replace_range(n - 1..n, &tone.to_string());
        }
    }

    fn tone(&self) -> Option<char> {
        self.final_tok.chars().last().filter(|c| c.is_ascii_digit())
    }
}

fn contains_digit3(s: &str) -> bool {
    s.bytes().any(|b| b == b'3')
}

/// 三声连读: pypinyin _third_tone 原样移植
fn third_tone(entries: &mut [CharEntry]) {
    let syls: Vec<String> = entries.iter().map(|e| e.syllable()).collect();
    if !syls.iter().any(|s| s.contains('3')) {
        return;
    }
    // 连续三声运行长度(只保留最后一次计数, 与 pypinyin 一致)
    let mut third_num = 0usize;
    for s in &syls {
        if contains_digit3(s) {
            third_num += 1;
        } else {
            third_num = 0;
        }
    }
    if third_num == 2 {
        for e in entries.iter_mut() {
            if contains_digit3(&e.syllable()) {
                e.set_tone('2');
                break;
            }
        }
    } else if third_num > 2 {
        let mut n = 1usize;
        for e in entries.iter_mut() {
            if contains_digit3(&e.syllable()) {
                if n == third_num {
                    break;
                }
                e.set_tone('2');
                n += 1;
            }
        }
    }
}

/// "不"/"一" 变调: pypinyin _bu/_yi 原样移植
fn bu_yi(han: &[char], entries: &mut [CharEntry], target: char, last_tone: char) {
    if !han.contains(&target) {
        return;
    }
    for (i, h) in han.iter().enumerate() {
        if i >= entries.len() {
            break;
        }
        if *h != target {
            continue;
        }
        let next_has_4 = i + 1 < entries.len() && entries[i + 1].tone() == Some('4');
        if i < han.len() - 1 && next_has_4 {
            // 4 → 2: bù→bú / yī→yí
            entries[i].set_tone('2');
        } else if i < han.len() - 1 {
            // 后接非四声 → 一律变: bù / yī→yì
            entries[i].set_tone(last_tone);
        } else {
            // 词尾: 不→bù(4) / 一→yī(1)
            entries[i].set_tone(last_tone);
        }
    }
}

/// 对一段逐字拼读应用完整变调链(三声 → 不 → 一)
pub fn apply_sandhi(han: &[char], entries: &mut [CharEntry]) {
    third_tone(entries);
    bu_yi(han, entries, '不', '4');
    bu_yi(han, entries, '一', '1');
}

pub struct PinyinTables {
    chars: HashMap<char, CharEntry>,
    phrases: HashMap<String, Vec<String>>,
}

impl PinyinTables {
    pub fn load() -> anyhow::Result<Self> {
        let mut chars = HashMap::with_capacity(30000);
        for line in CHAR_TABLE.lines() {
            let Some((ch, toks)) = line.split_once('\t') else {
                continue;
            };
            let mut it = toks.split(' ');
            let entry = match (it.next(), it.next()) {
                (Some(a), Some(b)) => CharEntry {
                    initial: Some(a.to_string()),
                    final_tok: b.to_string(),
                },
                (Some(a), None) => CharEntry { initial: None, final_tok: a.to_string() },
                _ => continue,
            };
            if let Some(c) = ch.chars().next() {
                chars.insert(c, entry);
            }
        }
        let mut phrases = HashMap::with_capacity(30000);
        for line in PHRASE_TABLE.lines() {
            let Some((w, toks)) = line.split_once('\t') else {
                continue;
            };
            phrases.insert(w.to_string(), toks.split(' ').map(str::to_string).collect());
        }
        Ok(Self { chars, phrases })
    }

    /// 词组表命中(直接使用 pypinyin 的最终读音, 不再叠加变调)
    pub fn phrase(&self, w: &str) -> Option<&Vec<String>> {
        self.phrases.get(w)
    }

    /// 逐字拼读 + 变调。返回 None 表示存在无法读音的字符。
    pub fn per_char_sandhi(&self, seg: &str) -> Option<Vec<String>> {
        let han: Vec<char> = seg.chars().collect();
        let mut entries: Vec<CharEntry> = Vec::with_capacity(han.len());
        // 对齐数组: Some(entry)=可变调的音节, None=原样透传的 token(标点/生僻字)
        let mut passthrough: Vec<Option<String>> = Vec::with_capacity(han.len());
        for c in &han {
            match self.chars.get(c) {
                Some(e) => {
                    entries.push(e.clone());
                    passthrough.push(None);
                }
                None => {
                    // 生僻字符: 原 token 透传(与 pypinyin errors='default' 一致), 不参与变调
                    // 变调算法要求 han 与音节一一对应, 这里用空 entry 占位并标记透传
                    entries.push(CharEntry { initial: None, final_tok: String::new() });
                    passthrough.push(Some(c.to_string()));
                }
            }
        }
        apply_sandhi(&han, &mut entries);
        let mut out = Vec::with_capacity(han.len() * 2);
        for (i, pt) in passthrough.iter().enumerate() {
            match pt {
                Some(t) => out.push(t.clone()),
                None => out.extend(entries[i].tokens()),
            }
        }
        Some(out)
    }
}
