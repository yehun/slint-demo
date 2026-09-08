// 第七幕: SwipeGestureHandler / 触摸手势
// 无需 @material, 纯 Slint 标准组件 + 手势处理器

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    slint_build::compile("ui/app.slint")
        .expect("Failed to compile Slint UI");
}
