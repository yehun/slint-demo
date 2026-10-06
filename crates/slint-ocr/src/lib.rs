//! 本地 OCR 引擎：onnxruntime + PaddleOCR PP-OCRv4（det/cls/rec 三件套）
//!
//! - 推理运行时：直接依赖 `ort`（load-dynamic 桌面 / 编译期链接 Android），动态库加载/
//!   探测/会话构建策略收敛在 [`ort_ext`]，本 crate 不再依赖外部 `ort-onnx`。
//! - 模型：det/cls/rec 三个 .onnx（内存加载，`commit_from_memory`）+ keys.txt 字典
//! - 预处理/后处理参数对齐 RapidOCR 3.0.0 官方配置（ch_PP-OCRv4 模型）：
//!   det 短边 736（limit_type=min）32 对齐、归一化 (x/255-0.5)/0.5；
//!   rec 高 48 动态宽（≤640）、/255；cls 48x192、/255；RGB 通道序
//! - MVP 边界：检测框为轴对齐外接框（未做旋转框 + 透视矫正）

#![cfg(not(target_arch = "wasm32"))]

pub mod det;
pub mod ort_ext;
pub mod preprocess;
pub mod rec;

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{anyhow, Result};
use image::{DynamicImage, ImageBuffer, Rgb};
use ndarray::Dimension;
use ort_ext::ort::{session::Session, value::Tensor};
use ort_ext::{build_session_from_bytes, ensure, ort, Threads};
use parking_lot::Mutex;

pub const DET_LIMIT_SIDE: u32 = 1280;
pub const REC_FIXED_H: u32 = 48;
pub const REC_MAX_W: u32 = 640;
pub const CLS_ANGLE_THRESH: f32 = 0.9;

/// 单行识别结果（含检测框位置，原图坐标）
#[derive(Debug, Clone)]
pub struct OcrLine {
    pub text: String,
    pub confidence: f32,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// 识别结果与各阶段耗时（毫秒）
#[derive(Debug, Clone, Default)]
pub struct OcrResult {
    pub lines: Vec<OcrLine>,
    pub det_ms: u128,
    pub cls_ms: u128,
    pub rec_ms: u128,
    pub total_ms: u128,
}

struct LoadedModels {
    det: Session,
    rec: Session,
    cls: Option<Session>,
    keys: Vec<String>,
}

/// OCR 引擎：模型懒加载 + 串行推理（Session::run 需要 &mut，一次一张图）
pub struct OcrService {
    models: Mutex<Option<LoadedModels>>,
    library_error: Mutex<Option<String>>,
}

impl Default for OcrService {
    fn default() -> Self {
        Self::new()
    }
}

impl OcrService {
    pub fn new() -> Self {
        // 构造时探测并加载 onnxruntime 库（失败不 panic，识别时给出明确错误）
        let library_error = match ensure() {
            Ok(()) => None,
            Err(e) => Some(e),
        };
        Self {
            models: Mutex::new(None),
            library_error: Mutex::new(library_error),
        }
    }

    /// onnxruntime 库是否可用
    pub fn ort_available(&self) -> bool {
        self.library_error.lock().is_none()
    }

    /// 库加载失败原因（缺库/版本不匹配）
    pub fn library_error(&self) -> Option<String> {
        self.library_error.lock().clone()
    }

    /// 模型是否已加载
    pub fn models_loaded(&self) -> bool {
        self.models.lock().is_some()
    }

    /// 释放模型（销毁所有推理会话，释放内存）
    pub fn unload_models(&self) {
        *self.models.lock() = None;
        log::info!("OCR 模型已释放");
    }

    /// 从内存字节加载模型：det/rec 必选，cls/keys 可选（不落盘）
    pub fn load_models(
        &self,
        det_bytes: &[u8],
        cls_bytes: Option<&[u8]>,
        rec_bytes: &[u8],
        keys_bytes: Option<&[u8]>,
    ) -> Result<()> {
        ensure().map_err(anyhow::Error::msg)?;
        let det = build_session_from_bytes(det_bytes, Threads::Auto)?;
        let rec = build_session_from_bytes(rec_bytes, Threads::Serial)?;
        let cls = cls_bytes
            .map(|b| build_session_from_bytes(b, Threads::Serial))
            .transpose()?;
        let keys = keys_bytes.map(rec::load_keys).unwrap_or_default();
        let keys_len = keys.len();
        *self.models.lock() = Some(LoadedModels { det, rec, cls, keys });
        log::info!(
            "OCR 模型加载完成: det={}MB rec={}MB cls={} keys={}chars",
            det_bytes.len() / 1024 / 1024,
            rec_bytes.len() / 1024 / 1024,
            cls_bytes.map_or(0, |b| b.len() / 1024 / 1024),
            keys_len
        );
        Ok(())
    }

