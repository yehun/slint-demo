//! 中文文本归一化(cn2an an2cn 最小移植): 数字→汉字读法。
//!
//! 覆盖参考实现 cn2an.transform(text, "an2cn") 在实际语料中出现的主要模式:
//! 百分比 / 分数 / 负数 / 小数 / 年份(数字+年 逐字读) / 数值(万/亿 分组)。
//! 其余字符原样保留。

/// 数字序列 → 中文值读法: "13"→十三, "110"→一百一十, "1000"→一千, "105"→一百零五,
/// "2026"→二千零二十六, "10013"→一万零一十三 (规则与 cn2an 实测对齐)。
fn int_to_cn(s: &str) -> String {
    let digits = ["零", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
    let units = ["", "十", "百", "千"];
    let big = ["", "万", "亿", "万亿"];

    let s = s.trim_start_matches('0');
    if s.is_empty() {
        return "零".to_string();
    }
    let ds: Vec<usize> = s.bytes().map(|b| (b - b'0') as usize).collect();

    // 从右按 4 位分组(万进制), 高位组在前; 只有首组可能不足 4 位
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut rem: &[usize] = &ds[..];
    while rem.len() > 4 {
        let split = (rem.len() - 1) % 4 + 1;
        groups.push(rem[..split].to_vec());
        rem = &rem[split..];
    }
    groups.push(rem.to_vec());
    let gcount = groups.len();

    let mut out = String::new();
    let mut any_emitted = false; // 整个数串是否已输出过有效数字
    let mut zero_gap = false; // 跨组全零洞
    for (gi, g) in groups.iter().enumerate() {
        if is_all_zero(g) {
            zero_gap = true;
            continue;
        }
        let big_unit = big[gcount - 1 - gi];
        let mut seg = String::new();
        let len = g.len();
        let mut zero_pending = false;
        let mut emitted = false;
        for (i, &d) in g.iter().enumerate() {
            let unit_pos = len - 1 - i; // 3=千 2=百 1=十 0=个
            if d == 0 {
                if emitted {
                    zero_pending = true;
                }
                continue;
            }
            if zero_pending {
                seg.push('零');
                zero_pending = false;
            }
            // 整串开头的 "一十" 省略 "一": 13→十三, 10→十; 其余位置保留: 110→一百一十
            if !(unit_pos == 1 && d == 1 && !any_emitted) {
                seg.push_str(digits[d]);
            }
            seg.push_str(units[unit_pos]);
            emitted = true;
            any_emitted = true;
        }
        // 组间补零: 本组千位为 0, 或中间隔了全零组
        if g[0] == 0 && emitted || zero_gap && emitted {
            out.push('零');
        }
        out.push_str(&seg);
        out.push_str(big_unit);
        zero_gap = false;
    }
    if out.is_empty() {
        return "零".to_string();
    }
    out
}

fn is_all_zero(g: &[usize]) -> bool {
    g.iter().all(|&d| d == 0)
}

/// 数字串逐字读: "2026" → 二零二六
fn digits_to_cn(s: &str) -> String {
    let digits = ["零", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
    let mut out = String::new();
    let s = s.trim_start_matches('0');
    let s = if s.is_empty() { "0" } else { s };
    for b in s.bytes() {
        out.push_str(digits[(b - b'0') as usize]);
    }
    out
}

/// 小数点后的数字逐个读: "5"→五, "60"→六零
fn decimal_digits_to_cn(s: &str) -> String {
    let digits = ["零", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_digit() {
            out.push_str(digits[(b - b'0') as usize]);
        }
    }
    out
}

/// cn2an.transform(text, "an2cn") 的最小移植
pub fn normalize(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() * 3);
    let mut i = 0;
    let n = chars.len();

    let is_digit = |c: char| c.is_ascii_digit();
    let take_digits = |chars: &[char], mut i: usize| -> (String, usize) {
        let start = i;
        while i < chars.len() && is_digit(chars[i]) {
            i += 1;
        }
        (chars[start..i].iter().collect(), i)
    };

    while i < n {
        let c = chars[i];
        if !is_digit(c) {
            out.push(c);
            i += 1;
            continue;
        }
        // 一段连续数字(含小数点)
        let (num, mut j) = take_digits(&chars, i);
        // 小数
        if j + 1 <= n && j < n && chars[j] == '.' && j + 1 < n && is_digit(chars[j + 1]) {
            let (frac, j2) = take_digits(&chars, j + 1);
            out.push_str(&int_to_cn(&num));
            out.push('点');
            out.push_str(&decimal_digits_to_cn(&frac));
            j = j2;
            i = j;
            continue;
        }
        // 年份: 数字 + 年 → 逐字读
        if j < n && chars[j] == '年' && num.len() <= 4 {
            out.push_str(&digits_to_cn(&num));
            i = j;
            continue;
        }
        // 万/亿 数量级后缀: 9000万 → 九千万
        if j < n && (chars[j] == '万' || chars[j] == '亿') {
            out.push_str(&int_to_cn(&num));
            // 后缀字符由下一轮原样输出
            i = j;
            continue;
        }
        out.push_str(&int_to_cn(&num));
        i = j;
    }
    out
}

/// 百分比/分数/负号处理(在 normalize 之前做模式替换, 对齐 cn2an smart 模式行为)
pub fn transform(text: &str) -> String {
    let t = text.to_string();
    // 先处理 % / 负号 / 分数, 再交给 normalize
    let chars: Vec<char> = t.chars().collect();
    let mut rebuilt = String::with_capacity(t.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '%' {
            // 找到数字段起点
            let mut s = i;
            let mut saw_dot = false;
            while s > 0 && (chars[s - 1].is_ascii_digit() || (chars[s - 1] == '.' && !saw_dot)) {
                if chars[s - 1] == '.' {
                    saw_dot = true;
                }
                s -= 1;
            }
            let num: String = chars[s..i].iter().collect();
            if num.chars().next().map(|x| x.is_ascii_digit()).unwrap_or(false) {
                // 回退 rebuilt 中已写入的数字字符
                let digit_char_count: usize = num.chars().count();
                let remove_bytes: usize = rebuilt
                    .chars()
                    .rev()
                    .take(digit_char_count)
                    .map(|x| x.len_utf8())
                    .sum();
                let cut = rebuilt.len() - remove_bytes;
                rebuilt.truncate(cut);
                rebuilt.push_str("百分之");
                let (int_part, frac_part) = match num.split_once('.') {
                    Some((a, b)) => (a, Some(b)),
                    None => (num.as_str(), None),
                };
                rebuilt.push_str(&int_to_cn(int_part));
                if let Some(f) = frac_part {
                    rebuilt.push('点');
                    rebuilt.push_str(&decimal_digits_to_cn(f));
                }
                i += 1;
                continue;
            }
            rebuilt.push(c);
            i += 1;
        } else if c == '-' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
            // 负数: -5 → 负五 (cn2an: '负')
            let (num, j) = take_digits_impl(&chars, i + 1);
            rebuilt.push_str("负");
            rebuilt.push_str(&int_to_cn(&num));
            i = j;
        } else if c == '/' {
            // 分数: a/b → b分之a (cn2an 风格)
            let mut s = i;
            while s > 0 && chars[s - 1].is_ascii_digit() {
                s -= 1;
            }
            let mut e = i + 1;
            while e < chars.len() && chars[e].is_ascii_digit() {
                e += 1;
            }
            let a: String = chars[s..i].iter().collect();
            let b: String = chars[i + 1..e].iter().collect();
            if !a.is_empty() && !b.is_empty() {
                // 回退 rebuilt 中已写入的 a
                let remove_bytes: usize = a.chars().map(|x| x.len_utf8()).sum();
                let cut = rebuilt.len() - remove_bytes;
                rebuilt.truncate(cut);
                rebuilt.push_str(&int_to_cn(&b));
                rebuilt.push_str("分之");
                rebuilt.push_str(&int_to_cn(&a));
                i = e;
                continue;
            }
            rebuilt.push(c);
            i += 1;
        } else {
            rebuilt.push(c);
            i += 1;
        }
    }
    normalize(&rebuilt)
}

