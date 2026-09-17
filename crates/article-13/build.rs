// 第十三篇, Slint开发的第一个音乐播放器
// 自定义深色主题 + lucide-slint 图标库

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=ui/app.slint");

    // 注册 lucide 命名空间
    let library = std::collections::HashMap::from([(
        "lucide".to_string(),
        std::path::PathBuf::from(lucide_slint::get_slint_file_path().to_string()),
    )]);

    let config = slint_build::CompilerConfiguration::new()
        .with_library_paths(library);

    slint_build::compile_with_config("ui/app.slint", config)
        .expect("Failed to compile Slint UI with lucide icons");
}
