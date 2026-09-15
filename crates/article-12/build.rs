// 第十二幕: Slint做自己的电子表格
// Material 主题: 注册 @material 命名空间 + 设置 material 样式

use std::collections::HashMap;
use std::env;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/themes/material/material.slint");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let manifest_path = Path::new(&manifest_dir);

    // 注册 @material 命名空间 (与 article-06 共享同一套 Material 主题)
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
