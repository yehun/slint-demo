// 第二十篇, 证件照排版
//
// 核心知识点:
// 1. 排版本质是整数规划里的"小盒装大盒": footprint = 照片mm + 两侧边框mm,
//    floor(纸宽/footW) × floor(纸高/footH), 横竖两种朝向各算一遍取块数多者
// 2. 预览与导出共用同一套排版公式: 预览由 Slint 纯派生函数即时算(改参数秒刷),
//    导出由 Rust 按 300dpi 重新算一遍 —— 两侧语义必须一致
// 3. 比例裁剪: 选框锁定目标宽高比(拖动移动 + 四角缩放, 高随宽联动),
//    UI 回传归一化(0..1)左上角 + 宽高, Rust 按像素钳位裁剪
// 4. 导出 = pick_file_to_save 选落点 → 后台线程 300dpi 合成(照片 Lanczos3
//    缩放 + 白底块 + 逐块铺纸) → PNG 落盘, 全程不堵 UI
//
// 入口:
//   desktop_main —— Windows / Linux / macOS
//   android_main —— Android (cargo apk2, 文件选/存走 SAF, 无需存储权限)

slint::include_modules!();

// 排版/裁剪/合成是纯逻辑, 公开出去: 无 GUI 也能用(examples/make-sample.rs)
pub mod core;

use std::io::Read;
use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, SharedPixelBuffer};
use slint_file_picker::{pick_file_to_save, pick_image, FileFilter, PickResult};
use slint_fs::PlatformPath;

/// 跨回调共享状态: 源图像素(选图后暂存整图, 确认裁剪后替换为裁出的照片)。
pub struct AppState {
    app: slint::Weak<MainWindow>,
    source: Mutex<Option<image::RgbaImage>>,
}

impl AppState {
    fn new(app: slint::Weak<MainWindow>) -> Self {
        Self { app, source: Mutex::new(None) }
    }

    /// 后台线程 → UI 线程的统一通道(文件回调跑在后台线程, 属性必须回 UI 线程写)
    fn ui(&self, f: impl FnOnce(&MainWindow) + Send + 'static) {
        let app = self.app.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(app) = app.upgrade() {
                f(&app);
            }
        });
    }

    fn set_status(&self, text: impl Into<slint::SharedString>) {
        let text = text.into();
        self.ui(move |app| app.global::<IdPhotoModel>().set_status(text));
    }
}

pub fn desktop_main() {
    let app = MainWindow::new().expect("创建窗口失败");
    bind(&app);
    app.run().expect("运行窗口失败");
}

