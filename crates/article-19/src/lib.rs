// 第十九篇, 噪音测量
//
// 核心知识点:
// 1. 麦克风采集用 cpal 的输入流(build_input_stream), 支持 f32/i16/u16 多种采样格式
// 2. 声压级 = 对一小段采样做 RMS → 取 20*log10(rms) 得到 dBFS(相对满量程分贝)
// 3. 加一个校准偏移(0 dBFS ≈ 偏移 dB SPL)即可把读数换算成近似声压级; 真校准需参考音
// 4. 实时 UI 全程跑在 Slint 事件循环: Timer 每 50ms 取一次采样窗口, 算完直接写属性
// 5. 历史曲线用 Rust 手绘进 SharedPixelBuffer<RGB8>, 再塞给 Slint 的 image 属性
//
// 入口:
//   desktop_main —— Windows / Linux / macOS
//   android_main —— Android (cargo apk2, 需 RECORD_AUDIO 权限)

slint::include_modules!();

mod audio;

use std::cell::RefCell;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::time::Duration;

use slint::{ComponentHandle, Timer, TimerMode};

/// 跨回调共享的测量状态。
pub struct AppState {
    /// 主窗口弱引用: Timer 回调里 upgrade 后写属性(避免持有强引用导致窗口关不掉)
    pub app: slint::Weak<MainWindow>,
    /// cpal 数据回调往里塞采样(单声道 f32, 范围约 [-1, 1]), Timer 定时抽干
    pub buf: Arc<Mutex<Vec<f32>>>,
    /// 当前/峰值/最低/平均 dB 与历史曲线缓冲
    pub level: Arc<Mutex<audio::LevelState>>,
    /// 是否正在采集(cpal 回调据此决定是否丢数据, 停止时直接停流)
    pub running: Arc<AtomicBool>,
}

impl AppState {
    fn new(app: slint::Weak<MainWindow>) -> Self {
        Self {
            app,
            buf: Arc::new(Mutex::new(Vec::with_capacity(8192))),
            level: Arc::new(Mutex::new(audio::LevelState::default())),
            running: Arc::new(AtomicBool::new(false)),
        }
    }
}

pub fn desktop_main() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .try_init();

    let app = MainWindow::new().expect("创建窗口失败");
    let state = Arc::new(AppState::new(app.as_weak()));
    bind(&app, state);
    app.run().expect("运行窗口失败");
}

/// 把 UI 回调与定时采样接到 AppState 上。
fn bind(app: &MainWindow, state: Arc<AppState>) {
    // 开始 / 停止 测量
    let s = state.clone();
    app.global::<NoiseModel>().on_toggle_measure(move || {
        // 在回调里再取一次强引用(此时窗口必然还在)
        let app = s.app.upgrade().unwrap();
        let g = app.global::<NoiseModel>();
        if s.running.load(Ordering::SeqCst) {
            audio::stop(&s.running);
            g.set_measuring(false);
            g.set_status("已停止".into());
        } else {
            let mut info = String::new();
            match audio::start(&s.buf, &s.running, &mut info) {
                Ok(()) => {
                    // 新一轮测量: 清空上一轮的统计, 历史曲线也重置
                    *s.level.lock().unwrap() = audio::LevelState::default();
                    g.set_measuring(true);
                    g.set_status("测量中… 对它说话, 或制造点声音".into());
                    g.set_device_text(info.into());
                }
                Err(e) => g.set_status(format!("启动失败: {e}").into()),
            }
        }
    });

    // 每 50ms 抽干采样窗口 → 算 dB → 写回 UI(回调跑在事件循环, 可直接写属性)
    let s = state.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
        let app = match s.app.upgrade() {
            Some(a) => a,
            None => return,
        };
        audio::tick(&s.buf, &s.level, &app);
    });
    // Timer 一被 drop 回调就停, 必须让它活过本函数(thread_local 保管到进程结束)
    METER_TIMER.with(|cell| *cell.borrow_mut() = Some(timer));
}

thread_local! {
    static METER_TIMER: RefCell<Option<Timer>> = const { RefCell::new(None) };
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    // 崩溃先落到 stderr(会被 logcat 捕获), 否则 native 闪退没有任何线索。
    std::panic::set_hook(Box::new(|info| {
        eprintln!("RUST PANIC: {info}");
    }));

    slint::android::init(app).expect("init android");

    let window = MainWindow::new().expect("创建窗口失败");
    let state = Arc::new(AppState::new(window.as_weak()));
    bind(&window, state);

    // Android 软件渲染器没有跨字体族的字形回退, 指定设备自带的全量简体字体族。
    window
        .global::<NoiseModel>()
        .set_font_family("Noto Sans CJK SC".into());

    window.run().expect("运行窗口失败");
}
