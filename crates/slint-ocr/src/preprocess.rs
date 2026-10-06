//! 图像预处理：resize / RGB→NCHW 归一化（对齐 RapidOCR/PaddleOCR 管线）

use image::{DynamicImage, ImageBuffer, Rgb};
use image::imageops::FilterType;

/// 等比缩放到指定尺寸（RGB）
pub fn resize_rgb(img: &DynamicImage, w: u32, h: u32) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    let rgb = img.to_rgb8();
    image::imageops::resize(&rgb, w, h, FilterType::Triangle)
}

/// 等比缩放 ImageBuffer（RGB）
pub fn resize_rgb_buffer(img: &ImageBuffer<Rgb<u8>, Vec<u8>>, w: u32, h: u32) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    image::imageops::resize(img, w, h, FilterType::Triangle)
}

/// RGB8 → NCHW float32，归一化 (x/255 - mean) / std
pub fn rgb_to_nchw(img: &ImageBuffer<Rgb<u8>, Vec<u8>>, mean: [f32; 3], std: [f32; 3]) -> Vec<f32> {
    let (w, h) = img.dimensions();
    let mut out = vec![0f32; (3 * w * h) as usize];
    for (x, y, p) in img.enumerate_pixels() {
        for c in 0..3u32 {
            let v = p.0[c as usize] as f32 / 255.0;
            let idx = (c * h * w + y * w + x) as usize;
            out[idx] = (v - mean[c as usize]) / std[c as usize];
        }
    }
    out
}

/// det 输入尺寸（对齐 RapidOCR/PaddleOCR 配置）：
/// limit_type=min：短边不足 736 时放大到 736，长边不压缩；宽高对齐 32 的倍数
pub fn det_input_size(w: u32, h: u32, limit_side: u32) -> (u32, u32) {
    let ratio = if w.min(h) < limit_side {
        limit_side as f32 / w.min(h) as f32
    } else {
        1.0
    };
    let nw = ((w as f32 * ratio / 32.0).round() as u32).max(1) * 32;
    let nh = ((h as f32 * ratio / 32.0).round() as u32).max(1) * 32;
    (nw.max(32), nh.max(32))
}

/// rec 输入尺寸：高度固定（PP-OCRv4 为 48），宽度按宽高比缩放并对齐 4 的倍数，限制最大宽
pub fn rec_input_size(w: u32, h: u32, fixed_h: u32, max_w: u32) -> (u32, u32) {
    let ratio = fixed_h as f32 / h.max(1) as f32;
    let mut nw = ((w as f32 * ratio).round() as u32).max(4);
    nw = (nw / 4) * 4;
    (nw.min(max_w), fixed_h)
}

/// cls 输入尺寸（固定 192x48）
pub fn cls_input_size() -> (u32, u32) {
    (192, 48)
}
