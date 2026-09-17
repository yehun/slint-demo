//! # slint-file-picker
//!
//! 跨平台文件选择器: 结果统一为 [`slint_fs::PlatformPath`],
//! 选完直接用 slint-fs 读写 (Android 的 content URI 已带读授权).
//!
//! | 平台 | 实现 |
//! |---|---|
//! | desktop | `rfd` 原生文件对话框 |
//! | Android | SAF `ACTION_OPEN_DOCUMENT` / `ACTION_CREATE_DOCUMENT` (经 Java 侧发起) |
//! | WASM | 暂不支持 |
//!
//! ## API
//!
//! 回调式 (Android 侧系统对话框是异步的, desktop 内部也在后台线程打开,
//! **回调一律在后台线程执行**, 更新 Slint UI 请用 `slint::invoke_from_event_loop`):
//!
//! ```rust
//! use slint_file_picker::{pick_file, FileFilter, PickResult};
//!
//! pick_file(
//!     vec![FileFilter::new("文本文件").extension("txt").extension("md")
//!         .mime("text/plain")],
//!     |result| match result {
//!         PickResult::Picked(path) => {
//!             let mut f = path.read_file().unwrap();
//!             let text = f.read_string().unwrap();
//!             println!("{text}");
//!         }
//!         PickResult::Cancelled => {}
//!         PickResult::Error(e) => eprintln!("{e}"),
//!     },
//! );
//! ```
//!
//! ## Android 接入 (三步)
//!
//! 1. 把 `android/java/com/yehun/slintfs/SlintFilePicker.java` 复制到
//!    demo crate 的 `android/java/com/yehun/slintfs/` (保持包路径);
//! 2. demo 的 `MainActivity` 里加一个转发:
//!
//!    ```java
//!    @Override
//!    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
//!        super.onActivityResult(requestCode, resultCode, data);
//!        com.yehun.slintfs.SlintFilePicker.handle(requestCode, resultCode, data);
//!    }
//!    ```
//!
//! 3. 无需任何存储权限 (SAF 授权模型).

/// 文件类型过滤器: desktop 按扩展名, Android 按 MIME 类型
#[derive(Debug, Clone)]
pub struct FileFilter {
    pub name: String,
    pub extensions: Vec<String>,
    pub mime_types: Vec<String>,
}

impl FileFilter {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            extensions: Vec::new(),
            mime_types: Vec::new(),
        }
    }

    pub fn extension(mut self, ext: impl Into<String>) -> Self {
        self.extensions.push(ext.into());
        self
    }

    pub fn mime(mut self, mime: impl Into<String>) -> Self {
        self.mime_types.push(mime.into());
        self
    }
}

/// 选择结果. `Picked` 携带统一路径, 可直接读写.
#[derive(Debug)]
pub enum PickResult {
    Picked(slint_fs::PlatformPath),
    Cancelled,
    Error(String),
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
mod desktop;

#[cfg(target_os = "android")]
mod android;

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
pub use desktop::{
    pick_file,
    pick_file_to_save,
    pick_files,
};

#[cfg(target_os = "android")]
pub use android::{
    pick_file,
    pick_file_to_save,
    pick_files,
};