    /// 从文件加载模型（det/rec 必选，cls/keys 可选；传 None 表示跳过该部件）
    pub fn load_models_from_files(
        &self,
        det: &Path,
        cls: Option<&Path>,
        rec: &Path,
        keys: Option<&Path>,
    ) -> Result<()> {
        let det_bytes = std::fs::read(det)
            .map_err(|e| anyhow!("读取 det 模型失败 ({}): {e}", det.display()))?;
        let rec_bytes = std::fs::read(rec)
            .map_err(|e| anyhow!("读取 rec 模型失败 ({}): {e}", rec.display()))?;
        let cls_bytes = match cls {
            Some(p) => Some(
                std::fs::read(p)
                    .map_err(|e| anyhow!("读取 cls 模型失败 ({}): {e}", p.display()))?,
            ),
            None => None,
        };
        let keys_bytes = match keys {
            Some(p) => Some(
                std::fs::read(p)
                    .map_err(|e| anyhow!("读取 keys 字典失败 ({}): {e}", p.display()))?,
            ),
            None => None,
        };
        self.load_models(&det_bytes, cls_bytes.as_deref(), &rec_bytes, keys_bytes.as_deref())
    }

    /// 扫描目录并加载模型：文件名优先 det.onnx/cls.onnx/rec.onnx/keys.txt，
    /// 否则按关键词模糊匹配（兼容 RapidOCR 长文件名）
    pub fn load_models_from_dir(&self, dir: &Path) -> Result<()> {
        let (det, cls, rec, keys) = scan_model_dir(dir);
        let det = det.ok_or_else(|| anyhow!("目录 {} 缺少 det 模型(det.onnx)", dir.display()))?;
        let rec = rec.ok_or_else(|| anyhow!("目录 {} 缺少 rec 模型(rec.onnx)", dir.display()))?;
        self.load_models_from_files(&det, cls.as_deref(), &rec, keys.as_deref())
    }

    /// 识别一张图
    pub fn recognize(&self, img: &DynamicImage) -> Result<OcrResult> {
        let mut models = self.models.lock();
        let m = models
            .as_mut()
            .ok_or_else(|| anyhow!("OCR 模型未加载，请先选择 det/rec 模型文件"))?;
        recognize_inner(m, img)
    }
}

/// 扫描模型目录：返回 (det, cls, rec, keys) 文件路径
pub fn scan_model_dir(dir: &Path) -> (Option<PathBuf>, Option<PathBuf>, Option<PathBuf>, Option<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return (None, None, None, None),
    };
    let files: Vec<String> = entries
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
        .collect();

    let find = |exact: &[&str], keywords: &[&str], ext: &str| -> Option<PathBuf> {
        for name in exact {
            if files.iter().any(|f| f == name) {
                return Some(dir.join(name));
            }
        }
        files.iter().find_map(|f| {
            let lower = f.to_lowercase();
            if lower.ends_with(ext) && keywords.iter().any(|k| lower.contains(k)) {
                Some(dir.join(f))
            } else {
                None
            }
        })
    };

    let det = find(&["det.onnx"], &["det"], ".onnx");
    let cls = find(&["cls.onnx"], &["cls"], ".onnx");
    let rec = find(&["rec.onnx"], &["rec"], ".onnx");
    let keys = find(&["keys.txt"], &["keys"], ".txt");
    (det, cls, rec, keys)
}

