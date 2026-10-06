// 第十七篇 — 模型加载 / 图片选择 / 识别 / 预览框绘制 / 保存
//
// 全部耗时动作都在 std::thread 里跑:
//   - 加载模型: det/cls/rec 三个 ONNX 灌进 ort, 第一次需要几秒
//   - 识别:     原图 → det(文本检测) → cls(方向分类) → rec(文字识别), 结果带检测框
// 完成后用 AppState::ui 把结果(预览图 + 文本 + 耗时)写回 Slint 属性。
// 检测框由 Rust 画在原图上, 整张预览图回传给 Slint 的 Image 元素显示。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use slint::ComponentHandle;
use slint_file_picker::{pick_file, FileFilter, PickResult};
use slint_ocr::{OcrLine, OcrResult, OcrService};

use crate::state::AppState;
use crate::{OcrLineItem, OcrModel, MainWindow};

/// 模型目录发现顺序:
///   1. 环境变量 PP_OCR_MODEL_DIR
///   2. 可执行文件旁边的 models/pp-ocr
///   3. ~/.local/share/slint-demo/models/pp-ocr
///   4. 工作目录下的 models/pp-ocr
/// 权重文件仓库不分发(见 scripts/fetch-model.sh)
fn default_model_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("PP_OCR_MODEL_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return Some(p);
        }
    }
    // 可执行文件旁边的 models 目录(打包发布场景)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cand = dir.join("models/pp-ocr");
            if cand.is_dir() {
                return Some(cand);
            }
        }
    }
    let home_models = std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".local/share/slint-demo/models/pp-ocr"));
    if let Some(p) = home_models {
        if p.is_dir() {
            return Some(p);
        }
    }
    let local = PathBuf::from("models/pp-ocr");
    if local.is_dir() {
        return Some(local);
    }
    None
}

pub fn bind(app: &MainWindow, state: Arc<AppState>) {
    let s = state.clone();
    app.global::<OcrModel>().on_load_model(move || {
        if s.is_busy() {
            return;
        }
        match default_model_dir() {
            Some(dir) => load_model(s.clone(), dir),
            None => pick_model_dir(s.clone()),
        }
    });

    let s = state.clone();
    app.global::<OcrModel>().on_pick_image(move || {
        if s.is_busy() {
            return;
        }
        pick_image(s.clone());
    });

    let s = state.clone();
    app.global::<OcrModel>().on_recognize(move || {
        if s.is_busy() {
            return;
        }
        recognize(s.clone());
    });

    let s = state.clone();
    app.global::<OcrModel>().on_save_text(move || save_text(s.clone()));
}

/// 启动时自动发现模型目录(有就直接加载, 没有就留一句提示)
pub fn auto_discover_model(state: &Arc<AppState>) {
    match default_model_dir() {
        Some(dir) => {
            let shown = dir.clone();
            state.ui(move |app| {
                app.global::<OcrModel>()
                    .set_model_status(format!("发现模型目录: {}", shown.display()).into());
            });
            load_model(state.clone(), dir);
        }
        None => state.ui(|app| {
            app.global::<OcrModel>().set_model_status(
                "未找到模型目录。设置 PP_OCR_MODEL_DIR, 或点“加载模型”手动选择。".into(),
            );
        }),
    }
}

fn pick_model_dir(state: Arc<AppState>) {
    state.set_status("请选择模型目录里的 det.onnx / keys.txt");
    pick_file(
        vec![
            FileFilter::new("ONNX 模型")
                .extension("onnx")
                .mime("*/*"),
            FileFilter::new("字典文件")
                .extension("txt")
                .mime("text/plain"),
            FileFilter::new("所有文件").mime("*/*"),
        ],
        move |result| match result {
            PickResult::Picked(p) => {
                let path = PathBuf::from(p.to_string());
                let dir = if path.is_dir() {
                    path
                } else {
                    path.parent()
                        .map(Path::to_path_buf)
                        .unwrap_or(path)
                };
                load_model(state.clone(), dir);
            }
            PickResult::Cancelled => {}
            PickResult::Error(e) => state.set_status(format!("选择失败: {e}")),
        },
    );
}

