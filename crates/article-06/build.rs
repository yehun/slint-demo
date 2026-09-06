// 第六幕: 基于Slint官方Material建立Android主题体系
// 关键: 通过 with_library_paths 注册 @material 命名空间
//   @material → 自定义入口 (使用自定义 MaterialPalette, 内置 7 套主题)
// 标准组件通过 ui/ 下的符号链接复用, 只有 material_palette.slint 是自定义的

use std::collections::HashMap;
use std::env;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/themes/material/material.slint");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let manifest_path = Path::new(&manifest_dir);

    // 自定义 @material 入口 (包含自定义 MaterialPalette)
    let material_path = manifest_path.join("../../assets/themes/material/material.slint");

    let library_paths = HashMap::from([
        ("material".to_string(), material_path),
    ]);

    let config = slint_build::CompilerConfiguration::new()
        .with_library_paths(library_paths)
        .with_style("material".into());

    slint_build::compile_with_config("ui/app.slint", config)
        .expect("Failed to compile Slint UI with Material theme");
}
