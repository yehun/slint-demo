mod utils;
mod scheme;
mod theme;

use slint::ComponentHandle;
use crate::{
    MainWindow,
    ThemeSwitcher,
    MaterialPalette
};

/// 初始化主题: 加载 JSON 主题, 设置主题名列表, 应用默认主题
pub fn init(main_window: &MainWindow) {
    let theme_map = theme::load_theme_map();

    // 按 sort 排序, 提取主题名
    let mut theme_sort: Vec<(String, u8)> = theme_map
        .iter()
        .map(|(k, v)| (k.to_string(), v.sort.unwrap_or(0)))
        .collect();
    theme_sort.sort_by(|(_, a), (_, b)| a.cmp(b));
    let theme_names: Vec<String> = theme_sort.into_iter().map(|(k, _)| k).collect();

    // 设置主题名到 ThemeSwitcher
    let slint_names: Vec<slint::SharedString> = theme_names.iter().map(|s| s.as_str().into()).collect();
    main_window.global::<ThemeSwitcher>().set_theme_names(slint_names.as_slice().into());

    // 应用默认主题 (Slint 蓝)
    if let Some(theme) = theme_map.get("Slint") {
        let schemes = theme.schemes.to_slint();
        main_window.global::<MaterialPalette>().set_schemes(schemes);
    }

    // 注册主题切换回调 (当用户选择主题时, 从 JSON 加载并应用)
    let theme_map = theme::load_theme_map();
    let ww = main_window.as_weak();
    main_window.global::<ThemeSwitcher>().on_applied(move |index| {
        if let Some(w) = ww.upgrade() {
            if let Some(name) = theme_names.get(index as usize) {
                if let Some(t) = theme_map.get(name) {
                    let schemes = t.schemes.to_slint();
                    w.global::<MaterialPalette>().set_schemes(schemes);
                }
            }
        }
    });
}