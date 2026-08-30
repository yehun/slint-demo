fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=ui/app.slint");
    println!("cargo:rerun-if-changed=ui/assets/fonts/MiSans-Regular.ttf");

    // 嵌入中文字体, 解决 Android 中文乱码
    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles);

    slint_build::compile_with_config("ui/app.slint", config)
        .expect("Failed to compile slint");
}
