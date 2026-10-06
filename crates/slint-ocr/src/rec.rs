//! 文本识别：动态宽度 rec 推理 + CTC 解码 + 字典映射

use anyhow::Result;

/// 解析字典文件（ppocr_keys_v1 风格：每行一个字符，首行为空格）
pub fn load_keys(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .split('\n')
        .map(|s| s.trim_end_matches('\r').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// CTC 解码：logits 为 [T, C]（softmax 后概率，行主序）
/// 规则：argmax → 合并相邻相同 → 跳过 blank(id=0) → keys[id-1] 映射
pub fn decode_ctc(logits: &[f32], t: usize, c: usize, keys: &[String]) -> (String, f32) {
    let mut ids: Vec<u32> = Vec::with_capacity(t);
    let mut scores: Vec<f32> = Vec::with_capacity(t);
    for i in 0..t {
        let row = &logits[i * c..(i + 1) * c];
        let mut max_v = f32::MIN;
        let mut max_i = 0usize;
        for (j, v) in row.iter().enumerate() {
            if *v > max_v {
                max_v = *v;
                max_i = j;
            }
        }
        ids.push(max_i as u32);
        scores.push(max_v);
    }

    let mut text = String::new();
    let mut conf_sum = 0f32;
    let mut conf_n = 0usize;
    let mut prev: i32 = -1;
    for (i, id) in ids.iter().enumerate() {
        if *id == 0 {
            // blank：分隔相邻相同字符
            prev = -1;
            continue;
        }
        if prev == *id as i32 {
            continue;
        }
        prev = *id as i32;
        if let Some(ch) = keys.get((*id as usize).saturating_sub(1)) {
            text.push_str(ch);
            conf_sum += scores[i];
            conf_n += 1;
        }
    }
    let confidence = if conf_n > 0 { conf_sum / conf_n as f32 } else { 0.0 };
    (text, confidence)
}

/// 简单的 beam 宽度 1 解码入口（等价 decode_ctc），供外部统一调用
pub fn decode(logits: &[f32], t: usize, c: usize, keys: &[String]) -> Result<(String, f32)> {
    Ok(decode_ctc(logits, t, c, keys))
}