fn load_model(state: Arc<AppState>, dir: PathBuf) {
    *state.model_dir.lock().unwrap() = Some(dir.clone());
    state.set_busy(true);
    state.set_status("正在加载模型…");
    let loading = dir.clone();
    state.ui(move |app| {
        app.global::<OcrModel>()
            .set_model_status(format!("加载中: {}", loading.display()).into());
    });

    std::thread::spawn(move || {
        let dir_disp = dir.display().to_string();
        // 包一层 catch_unwind: ort 在找不到动态库时会直接 panic(而非返回 Err),
        // 若直接崩溃在后台线程, UI 会永远停在"加载中"。这里把它转成可见的失败提示。
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let svc = OcrService::new();
            if !svc.ort_available() {
                return Err(anyhow::anyhow!(
                    svc.library_error()
                        .unwrap_or_else(|| "onnxruntime 动态库不可用".into())
                ));
            }
            svc.load_models_from_dir(&dir)?;
            Ok(svc)
        }));
        match result {
            Ok(Ok(svc)) => {
                *state.engine.lock().unwrap() = Some(svc);
                state.set_busy(false);
                state.set_status("模型已就绪, 选一张图片开始识别");
                state.ui(move |app| {
                    let m = app.global::<OcrModel>();
                    m.set_model_ready(true);
                    m.set_model_status(format!("已加载: {dir_disp}").into());
                });
            }
            Ok(Err(e)) => {
                state.set_busy(false);
                state.set_status(format!("加载失败: {e}"));
                state.ui(move |app| {
                    app.global::<OcrModel>()
                        .set_model_status(format!("加载失败: {e}").into());
                });
            }
            Err(_) => {
                state.set_busy(false);
                state.set_status(
                    "加载崩溃: 推理库初始化失败, 请确认已安装 onnxruntime 或设置 ORT_DYLIB_PATH",
                );
                state.ui(move |app| {
                    app.global::<OcrModel>().set_model_status(
                        "加载崩溃: 未找到 onnxruntime 动态库(onnxruntime.so), 请安装后重试".into(),
                    );
                });
            }
        }
    });
}

fn pick_image(state: Arc<AppState>) {
    pick_file(
        vec![
            FileFilter::new("图片")
                .extension("png")
                .extension("jpg")
                .extension("jpeg")
                .extension("bmp")
                .extension("gif")
                .mime("image/*"),
            FileFilter::new("所有文件").mime("*/*"),
        ],
        move |result| match result {
            PickResult::Picked(p) => apply_image(state.clone(), PathBuf::from(p.to_string())),
            PickResult::Cancelled => {}
            PickResult::Error(e) => state.set_status(format!("选择失败: {e}")),
        },
    );
}

fn apply_image(state: Arc<AppState>, path: PathBuf) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());

    *state.image_path.lock().unwrap() = Some(path.clone());
    state.ui(move |app| {
        let m = app.global::<OcrModel>();
        m.set_image_name(name.into());
        m.set_image_ready(true);
        if let Ok(img) = slint::Image::load_from_path(&path) {
            m.set_preview(img);
        }
    });
    state.set_status("图片已选择, 点“开始识别”");
}

fn recognize(state: Arc<AppState>) {
    let Some(img_path) = state.image_path.lock().unwrap().clone() else {
        state.set_status("请先选择图片");
        return;
    };
    {
        let guard = state.engine.lock().unwrap();
        if guard.is_none() {
            state.set_status("请先加载模型");
            return;
        }
    }

    state.set_busy(true);
    state.set_status("识别中: 文本检测 → 方向分类 → 文字识别…");

    std::thread::spawn(move || {
        let res = (|| -> anyhow::Result<(OcrResult, image::DynamicImage, PathBuf)> {
            let engine_arc = state.engine.clone();
            let guard = engine_arc.lock().unwrap();
            let svc = match guard.as_ref() {
                Some(s) => s,
                None => return Err(anyhow::anyhow!("模型未加载")),
            };
            let img = image::open(&img_path)?;
            let result = svc.recognize(&img)?;
            drop(guard);
            Ok((result, img, img_path.clone()))
        })();

        match res {
            Ok((result, img, _path)) => {
                let line_count = result.lines.len();

                // 纯文本(用于保存 / 复制): 每行原文, 不带置信度
                let text = if line_count == 0 {
                    "(未检测到文字)".to_string()
                } else {
                    result
                        .lines
                        .iter()
                        .map(|l| l.text.clone())
                        .collect::<Vec<_>>()
                        .join("\n")
                };

                // 结构化逐行结果(带置信度), 用于在列表里展示
                let lines_model: Vec<OcrLineItem> = result
                    .lines
                    .iter()
                    .enumerate()
                    .map(|(i, l)| OcrLineItem {
                        index: i as i32,
                        text: l.text.clone().into(),
                        confidence: l.confidence,
                    })
                    .collect();

                // 在原图上画检测框 → 预览 PNG(路径跨线程传给闭包, 闭包内再加载 Image)
                let preview_path = match render_preview(&img, &result.lines) {
                    Ok(p) => p,
                    Err(e) => {
                        log::warn!("预览框绘制失败: {e}");
                        let fb = PathBuf::from("output/ocr_preview.png");
                        let _ = std::fs::create_dir_all("output");
                        let _ = img.to_rgba8().save(&fb);
                        fb
                    }
                };

                state.set_busy(false);
                state.set_status(format!("识别完成: {line_count} 行文字"));
                state.ui(move |app| {
                    let m = app.global::<OcrModel>();
                    m.set_preview(
                        slint::Image::load_from_path(&preview_path).unwrap_or_default(),
                    );
                    m.set_result_text(text.into());
                    m.set_result_lines(lines_model.as_slice().into());
                    m.set_det_ms(result.det_ms as i32);
                    m.set_rec_ms(result.rec_ms as i32);
                    m.set_total_ms(result.total_ms as i32);
                    m.set_has_result(true);
                });
            }
            Err(e) => {
                state.set_busy(false);
                state.set_status(format!("识别失败: {e}"));
            }
        }
    });
}

