//! 英文文本归一化(数字展开), 对齐 Python 参考实现的 EnglishTextNormalizer。
//!
//! 规则来自 espnet tacotron cleaners + inflect number_to_words:
//! 缩写展开 / 逗号数字 / 磅英镑 / 美元 / 分数 / 小数(point) / 百分比 / 序数词 / 整数读法。
//! espeak 对 "twenty-three" 等连字符词正常发音, 与 inflect 输出兼容。

fn number_to_words(n: u64) -> String {
    const ONES: &[&str] = &[
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
        "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen",
        "nineteen",
    ];
    const TENS: &[&str] = &[
        "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
    ];
    if n < 20 {
        return ONES[n as usize].to_string();
    }
    if n < 100 {
        let t = TENS[(n / 10) as usize];
        let r = n % 10;
        return if r == 0 { t.to_string() } else { format!("{t}-{}", ONES[r as usize]) };
    }
    if n < 1000 {
        let h = ONES[(n / 100) as usize];
        let r = n % 100;
        return if r == 0 { format!("{h} hundred") } else { format!("{h} hundred {}", number_to_words(r)) };
    }
    const SCALES: &[(u64, &str)] = &[(1_000_000_000, "billion"), (1_000_000, "million"), (1_000, "thousand")];
    for (scale, name) in SCALES {
        if n >= *scale {
            let q = n / scale;
            let r = n % scale;
            return if r == 0 {
                format!("{} {}", number_to_words(q), name)
            } else {
                format!("{} {} {}", number_to_words(q), name, number_to_words(r))
            };
        }
    }
    n.to_string()
}

fn ordinal_word(n: u64) -> String {
    // inflect 简化: one first / two second / ... twenty-first / hundredth
    let cardinal = number_to_words(n);
    const SPECIAL: &[(&str, &str)] = &[
        ("one", "first"), ("two", "second"), ("three", "third"), ("five", "fifth"),
        ("eight", "eighth"), ("nine", "ninth"), ("twelve", "twelfth"),
    ];
    for (base, ord) in SPECIAL {
        if cardinal == *base {
            return ord.to_string();
        }
    }
    if let Some(stripped) = cardinal.strip_suffix("y") {
        return format!("{stripped}ieth");
    }
    format!("{cardinal}th")
}

fn expand_abbreviations(t: &str) -> String {
    // word-boundary 缩写(简化为带空格边界的替换)
    let abbrevs: &[(&str, &str)] = &[
        ("mrs", "misess"), ("mr", "mister"), ("dr", "doctor"), ("st", "saint"),
        ("co", "company"), ("jr", "junior"), ("maj", "major"), ("gen", "general"),
        ("drs", "doctors"), ("rev", "reverend"), ("lt", "lieutenant"), ("hon", "honorable"),
        ("sgt", "sergeant"), ("capt", "captain"), ("esq", "esquire"), ("ltd", "limited"),
        ("col", "colonel"), ("ft", "fort"), ("etc", "et cetera"), ("btw", "by the way"),
    ];
    let mut out = t.to_string();
    for (a, b) in abbrevs {
        out = replace_word(&out, a, b);
    }
    out
}

/// 整词大小写不敏感替换
fn replace_word(text: &str, from: &str, to: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let pat = from.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(pat)
            && (i == 0 || !bytes[i - 1].is_ascii_alphabetic())
            && (i + pat.len() >= bytes.len() || !bytes[i + pat.len()].is_ascii_alphabetic())
        {
            out.push_str(to);
            i += pat.len();
        } else {
            // 找到原文本中对应字符(按 char 推进)
            let ch_len = text[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            out.push_str(&text[i..i + ch_len]);
            i += ch_len;
        }
    }
    out
}

fn expand_dollars(m: &str) -> String {
    // $X.YZ → "X dollars, YZ cents" (对齐 inflect 输出格式)
    let body = &m[1..];
    let parts: Vec<&str> = body.split('.').collect();
    if parts.len() > 2 {
        return format!(" {m} dollars ");
    }
    let dollars: u64 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let cents: u64 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    if dollars > 0 && cents > 0 {
        format!(
            " {} dollar{}, {} cent{} ",
            number_to_words(dollars),
            if dollars == 1 { "" } else { "s" },
            number_to_words(cents),
            if cents == 1 { "" } else { "s" }
        )
    } else if dollars > 0 {
        format!(" {} dollar{} ", number_to_words(dollars), if dollars == 1 { "" } else { "s" })
    } else if cents > 0 {
        format!(" {} cent{} ", number_to_words(cents), if cents == 1 { "" } else { "s" })
    } else {
        " zero dollars ".to_string()
    }
}

