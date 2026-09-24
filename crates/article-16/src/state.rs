// 第十六篇 — 跨线程共享状态
//
// 三条规矩:
// 1. 模型(LuxTTS)只在工作线程里创建/使用, UI 线程绝不持有它
// 2. 一切 UI 更新都经 ui() 回到 Slint 事件循环(invoke_from_event_loop)
// 3. 合成结果(48kHz f32 波形)留在 Rust 侧, UI 只拿一个 bool + 描述文本

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, SharedString, Weak};
use slint_lux_tts::LuxTTS;

use crate::{CloneModel, MainWindow};

pub struct AppState {
    weak: Mutex<Weak<MainWindow>>,
    /// 推理引擎(加载完成后 Some)
    pub engine: Arc<Mutex<Option<Arc<LuxTTS>>>>,
    /// 最近一次合成的波形(48kHz 单声道)
    pub last: Arc<Mutex<Option<Vec<f32>>>>,
    /// 参考音频路径
    pub ref_path: Arc<Mutex<Option<PathBuf>>>,
    /// 模型目录(自动发现或用户指定)
    pub model_dir: Arc<Mutex<Option<PathBuf>>>,
    /// 是否有耗时任务在跑(模型加载 / 合成)
    pub busy: Arc<AtomicBool>,
    /// 播放代数: 旧线程的"播放结束"回写不得覆盖新的一次播放
    pub play_gen: Arc<Mutex<u64>>,
}

impl AppState {
    pub fn new(weak: Weak<MainWindow>) -> Self {
        Self {
            weak: Mutex::new(weak),
            engine: Arc::new(Mutex::new(None)),
            last: Arc::new(Mutex::new(None)),
            ref_path: Arc::new(Mutex::new(None)),
            model_dir: Arc::new(Mutex::new(None)),
            busy: Arc::new(AtomicBool::new(false)),
            play_gen: Arc::new(Mutex::new(0)),
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
        self.ui(move |app| app.global::<CloneModel>().set_status(text));
    }

    pub fn set_busy(&self, busy: bool) {
        self.busy.store(busy, Ordering::SeqCst);
        self.ui(move |app| app.global::<CloneModel>().set_busy(busy));
    }

    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    /// 参考音频就绪 = 选了文件 + 填了转写文本(音素对齐必须要文本)
    pub fn refresh_ref_ready(&self) {
        let has_path = self.ref_path.lock().unwrap().is_some();
        let has_text = !self.ref_text().is_empty();
        self.ui(move |app| app.global::<CloneModel>().set_ref_ready(has_path && has_text));
    }

    fn ref_text(&self) -> String {
        // UI 是唯一真源: 没有 weak 时按"无文本"处理, 只影响按钮可用性
        self.weak
            .lock()
            .unwrap()
            .upgrade()
            .map(|app| app.global::<CloneModel>().get_ref_text().to_string())
            .unwrap_or_default()
    }
}
