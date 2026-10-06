// 第十七篇 — 跨线程共享状态
//
// 三条规矩:
// 1. OCR 引擎(slint_ocr::OcrService)只在工作线程里创建/使用, UI 线程绝不持有它
// 2. 一切 UI 更新都经 ui() 回到 Slint 事件循环(invoke_from_event_loop)
// 3. 识别结果(检测框 + 文本)留在 Rust 侧, UI 只拿预览图与文本描述

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, SharedString, Weak};

use crate::{OcrModel, MainWindow};
use slint_ocr::OcrService;

pub struct AppState {
    weak: Mutex<Weak<MainWindow>>,
    /// OCR 引擎(加载完成后 Some)
    pub engine: Arc<Mutex<Option<OcrService>>>,
    /// 已选图片路径
    pub image_path: Arc<Mutex<Option<PathBuf>>>,
    /// 模型目录(自动发现或用户指定)
    pub model_dir: Arc<Mutex<Option<PathBuf>>>,
    /// 是否有耗时任务在跑(模型加载 / 识别)
    pub busy: Arc<AtomicBool>,
}

impl AppState {
    pub fn new(weak: Weak<MainWindow>) -> Self {
        Self {
            weak: Mutex::new(weak),
            engine: Arc::new(Mutex::new(None)),
            image_path: Arc::new(Mutex::new(None)),
            model_dir: Arc::new(Mutex::new(None)),
            busy: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 事件循环线程里取窗口句柄(读 UI 属性用; 工作线程不要长期持有)
    pub fn weak_upgrade(&self) -> Option<MainWindow> {
        self.weak.lock().unwrap().upgrade()
    }

    /// 把闭包丢回 Slint 事件循环执行(任何线程都能调)
    pub fn ui<F>(&self, f: F)
    where
        F: FnOnce(&MainWindow) + Send + 'static,
    {
        let weak = self.weak.lock().unwrap().clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(app) = weak.upgrade() {
                f(&app);
            }
        });
    }

    pub fn set_status(&self, text: impl Into<SharedString>) {
        let text: SharedString = text.into();
        self.ui(move |app| app.global::<OcrModel>().set_status(text));
    }

    pub fn set_busy(&self, busy: bool) {
        self.busy.store(busy, Ordering::SeqCst);
        self.ui(move |app| app.global::<OcrModel>().set_busy(busy));
    }

    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }
}
