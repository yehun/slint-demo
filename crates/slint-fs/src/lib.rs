//! # slint-fs
//!
//! `PlatformPath` 统一路径抽象: desktop 本地路径 / Android content URI 双模式分发.
//!
//! `Platform*` 前缀表示**统一类型**: 内部按当前平台分发 (desktop 走 `std::fs`,
//! Android 走 JNI + ContentResolver/MediaStore), 调用方直接使用, 无需任何 `cfg` 分支.
//!
//! ## Android 侧说明
//!
//! - JNI 环境经 [`ndk-context`] 自举 (android-activity 在 `android_main` 启动时初始化),
//!   无需显式 init, 任意线程可用.
//! - content URI 支持: 创建 (MediaStore insert)、读写 (openFileDescriptor + fd)、
//!   删除、查询 (文件名/大小/MIME/存在性/父子路径).
//! - 暂不支持: `list_files` 的 MediaStore 列表查询 (返回 `NotSupported`).
//!
//! ## 示例
//!
//! ```rust
//! use slint_fs::PlatformPath;
//!
//! // 本地路径
//! let dir = PlatformPath::from_local("/tmp/demo");
//! let file = PlatformPath::create_file(&dir, "sub", "a.txt",
//!     slint_fs::PlatformFileFormat::Mp3)?;
//! let mut f = file.write_file()?;
//!
//! // Android 返回的 content URI 字符串也能直接解析
//! let p = PlatformPath::new("content://media/external/images/media/100");
//! assert!(p.is_content());
//! ```

pub mod mime_type {
    pub use mime_type::*;
}

#[cfg(target_os = "android")]
mod android;

mod error;
pub use error::{Error, Result};

mod file_entry;
pub use file_entry::PlatformFileEntry;

mod file_type;
pub use file_type::{
    PlatformFileFormat,
    PlatformFileType
};

mod file_path;
pub use file_path::PlatformPath;

#[cfg(target_os = "android")]
mod file_descriptor;

mod file;
pub use file::PlatformFile;

mod file_access;
pub use file_access::PlatformFileMode;
