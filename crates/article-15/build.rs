// article-15 build.rs
//
// 两件事:
// 1) 用 msgfmt 把 lang/<locale>/LC_MESSAGES/*.po 编译成 .mo
//    - article-15.mo    : Slint 侧 @tr 的翻译 (被 with_bundled_translations bundle 进二进制)
//    - article-15-rs.mo : Rust 侧 gettext 的翻译 (被 src/i18n.rs 用 include_bytes! 嵌入)
// 2) 用 slint-build 编译 UI, 并 .with_bundled_translations("lang") 把 .mo 一并打进二进制
//
// .po 的来源(官方工作流, 见 Makefile 的 i18n 目标):
//   slint-tr-extractor --package-name article-15 -o lang/article-15.pot ui/app.slint  # 提取 @tr
//   msgmerge --update lang/<locale>/LC_MESSAGES/article-15.po lang/article-15.pot      # 更新翻译
//   (Rust 侧 article-15-rs.* 不在 @tr 里, 由官方工具管不到, 手工维护)
//
// 注意: 构建机需要 gettext(msgfmt)。没有 msgfmt 且 .mo 已经预生成时, 跳过编译直接复用。

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=ui/app.slint");
    println!("cargo:rerun-if-changed=lang");

    build_translations();
    build_slint();
}

fn build_translations() {
    // 每个语言/domain 一个 .mo
    let locales = ["zh_CN", "en"];
    let domains = ["article-15", "article-15-rs"];
    for locale in locales {
        for domain in domains {
            let po = format!("lang/{locale}/LC_MESSAGES/{domain}.po");
            let mo = format!("lang/{locale}/LC_MESSAGES/{domain}.mo");
            if std::path::Path::new(&po).exists() {
                println!("cargo:rerun-if-changed={po}");
                match std::process::Command::new("msgfmt")
                    .args([po.as_str(), "-o", mo.as_str()])
                    .output()
                {
                    Ok(out) if out.status.success() => {}
                    Ok(out) => panic!(
                        "msgfmt 编译 {po} 失败: {}",
                        String::from_utf8_lossy(&out.stderr)
                    ),
                    Err(_) => {
                        // 没有 msgfmt 时, 若 .mo 已预生成则复用, 否则报错
                        if !std::path::Path::new(&mo).exists() {
                            panic!("未找到 msgfmt, 请先安装 gettext: sudo apt install gettext 或 brew install gettext");
                        }
                    }
                }
            }
        }
    }
}

fn build_slint() {
    let config = slint_build::CompilerConfiguration::new()
        // 把 lang/ 下的 .mo 作为 bundled translation 打进二进制, 运行时按 locale 热切换
        .with_bundled_translations("lang");
    slint_build::compile_with_config("ui/app.slint", config).expect("slint compile");
}
