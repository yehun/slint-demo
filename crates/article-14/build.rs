fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=ui/app.slint");

    let library = std::collections::HashMap::from([(
        "lucide".to_string(),
        std::path::PathBuf::from(lucide_slint::get_slint_file_path().to_string()),
    )]);

    let config = slint_build::CompilerConfiguration::new()
        .with_library_paths(library);

    slint_build::compile_with_config("ui/app.slint", config)
        .expect("slint compile");
}