fn recognize_inner(m: &mut LoadedModels, img: &DynamicImage) -> Result<OcrResult> {
    let t0 = Instant::now();
    let rgb = img.to_rgb8();
    let (img_w, img_h) = rgb.dimensions();

    // ---- 文本检测 ----
    let (dw, dh) = preprocess::det_input_size(img_w, img_h, DET_LIMIT_SIDE);
    let det_img = preprocess::resize_rgb(img, dw, dh);
    let det_data = preprocess::rgb_to_nchw(&det_img, [0.5, 0.5, 0.5], [0.5, 0.5, 0.5]);
    let prob = run_det(&mut m.det, &det_data, dw, dh)?;
    let det_ms = t0.elapsed().as_millis();

    let boxes = det::db_postprocess(&prob, dw, dh, img_w, img_h);
    if boxes.is_empty() {
        return Ok(OcrResult {
            det_ms,
            total_ms: t0.elapsed().as_millis(),
            ..Default::default()
        });
    }

    // ---- 逐框：cls（可选）+ rec ----
    let t1 = Instant::now();
    let mut lines = Vec::with_capacity(boxes.len());
    for b in &boxes {
        // 旋转框透视矫正裁剪（倾斜文本拉正后再识别）
        let crop = warp_crop(&rgb, b, 4);
        let crop = match &mut m.cls {
            Some(cls) => {
                let need_rotate = run_cls(cls, &crop)?;
                if need_rotate {
                    image::imageops::rotate180(&crop)
                } else {
                    crop
                }
            }
            None => crop,
        };
        if let Some((text, confidence)) = run_rec(&mut m.rec, &crop, &m.keys)? {
            lines.push(OcrLine {
                text,
                confidence,
                x: b.x,
                y: b.y,
                w: b.w,
                h: b.h,
            });
        }
    }
    let rec_ms = t1.elapsed().as_millis();

    // 按阅读顺序排序：行聚类（y 差在阈值内视为同一行），行内从左到右，行间从上到下。
    // 同一行文字框的 y 常有几像素偏差，直接按 y 排序会把一行拆散。
    sort_lines_by_position(&mut lines);
    Ok(OcrResult {
        lines,
        det_ms,
        cls_ms: 0,
        rec_ms,
        total_ms: t0.elapsed().as_millis(),
    })
}

/// 运行 det：输入 [1,3,H,W] → 输出 [1,1,H,W] 概率图
fn run_det(session: &mut Session, data: &[f32], w: u32, h: u32) -> Result<Vec<f32>> {
    let arr = ndarray::Array4::from_shape_vec((1, 3, h as usize, w as usize), data.to_vec())?;
    let tensor = Tensor::from_array(arr)?;
    let name = session.inputs()[0].name().to_string();
    let outputs = session.run(ort::inputs![name => tensor])?;
    let (_shape, data) = outputs[0].try_extract_tensor::<f32>()?;
    Ok(data.to_vec())
}

