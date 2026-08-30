//! # slint-common
//!
//! Slint 三端初始化的共享模板. 提供宏 `slint_demo!` 自动生成
//! desktop / android / wasm 三套入口, 让每个 demo crate 只需
//! 关心自己的界面和业务逻辑.
//!
//! ## 使用方式
//!
//! 在 demo crate 的 `lib.rs` 中:
//!
//! ```rust
//! slint::include_modules!();
//! slint_common::slint_demo!(MainWindow);
//! ```
//!
//! 然后在 `main.rs` 中:
//!
//! ```rust
//! fn main() {
//!     #[cfg(feature = "desktop")]
//!     {
//!         let w = app_lib::MainWindow::new().unwrap();
//!         app_lib::run_desktop(w);
//!     }
//! }
//! ```

/// 生成三端入口模板.
///
/// # 参数
///
/// - `$window`: 你的 MainWindow 类型 (由 `slint::include_modules!()` 生成)
///
/// # 生成内容
///
/// - `run_desktop(window)` — desktop 入口函数
/// - `android_main(app)` — android 入口 (仅 android target)
/// - `run_wasm()` — wasm 入口 (仅 wasm32 target)
///
/// # 示例
///
/// ```rust
/// slint::include_modules!();
/// slint_common::slint_demo!(MainWindow);
/// ```
#[macro_export]
macro_rules! slint_demo {
    ($window:ty) => {
        // ============================================================
        // Desktop 入口
        // ============================================================
        #[cfg(feature = "desktop")]
        pub fn run_desktop(window: $window) {
            window.show().expect("显示窗口失败");
            slint::run_event_loop().expect("事件循环异常");
        }

        // ============================================================
        // Android 入口
        //   编译目标: aarch64-linux-android
        //   需要: Android NDK + android.jar
        // ============================================================
        #[cfg(all(feature = "android", target_os = "android"))]
        #[unsafe(no_mangle)]
        fn android_main(app: slint::android::AndroidApp) {
            slint::android::init(app).expect("Slint Android 初始化失败");
            let window = <$window>::new().expect("创建主窗口失败");
            window.show().expect("显示窗口失败");
            slint::run_event_loop().expect("事件循环异常");
        }

        // ============================================================
        // WASM 入口
        //   编译目标: wasm32-unknown-unknown
        //   需要: wasm-pack 或 wasm-bindgen-cli
        // ============================================================
        #[cfg(all(feature = "wasm", target_arch = "wasm32"))]
        #[wasm_bindgen::prelude::wasm_bindgen(start)]
        pub async fn run_wasm() {
            let window = <$window>::new().expect("创建主窗口失败");
            window.show().expect("显示窗口失败");
            slint::run_event_loop().expect("事件循环异常");
        }
    };
}

/// 重新导出, 方便 demo crate 使用
pub use slint;
