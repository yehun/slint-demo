// Desktop 实现: rfd 原生文件对话框.
//
// 统一在后台线程打开 (与 Android 的异步回调语义一致), 回调在后台线程执行.

use std::thread;

use slint_fs::PlatformPath;

use crate::{FileFilter, PickResult};

fn run_dialog(
    save: Option<String>,
    filters: Vec<FileFilter>,
    callback: impl FnOnce(PickResult) + Send + 'static,
) {
    thread::spawn(move || {
        let mut dialog = rfd::FileDialog::new();
        for filter in &filters {
            let exts: Vec<&str> = filter.extensions.iter().map(|s| s.as_str()).collect();
            if !exts.is_empty() {
                dialog = dialog.add_filter(filter.name.clone(), &exts);
            }
        }

        let picked = match save {
            Some(default_name) => dialog
                .set_file_name(&default_name)
                .save_file()
                .map(PlatformPath::from_local),
            None => dialog
                .pick_file()
                .map(PlatformPath::from_local),
        };

        callback(match picked {
            Some(path) => PickResult::Picked(path),
            None => PickResult::Cancelled,
        });
    });
}

/// 打开文件选择对话框 (选择已有文件)
pub fn pick_file(filters: Vec<FileFilter>, callback: impl FnOnce(PickResult) + Send + 'static) {
    run_dialog(None, filters, callback);
}

/// 打开保存文件对话框 (创建或覆盖)
pub fn pick_file_to_save(
    default_name: impl Into<String>,
    filters: Vec<FileFilter>,
    callback: impl FnOnce(PickResult) + Send + 'static,
) {
    run_dialog(Some(default_name.into()), filters, callback);
}