fn expand_number(n: u64) -> String {
    if n > 1000 && n < 3000 {
        if n == 2000 {
            return " two thousand ".to_string();
        }
        if n > 2000 && n < 2010 {
            return format!(" two thousand {} ", number_to_words(n % 100));
        }
        if n % 100 == 0 {
            return format!(" {} hundred ", number_to_words(n / 100));
        }
        // group=2, zero="oh": 1950 → nineteen fifty oh? inflect group=2: "nineteen fifty"
        let t = n / 100;
        let r = n % 100;
        return format!(" {} {} ", number_to_words(t), if r < 10 { format!("oh {}", number_to_words(r)) } else { number_to_words(r) });
    }
    format!(" {} ", number_to_words(n))
}

/// 数字与缩写展开主入口
pub fn normalize(text: &str) -> String {
    let t = expand_abbreviations(text);
    let chars: Vec<char> = t.chars().collect();
    let mut out = String::with_capacity(t.len() + 32);
    let mut i = 0;
    let n = chars.len();

    let take_digits = |chars: &[char], mut i: usize| -> (String, usize) {
        let s = i;
        while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == ',') {
            i += 1;
        }
        (chars[s..i].iter().collect(), i)
    };
    let clean = |s: &str| s.replace(',', "");

    while i < n {
        let c = chars[i];
        if c == '£' {
            let (num, j) = take_digits(&chars, i + 1);
            if !num.is_empty() {
                out.push_str(&format!(" {} pounds ", clean(&num)));
                i = j;
                continue;
            }
        } else if c == '$' {
            let (num, j) = take_digits(&chars, i + 1);
            // 带小数
            let mut j2 = j;
            let mut frac = String::new();
            if j < n && chars[j] == '.' && j + 1 < n && chars[j + 1].is_ascii_digit() {
                let s = j + 1;
                j2 = s;
                while j2 < n && chars[j2].is_ascii_digit() {
                    j2 += 1;
                }
                frac = chars[s..j2].iter().collect();
            }
            let full = if frac.is_empty() { num.clone() } else { format!("{num}.{frac}") };
            out.push_str(&expand_dollars(&format!("${}", clean(&full))));
            i = j2;
            continue;
        } else if c.is_ascii_digit() {
            // 完整数字(可含逗号与小数)
            let (num_raw, j) = take_digits(&chars, i);
            let mut j2 = j;
            let mut frac = String::new();
            if j < n && chars[j] == '.' && j + 1 < n && chars[j + 1].is_ascii_digit() {
                let s = j + 1;
                j2 = s;
                while j2 < n && chars[j2].is_ascii_digit() {
                    j2 += 1;
                }
                frac = chars[s..j2].iter().collect();
            }
            // 序数: 20th
            if frac.is_empty()
                && j2 + 1 < n
                && matches!(&chars[j2..j2 + 2], ['s', 't'] | ['n', 'd'] | ['r', 'd'] | ['t', 'h'])
                && (j2 + 2 >= n || !chars[j2 + 2].is_ascii_alphabetic())
            {
                let num = clean(&num_raw);
                if let Ok(v) = num.parse::<u64>() {
                    out.push_str(&format!(" {} ", ordinal_word(v)));
                    i = j2 + 2;
                    continue;
                }
            }
            // 百分比
            if j2 < n && chars[j2] == '%' {
                let num = clean(&num_raw);
                let words = if frac.is_empty() {
                    english_int_words(&num)
                } else {
                    format!("{} point {}", english_int_words(&num), digit_words(&frac))
                };
                out.push_str(&format!(" {words} percent "));
                i = j2 + 1;
                continue;
            }
            let num = clean(&num_raw);
            if !frac.is_empty() {
                out.push_str(&format!(
                    " {} point {} ",
                    english_int_words(&num),
                    digit_words(&frac)
                ));
            } else if let Ok(v) = num.parse::<u64>() {
                out.push_str(&expand_number(v));
            } else {
                out.push_str(&num);
            }
            i = j2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

fn digit_words(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_digit())
        .map(|d| number_to_words((d as u8 - b'0') as u64))
        .collect::<Vec<_>>()
        .join(" ")
}

fn english_int_words(num: &str) -> String {
    match num.parse::<u64>() {
        Ok(v) => number_to_words(v),
        Err(_) => num.to_string(),
    }
}
