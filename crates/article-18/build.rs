// article-18 build.rs
//
// 只做一件事: 用 slint-build 编译 ui/app.slint(生成的代码由 include_modules! 引入)

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=ui/app.slint");
    slint_build::compile("ui/app.slint").expect("slint compile");
}
