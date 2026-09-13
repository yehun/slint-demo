// 第十一幕: 文件选择与写入
//
// 核心知识点:
// 1. slint-file-picker —— 跨平台文件选择器 (desktop rfd / Android SAF), 回调统一在后台线程
// 2. slint-fs —— PlatformPath 统一路径, 选完直接读写 (Android content URI 已带授权)
// 3. 后台回调 → slint::invoke_from_event_loop —— 文件 IO 在后台线程做, UI 更新切回主线程
// 4. flush 的平台差异 —— Android 上 flush 触发 IS_PENDING=0, 文件才对其他应用可见
// 5. 对比第十幕: 平台细节全部下沉到公共 crate 后, 应用层只剩 UI 与业务编排
//
// 平台路径:
//   desktop: rfd 对话框 → 本地路径 → std::fs
//   Android: SAF 对话框 → content URI → openFileDescriptor (MediaStore)

slint::include_modules!();

use std::io::Write;

use slint_file_picker::{pick_file, pick_file_to_save, FileFilter, PickResult};

// ===== 公共辅助: 注册 UI 回调 =====

fn setup_ui_callbacks(main_window: &MainWindow) {
    // --- 打开文件: 选 → 后台读 → 主线程填充编辑器 ---
    let win_weak = main_window.as_weak();
    main_window.global::<EditorModel>().on_open_file(move || {
        let Some(win) = win_weak.upgrade() else { return };
        win.global::<EditorModel>()
            .set_status("正在打开文件选择器...".into());

        let win_weak = win_weak.clone();
        pick_file(
            vec![
                FileFilter::new("文本文件")
                    .extension("txt")
                    .extension("md")
                    .extension("rs")
                    .extension("json")
                    .extension("toml")
                    .mime("text/plain"),
                FileFilter::new("所有文件").mime("*/*"),
            ],
            move |result| {
                // ---- 后台线程: 文件 IO 放这里, 不卡 UI ----
                let payload = match result {
                    PickResult::Picked(path) => {
                        let display = path.to_string();
                        let name = path
                            .file_name()
                            .unwrap_or_else(|_| "未命名".to_string());
                        match path.read_file().and_then(|mut f| f.read_string()) {
                            Ok(text) => Ok((display, name, text)),
                            Err(e) => Err(format!("读取失败: {e}")),
                        }
                    }
                    PickResult::Cancelled => return,
                    PickResult::Error(e) => Err(format!("选择失败: {e}")),
                };

                // ---- 切回主线程更新 UI ----
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(win) = win_weak.upgrade() else { return };
                    let model = win.global::<EditorModel>();
                    match payload {
                        Ok((display, name, text)) => {
                            model.set_file_path(display.into());
                            model.set_editor_text(text.into());
                            model.set_status(format!("已打开: {name}").into());
                        }
                        Err(e) => model.set_status(e.into()),
                    }
                });
            },
        );
    });

    // --- 保存写入: 抓取编辑内容 → 选目标 → 后台写 → 主线程报状态 ---
    let win_weak = main_window.as_weak();
    main_window.global::<EditorModel>().on_save_file(move || {
        let Some(win) = win_weak.upgrade() else { return };
        let content = win.global::<EditorModel>().get_editor_text().to_string();
        win.global::<EditorModel>()
            .set_status("正在打开保存对话框...".into());

        let win_weak = win_weak.clone();
        pick_file_to_save(
            "untitled.txt",
            vec![
                FileFilter::new("文本文件")
                    .extension("txt")
                    .extension("md")
                    .mime("text/plain"),
            ],
            move |result| {
                // ---- 后台线程: 写文件 ----
                let payload = match result {
                    PickResult::Picked(path) => {
                        let display = path.to_string();
                        let write = (|| -> Result<usize, String> {
                            let mut f = path.write_file().map_err(|e| e.to_string())?;
                            let n = f
                                .write_all(content.as_bytes())
                                .map_err(|e| e.to_string())
                                .map(|_| content.len())?;
                            // Android 上 flush 会把 IS_PENDING 置 0, 文件才真正可见
                            f.flush().map_err(|e| e.to_string())?;
                            f.sync_all().map_err(|e| e.to_string())?;
                            Ok(n)
                        })();
                        match write {
                            Ok(n) => Ok((display, n)),
                            Err(e) => Err(format!("写入失败: {e}")),
                        }
                    }
                    PickResult::Cancelled => return,
                    PickResult::Error(e) => Err(format!("保存失败: {e}")),
                };

                // ---- 切回主线程更新状态栏 ----
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(win) = win_weak.upgrade() else { return };
                    let model = win.global::<EditorModel>();
                    match payload {
                        Ok((display, n)) => {
                            model.set_file_path(display.into());
                            model.set_status(format!("已写入 {n} 字节").into());
                        }
                        Err(e) => model.set_status(e.into()),
                    }
                });
            },
        );
    });
}

// ===== Desktop 入口 =====

#[cfg(feature = "desktop")]
pub fn desktop_main() {
    let main_window = MainWindow::new().expect("创建主窗口失败");
    setup_ui_callbacks(&main_window);
    main_window.show().expect("显示窗口失败");
    slint::run_event_loop().expect("事件循环异常");
}

// ===== Android 入口 =====
//
// 对比第十幕: 不再需要 init/缓存类/Arc 模式 —— 平台细节都在
// slint-fs / slint-file-picker 里. Java 侧接入见 android/java/ 与 README.

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    slint::android::init(app).expect("Slint Android 初始化失败");

    let main_window = MainWindow::new().expect("创建主窗口失败");
    setup_ui_callbacks(&main_window);
    main_window.show().expect("显示窗口失败");
    slint::run_event_loop().expect("事件循环异常");
}