/// 运行 cls：输入 [1,3,48,192] → 输出 [1,2]，返回是否需要旋转 180°
fn run_cls(session: &mut Session, crop: &ImageBuffer<Rgb<u8>, Vec<u8>>) -> Result<bool> {
    let (cw, ch) = preprocess::cls_input_size();
    let img = preprocess::resize_rgb_buffer(crop, cw, ch);
    let data = preprocess::rgb_to_nchw(&img, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let arr = ndarray::Array4::from_shape_vec((1, 3, ch as usize, cw as usize), data)?;
    let tensor = Tensor::from_array(arr)?;
    let name = session.inputs()[0].name().to_string();
    let outputs = session.run(ort::inputs![name => tensor])?;
    let (_shape, data) = outputs[0].try_extract_tensor::<f32>()?;
    let p_rotate = data.get(1).copied().unwrap_or(0.0);
    Ok(p_rotate > CLS_ANGLE_THRESH)
}

/// 运行 rec：输入 [1,3,48,W] → 输出 [1,T,C]，CTC 解码后返回 (文本, 置信度)
fn run_rec(
    session: &mut Session,
    crop: &ImageBuffer<Rgb<u8>, Vec<u8>>,
    keys: &[String],
) -> Result<Option<(String, f32)>> {
    let (cw, ch) = crop.dimensions();
    if cw == 0 || ch == 0 {
        return Ok(None);
    }
    let (rw, rh) = preprocess::rec_input_size(cw, ch, REC_FIXED_H, REC_MAX_W);
    let img = preprocess::resize_rgb_buffer(crop, rw, rh);
    let data = preprocess::rgb_to_nchw(&img, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let arr = ndarray::Array4::from_shape_vec((1, 3, rh as usize, rw as usize), data)?;
    let tensor = Tensor::from_array(arr)?;
    let name = session.inputs()[0].name().to_string();
    let outputs = session.run(ort::inputs![name => tensor])?;
    let (shape, data) = outputs[0].try_extract_tensor::<f32>()?;
    let dims = shape.to_ixdyn();
    if dims.ndim() != 3 {
        return Ok(None);
    }
    let t = dims[1];
    let c = dims[2];
    let (text, confidence) = rec::decode_ctc(data, t, c, keys);
    if text.is_empty() {
        return Ok(None);
    }
    Ok(Some((text, confidence)))
}

/// 旋转框透视矫正裁剪：把检测框（PCA 旋转外接矩形）按主方向拉正为轴对齐图，
/// 带 padding 防切字；倾斜文本拉正后识别精度大幅提升。
fn warp_crop(
    rgb: &ImageBuffer<Rgb<u8>, Vec<u8>>,
    b: &det::DetBox,
    pad: u32,
) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    let (src_w, src_h) = rgb.dimensions();
    let out_w = (b.rw + pad as f32 * 2.0).ceil() as u32;
    let out_h = (b.rh + pad as f32 * 2.0).ceil() as u32;
    let mut out = ImageBuffer::new(out_w.max(1), out_h.max(1));

    let cos = b.angle.cos();
    let sin = b.angle.sin();
    // 目标坐标系原点在旋转框中心；目标点 (u, v) → 源点 = R(-angle)·(u,v) + 中心
    let u0 = out_w as f32 / 2.0 - b.rw / 2.0;
    let v0 = out_h as f32 / 2.0 - b.rh / 2.0;
    for y in 0..out_h {
        for x in 0..out_w {
            let u = x as f32 - u0 - b.rw / 2.0;
            let v = y as f32 - v0 - b.rh / 2.0;
            let sx = b.cx + u * cos - v * sin;
            let sy = b.cy + u * sin + v * cos;
            out.put_pixel(x, y, sample_bilinear(rgb, sx, sy, src_w, src_h));
        }
    }
    out
}

/// 双线性采样（越界返回最近邻边缘像素，避免黑边干扰识别）
fn sample_bilinear(
    rgb: &ImageBuffer<Rgb<u8>, Vec<u8>>,
    sx: f32,
    sy: f32,
    src_w: u32,
    src_h: u32,
) -> Rgb<u8> {
    let x = sx.clamp(0.0, (src_w - 1) as f32);
    let y = sy.clamp(0.0, (src_h - 1) as f32);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(src_w - 1);
    let y1 = (y0 + 1).min(src_h - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;

    let p00 = rgb.get_pixel(x0, y0).0;
    let p10 = rgb.get_pixel(x1, y0).0;
    let p01 = rgb.get_pixel(x0, y1).0;
    let p11 = rgb.get_pixel(x1, y1).0;
    Rgb([
        ((p00[0] as f32 * (1.0 - fx) + p10[0] as f32 * fx) * (1.0 - fy)
            + (p01[0] as f32 * (1.0 - fx) + p11[0] as f32 * fx) * fy) as u8,
        ((p00[1] as f32 * (1.0 - fx) + p10[1] as f32 * fx) * (1.0 - fy)
            + (p01[1] as f32 * (1.0 - fx) + p11[1] as f32 * fx) * fy) as u8,
        ((p00[2] as f32 * (1.0 - fx) + p10[2] as f32 * fx) * (1.0 - fy)
            + (p01[2] as f32 * (1.0 - fx) + p11[2] as f32 * fx) * fy) as u8,
    ])
}

/// 按位置排序：行聚类（y 差 ≤ 行高一半视为同一行），行内按 x，行间按 y
fn sort_lines_by_position(lines: &mut Vec<OcrLine>) {
    if lines.len() <= 1 {
        return;
    }
    lines.sort_by(|a, b| a.y.cmp(&b.y));
    let mut row_start = 0usize;
    for i in 1..=lines.len() {
        let new_row = if i == lines.len() {
            true
        } else {
            let threshold = (lines[row_start].h / 2).max(8);
            (lines[i].y as i32 - lines[row_start].y as i32).abs() > threshold as i32
        };
        if new_row {
            // 同一行（[row_start, i)）按 x 从左到右
            lines[row_start..i].sort_by(|a, b| a.x.cmp(&b.x));
            row_start = i;
        }
    }
}