fn bind(app: &MainWindow) {
    let state = Arc::new(AppState::new(app.as_weak()));
    let model = app.global::<IdPhotoModel>();

    // ===== 选图: 相册选图(Android ACTION_PICK / 桌面图片对话框) → 后台解码 =====
    let s = state.clone();
    model.on_select_image(move || {
        // Slint 回调是 FnMut(可能被多次触发), pick 回调是 FnOnce ——
        // s 不能被内层闭包"吃掉", 进内层前必须再 clone 一份
        let s = s.clone();
        pick_image(move |result| match result {
            PickResult::Picked(p) => load_image(s, p),
            PickResult::Cancelled => {}
            PickResult::Error(e) => s.set_status(format!("选择失败: {e}")),
        });
    });

    // ===== 确认比例裁剪: 归一化选框 → 像素钳位裁剪 → 预览点亮 =====
    // (裁剪只是 crop_imm + to_image, 微秒级, 直接在 UI 线程做)
    let s = state.clone();
    model.on_confirm_crop(move |nx, ny, nw, nh| {
        let Some(img) = s.source.lock().unwrap().clone() else { return };
        let photo = core::crop_ratio(&img, nx, ny, nw, nh);
        let (w, h) = photo.dimensions();
        let preview =
            SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(photo.as_raw(), w, h);
        *s.source.lock().unwrap() = Some(photo);

        let app = s.app.upgrade().unwrap();
        let g = app.global::<IdPhotoModel>();
        g.set_preview(slint::Image::from_rgba8(preview));
        g.set_has_image(true);
        g.set_status("已裁剪 · 排版预览已更新".into());
    });

    // ===== 清除照片: 回到空态 =====
    let s = state.clone();
    model.on_clear_image(move || {
        *s.source.lock().unwrap() = None;
        let app = s.app.upgrade().unwrap();
        let g = app.global::<IdPhotoModel>();
        g.set_has_image(false);
        g.set_preview(slint::Image::default());
        g.set_status("已清除, 点纸面重新选图".into());
    });

    // ===== 导出: Android 直存相册(MediaStore) / 桌面选落点 → 后台 300dpi 合成 =====
    let s = state.clone();
    model.on_export(move || {
        if s.source.lock().unwrap().is_none() {
            s.set_status("请先选照片并裁剪");
            return;
        }
        // 参数在 UI 线程读齐(属性不能跨线程读), 随数据一起带入后台
        let (target, paper, b_enabled, b_mm) = {
            let app = s.app.upgrade().unwrap();
            let g = app.global::<IdPhotoModel>();
            (g.get_target_index(), g.get_paper_index(), g.get_border_enabled(), g.get_border_width())
        };
        let Some(img) = s.source.lock().unwrap().clone() else { return };
        // 时间戳文件名避免相册重名
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let file_name = format!("id-photo-{ts}.png");

        // 后台合成 + 写盘(两侧共用; 缩放整张 6 寸纸约几十毫秒, 不堵 UI)
        let compose_and_write = move |path: PlatformPath| -> Result<(), String> {
            let sheet = core::compose_sheet(&img, target, paper, b_enabled, b_mm)?;
            let mut png = Vec::new();
            image::DynamicImage::ImageRgba8(sheet)
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .map_err(|e| e.to_string())?;
            let mut file = path.write_file().map_err(|e| e.to_string())?;
            std::io::Write::write_all(&mut file, &png).map_err(|e| e.to_string())?;
            // 关键: 清 IS_PENDING 提交入库 —— MediaStore 里 pending 条目
            // 在相册中不可见, flush 会走 update 把 IS_PENDING 置 0
            std::io::Write::flush(&mut file).map_err(|e| e.to_string())?;
            Ok(())
        };

        #[cfg(target_os = "android")]
        {
            // 同桌面分支: FnMut 回调里进 'static 闭包前必须 clone
            let s = s.clone();
            std::thread::spawn(move || {
                let result = (|| -> Result<(), String> {
                    // 直接插入系统相册: MediaStore images 集合(⚠ 必须带 /media 段,
                    // insert 到 content://media/external/images 会被 MediaProvider 拒)
                    // RELATIVE_PATH = DCIM/Article20/, IS_PENDING 写完自动清零, 免存储权限
                    let base = PlatformPath::new("content://media/external/images/media");
                    let target_path = PlatformPath::create_file(
                        &base, "Article20", &file_name, slint_fs::PlatformFileFormat::Png,
                    )
                    .map_err(|e| e.to_string())?;
                    compose_and_write(target_path)
                })();
                match result {
                    Ok(()) => s.set_status("已保存到相册 DCIM/Article20/"),
                    Err(e) => {
                        // logcat 可靠留存(防其他应用刷屏滚掉), 状态栏只显示摘要
                        eprintln!("id-photo export error: {e}");
                        s.set_status(format!("导出失败: {e}"));
                    }
                }
            });
        }
        #[cfg(not(target_os = "android"))]
        {
            // 同 on_select_image: FnMut 回调里进 FnOnce 闭包前必须 clone
            let s = s.clone();
            let filters = vec![FileFilter::new("PNG 图片").extension("png").mime("image/png")];
            pick_file_to_save(file_name, filters, move |result| match result {
                PickResult::Picked(p) => match compose_and_write(p) {
                    Ok(()) => s.set_status("导出完成"),
                    Err(e) => s.set_status(format!("导出失败: {e}")),
                },
                PickResult::Cancelled => {}
                PickResult::Error(e) => s.set_status(format!("保存失败: {e}")),
            });
        }
    });
}

/// 解码选中文件: 动图取首帧, 统一转 rgba8。
/// 解码完成后推整图预览 + 像素尺寸, 自增 crop-epoch → UI 弹出比例裁剪层。
fn load_image(state: Arc<AppState>, path: slint_fs::PlatformPath) {
    state.set_status("解码中…");

    let read = (|| -> Result<Vec<u8>, String> {
        let mut file = path.read_file().map_err(|e| e.to_string())?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        Ok(buf)
    })();

    let decoded = read
        .and_then(|data| image::load_from_memory(&data).map_err(|e| e.to_string()))
        .map(|img| img.into_rgba8());

    match decoded {
        Ok(img) => {
            let (w, h) = img.dimensions();
            // SharedPixelBuffer 是 Send 的, 可以从后台线程搬进 UI 线程闭包;
            // slint::Image 内部 VRc 不 Send, 必须到 UI 线程再 from_rgba8
            let preview =
                SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(img.as_raw(), w, h);
            *state.source.lock().unwrap() = Some(img);
            state.ui(move |app| {
                let g = app.global::<IdPhotoModel>();
                g.set_full_preview(slint::Image::from_rgba8(preview));
                g.set_full_w(w as f32);
                g.set_full_h(h as f32);
                // 自增计数而非 bool: 连续两次选图值必然变化, changed 必触发
                g.set_crop_epoch(g.get_crop_epoch() + 1);
                g.set_status("拖动选框对准人脸, 确认裁剪".into());
            });
        }
        Err(e) => state.set_status(format!("解码失败: {e}")),
    }
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    // 崩溃先落到 stderr(会被 logcat 捕获), 否则 native 闪退没有任何线索。
    std::panic::set_hook(Box::new(|info| {
        eprintln!("RUST PANIC: {info}");
    }));

    // 把真实 Activity 全局引用注册给 slint-file-picker:
    // SAF 的选图/保存必须由真 Activity 发起 (ndk_context 里是 Application)
    // ⛔ init(app) 按值消费 app, 取指针必须放在它前面
    slint_file_picker::set_activity(app.activity_as_ptr());
    slint::android::init(app).expect("init android");

    let window = MainWindow::new().expect("创建窗口失败");
    bind(&window);

    // Android 软件渲染器没有跨字体族的字形回退, 指定设备自带的全量简体字体族。
    window.global::<IdPhotoModel>().set_font_family("Noto Sans CJK SC".into());

    window.run().expect("运行窗口失败");
}