fn take_digits_impl(chars: &[char], mut i: usize) -> (String, usize) {
    let start = i;
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    (chars[start..i].iter().collect(), i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cn2an_cases() {
        assert_eq!(transform("2026年9月13日"), "二零二六年九月十三日");
        assert_eq!(transform("30%"), "百分之三十");
        assert_eq!(transform("-5度"), "负五度");
        assert_eq!(transform("110报警"), "一百一十报警");
        assert_eq!(transform("第1名"), "第一名");
        assert_eq!(transform("3.5"), "三点五");
        assert_eq!(transform("1000人"), "一千人");
        assert_eq!(transform("12:30"), "十二:三十");
        assert_eq!(transform("9000万吨"), "九千万吨");
        assert_eq!(transform("性能提升了30%。"), "性能提升了百分之三十。");
        assert_eq!(int_to_cn("2026"), "二千零二十六");
        assert_eq!(int_to_cn("105"), "一百零五");
        assert_eq!(int_to_cn("10013"), "一万零一十三");
        assert_eq!(int_to_cn("100013"), "十万零一十三");
        assert_eq!(int_to_cn("2001005"), "二百万一千零五");
        assert_eq!(int_to_cn("100000001"), "一亿零一");
        assert_eq!(int_to_cn("1100000"), "一百一十万");
        assert_eq!(int_to_cn("10"), "十");
    }
}