/// 在原图 RGBA 上画红色检测框, 存成 output/ocr_preview.png, 返回预览图路径
/// (返回路径而非 `slint::Image`: Image 不是 Send, 不能跨线程带进事件循环闭包,
///  由闭包内在 UI 线程用 `load_from_path` 加载)
fn render_preview(
    img: &image::DynamicImage,
    lines: &[OcrLine],
) -> anyhow::Result<PathBuf> {
    let mut rgba = img.to_rgba8();
    let (iw, ih) = rgba.dimensions();
    for line in lines {
        // 检测框外扩半框高(每边 +h/2), 让红框明显圈住整行文字,
        // 否则紧框看起来偏小(对齐成熟 OCR 可视化的做法)。
        let pad = (line.h as f32 * 0.5) as u32;
        let x = line.x.saturating_sub(pad);
        let y = line.y.saturating_sub(pad);
        let w = (line.w + pad * 2).min(iw.saturating_sub(x));
        let h = (line.h + pad * 2).min(ih.saturating_sub(y));
        if w == 0 || h == 0 {
            continue;
        }
        draw_rect_outline(&mut rgba, x, y, w, h, [255, 82, 82, 255]);
    }
    std::fs::create_dir_all("output")?;
    let path = PathBuf::from("output/ocr_preview.png");
    rgba.save(&path)?;
    Ok(path)
}

/// 在 RGBA 图上画矩形边框(线宽 2)
fn draw_rect_outline(
    img: &mut image::RgbaImage,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    color: [u8; 4],
) {
    let (iw, ih) = img.dimensions();
    if iw == 0 || ih == 0 {
        return;
    }
    let t = 2u32;
    let x0 = x.min(iw - 1);
    let y0 = y.min(ih - 1);
    let x1 = (x + w).min(iw);
    let y1 = (y + h).min(ih);
    let c = image::Rgba(color);

    for i in x0..x1 {
        for dy in 0..t {
            let yy = y0.saturating_add(dy);
            if yy < ih {
                img.put_pixel(i, yy, c);
            }
        }
        for dy in 0..t {
            let yy = y1.saturating_sub(1).saturating_sub(dy);
            if yy < ih && yy >= y0 {
                img.put_pixel(i, yy, c);
            }
        }
    }
    for j in y0..y1 {
        for dx in 0..t {
            let xx = x0.saturating_add(dx);
            if xx < iw {
                img.put_pixel(xx, j, c);
            }
        }
        for dx in 0..t {
            let xx = x1.saturating_sub(1).saturating_sub(dx);
            if xx < iw && xx >= x0 {
                img.put_pixel(xx, j, c);
            }
        }
    }
}

fn save_text(state: Arc<AppState>) {
    let text = match state.weak_upgrade() {
        Some(app) => app.global::<OcrModel>().get_result_text().to_string(),
        None => return,
    };
    let dir = PathBuf::from("output");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        state.set_status(format!("创建输出目录失败: {e}"));
        return;
    }
    let path = dir.join("ocr_result.txt");
    match std::fs::write(&path, text.as_bytes()) {
        Ok(()) => state.set_status(format!("已保存: {}", path.display())),
        Err(e) => state.set_status(format!("保存失败: {e}")),
    }
}
