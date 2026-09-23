// article-15 Rust 侧翻译
//
// 与 Slint 的 @tr 平行: Rust 代码里的用户可见字符串也要翻译。
// 做法: 解析 .mo catalog, 用 include_bytes! 把 .mo 直接嵌进二进制
// (Android APK 里没有构建机文件路径, 嵌入后全平台通用)。
//
// 注意: 常规做法是用 `gettext` crate (gettext::Catalog), 但它依赖 gettext-sys,
// 会在 Android 上从 C 源码编译 gettext 库 —— 需要 Android NDK 的 clang, 缺它就无法构建。
// 本 demo 改为内置一个极简的 GNU .mo 解析器, 效果等价, 且让 demo 在桌面/Android
// 都能干净编译。架构(.mo + include_bytes!)不变, 仅"解析器"换成几十行 Rust。
//
// 仅 zh_CN 有翻译, 其余语言 catalog 为 None → 回退 msgid(英文原文)。

use parking_lot::Mutex;
use std::collections::HashMap;

/// Rust 侧翻译统一使用的 context, 与 .slint 的 @tr(组件名 context) 区分开
const RUST_MSGCTXT: &str = "Rust";

/// 当前语言的 mo catalog (original bytes → translated bytes)
static CATALOG: Mutex<Option<HashMap<Vec<u8>, Vec<u8>>>> = Mutex::new(None);

/// 注册 Slint bundled 翻译目录 (必须在 select_bundled_translation 之前调用一次)。
/// 指向 lang/, 里面按 <locale>/LC_MESSAGES/<domain>.mo 组织。
pub fn init_translations() {
    slint::init_translations!(concat!(env!("CARGO_MANIFEST_DIR"), "/lang/"));
}

/// 按 locale 重新加载 Rust 侧 catalog, 与 Slint bundled 翻译保持同步
fn load_catalog(locale: &str) -> Option<HashMap<Vec<u8>, Vec<u8>>> {
    if locale != "zh_CN" {
        return None;
    }
    static ZH_MO: &[u8] = include_bytes!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/lang/zh_CN/LC_MESSAGES/article-15-rs.mo")
    );
    match parse_mo(ZH_MO) {
        Ok(map) => Some(map),
        Err(e) => {
            log::warn!("[i18n] parse article-15-rs.mo failed: {e}");
            None
        }
    }
}

/// Rust 侧翻译查询: 按当前语言 catalog 查, 无翻译时回退 msgid(英文原文)
pub fn gettext(msgid: &str) -> String {
    match CATALOG.lock().as_ref() {
        Some(map) => {
            let key = make_key(RUST_MSGCTXT, msgid);
            map.get(&key)
                .map(|v| String::from_utf8_lossy(v).into_owned())
                .unwrap_or_else(|| msgid.to_string())
        }
        None => msgid.to_string(),
    }
}

/// 同步切换语言:
/// 1) Rust catalog 先切 (即使 Slint 切换失败, Rust 也能回退英文)
/// 2) 再切 Slint bundled translation
///
/// 顺序很重要: Rust catalog 必须先于 bundled 切换 —— headless 环境无 UI 实例时
/// select_bundled_translation 会返回 Err, 若后置切换 Rust catalog 会被跳过。
pub fn select_translations(locale: &str) -> Result<(), slint::SelectBundledTranslationError> {
    *CATALOG.lock() = load_catalog(locale);
    slint::select_bundled_translation(locale)
}

// ---- 极简 GNU .mo 解析 (little-endian) ----
// .mo 结构: magic(0x950412de) + version + N(字符串数) + O(原文表偏移) + T(译文表偏移)
//           + hashsize + hashoffset; 两张表各 N 个 (len, offset) 8 字节记录。
// 带 context 的条目, 原文里写作 "ctx\x04msgid" (\x04 = EOT 分隔符)。
fn make_key(ctx: &str, msgid: &str) -> Vec<u8> {
    if ctx.is_empty() {
        msgid.as_bytes().to_vec()
    } else {
        let mut k = Vec::with_capacity(ctx.len() + 1 + msgid.len());
        k.extend_from_slice(ctx.as_bytes());
        k.push(0x04);
        k.extend_from_slice(msgid.as_bytes());
        k
    }
}

fn parse_mo(data: &[u8]) -> Result<HashMap<Vec<u8>, Vec<u8>>, String> {
    if data.len() < 20 {
        return Err("mo too short".into());
    }
    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if magic != 0x9504_12de {
        return Err(format!("bad mo magic {magic:#x} (need little-endian 0x950412de)"));
    }
    let num = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let orig_off = u32::from_le_bytes([data[12], data[13], data[14], data[15]]) as usize;
    let trans_off = u32::from_le_bytes([data[16], data[17], data[18], data[19]]) as usize;

    let mut map = HashMap::with_capacity(num);
    for i in 0..num {
        let o = read_pair(data, orig_off + i * 8)?;
        let t = read_pair(data, trans_off + i * 8)?;
        if o.1 == 0 {
            continue; // 跳过空 msgid (文件头)
        }
        let key = data[o.0..o.0 + o.1].to_vec();
        let val = data[t.0..t.0 + t.1].to_vec();
        map.insert(key, val);
    }
    Ok(map)
}

fn read_pair(data: &[u8], off: usize) -> Result<(usize, usize), String> {
    if off + 8 > data.len() {
        return Err("mo pair out of range".into());
    }
    let len = u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]]) as usize;
    let pos = u32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]]) as usize;
    if pos + len > data.len() {
        return Err("mo string out of range".into());
    }
    Ok((pos, len))
}
