// 第二十篇, 证件照排版 —— 核心排版/裁剪/合成算法
//
// 三块纯逻辑, 与 UI(app.slint 的 IdPhotoModel 纯派生函数)同一套语义:
// 1. SheetLayout::compute —— 打印纸排版参数
//    footprint = 照片mm + 两侧边框mm; 横竖两种朝向各按
//    floor(纸宽/footW) × floor(纸高/footH) 取块数, 块数多者胜(最大化利用),
//    网格在打印纸上居中。
// 2. crop_ratio —— 归一化(0..1)选框 → 像素钳位裁剪, 保证照片纵横比 = 目标比例
// 3. compose_sheet —— 300dpi 合成: 照片 Lanczos3 缩放到目标像素 →
//    白底块内嵌照片(边框 = 裁切留白环) → 逐块铺上打印纸 → 返回整张 RgbaImage
//
// 尺寸约定(mm): 1寸 = 25×35, 2寸 = 35×49; 6寸纸 = 102×152, 7寸纸 = 127×178。

use image::RgbaImage;

/// 300dpi 换算系数(像素/毫米): 300 px / 25.4 mm ≈ 11.811 px/mm
pub const MM_TO_PX: f64 = 300.0 / 25.4;

/// 目标照片(mm): 0 = 1寸 25×35, 1 = 2寸 35×49
pub fn photo_mm(target: i32) -> (f64, f64) {
    if target == 0 { (25.0, 35.0) } else { (35.0, 49.0) }
}

/// 打印纸(mm, 纵向 宽×高): 0 = 6寸 102×152, 1 = 7寸 127×178
pub fn paper_mm(paper: i32) -> (f64, f64) {
    if paper == 0 { (102.0, 152.0) } else { (127.0, 178.0) }
}

/// 一张打印纸的排版参数(全部由 compute 派生, UI 预览与导出共用同一套落点公式)
#[derive(Debug, Clone, Copy)]
pub struct SheetLayout {
    /// 照片 mm
    pub qw: f64,
    pub qh: f64,
    /// 打印纸 mm
    pub pw: f64,
    pub ph: f64,
    /// 单侧边框 mm(关闭 = 0)
    pub b: f64,
    /// 单块 footprint = 照片 + 两侧边框
    pub fw: f64,
    pub fh: f64,
    /// 胜出朝向的行列块数
    pub cols: f64,
    pub rows: f64,
    /// 是否旋转 90° 铺放(横放块数更多时为 true)
    pub rotated: bool,
    /// 单块落纸尺寸(旋转后宽高互换)
    pub tw: f64,
    pub th: f64,
    /// 网格整体在纸上的居中偏移
    pub off_x: f64,
    pub off_y: f64,
}

impl SheetLayout {
    pub fn compute(target: i32, paper: i32, border_enabled: bool, border_mm: i32) -> Self {
        let (qw, qh) = photo_mm(target);
        let (pw, ph) = paper_mm(paper);
        let b = if border_enabled { border_mm.clamp(0, 20) as f64 } else { 0.0 };
        let (fw, fh) = (qw + 2.0 * b, qh + 2.0 * b);

        // 两种朝向各算块数, 取多者(同数取 A —— 减少无谓旋转)
        let cols_a = (pw / fw).floor();
        let rows_a = (ph / fh).floor();
        // 朝向 B: 照片横放(旋转 90°), footprint 宽高互换
        let cols_b = (pw / fh).floor();
        let rows_b = (ph / fw).floor();
        let rotated = cols_b * rows_b > cols_a * rows_a;
        let (cols, rows) = if rotated { (cols_b, rows_b) } else { (cols_a, rows_a) };
        let (tw, th) = if rotated { (fh, fw) } else { (fw, fh) };
        let off_x = (pw - cols * tw) / 2.0;
        let off_y = (ph - rows * th) / 2.0;

        Self { qw, qh, pw, ph, b, fw, fh, cols, rows, rotated, tw, th, off_x, off_y }
    }
}

/// 按归一化左上角 + 归一化宽高裁出目标比例照片。
/// 宽高钳在 [1, 整图], 位置钳进整图 —— 侧防 UI 传来的非法值。
pub fn crop_ratio(img: &RgbaImage, nx: f32, ny: f32, nw: f32, nh: f32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let cw_f = if nw.is_finite() { nw * w as f32 } else { 0.0 };
    let ch_f = if nh.is_finite() { nh * h as f32 } else { 0.0 };
    let cw = (cw_f.round().max(1.0).min(w as f32)) as u32;
    let ch = (ch_f.round().max(1.0).min(h as f32)) as u32;
    let x = ((nx * w as f32).round().max(0.0) as u32).min(w - cw);
    let y = ((ny * h as f32).round().max(0.0) as u32).min(h - ch);
    image::imageops::crop_imm(img, x, y, cw, ch).to_image()
}

/// 300dpi 合成整张打印纸:
/// 照片缩放到目标像素(1寸 295×413 / 2寸 413×579) → 白底块内嵌照片
/// (边框 = 照片四周的白环, 关闭时块即照片) → 按排版公式逐块铺满纸面
/// (旋转朝向整块 rotate90)。纸太小放不下一块时返回 Err。
pub fn compose_sheet(
    img: &RgbaImage,
    target: i32,
    paper: i32,
    border_enabled: bool,
    border_mm: i32,
) -> Result<RgbaImage, String> {
    let lay = SheetLayout::compute(target, paper, border_enabled, border_mm);
    if lay.cols < 1.0 || lay.rows < 1.0 {
        return Err("纸太小, 一块都放不下".into());
    }

    let qw_px = (lay.qw * MM_TO_PX).round().max(1.0) as u32;
    let qh_px = (lay.qh * MM_TO_PX).round().max(1.0) as u32;
    let b_px = (lay.b * MM_TO_PX).round() as i64;
    let paper_w_px = (lay.pw * MM_TO_PX).round() as u32;
    let paper_h_px = (lay.ph * MM_TO_PX).round() as u32;

    // 照片缩放到目标尺寸(裁剪后纵横比与目标一致, 仅有像素取整级偏差)
    let photo = image::imageops::resize(img, qw_px, qh_px, image::imageops::FilterType::Lanczos3);

    // 单块 = 白底 + 居中照片(边框 = 照片四周的白环; 关闭时 b=0, 块即照片)
    let tile_w_px = (lay.fw * MM_TO_PX).round() as u32;
    let tile_h_px = (lay.fh * MM_TO_PX).round() as u32;
    let mut tile = RgbaImage::from_pixel(tile_w_px, tile_h_px, image::Rgba([255u8, 255, 255, 255]));
    image::imageops::overlay(&mut tile, &photo, b_px, b_px);
    // 旋转朝向: 整块转 90°(块宽高互换), 铺纸位置按 tw/th 落点
    let tile = if lay.rotated { image::imageops::rotate90(&tile) } else { tile };

    // 打印纸画布(白底) + 逐块铺放
    let mut sheet = RgbaImage::from_pixel(paper_w_px, paper_h_px, image::Rgba([255u8, 255, 255, 255]));
    for r in 0..lay.rows as u32 {
        for c in 0..lay.cols as u32 {
            let x = ((lay.off_x + c as f64 * lay.tw) * MM_TO_PX).round() as i64;
            let y = ((lay.off_y + r as f64 * lay.th) * MM_TO_PX).round() as i64;
            image::imageops::overlay(&mut sheet, &tile, x, y);
        }
    }
    Ok(sheet)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_1in_on_6in() {
        // 1寸 25×35 上 6寸纸 102×152: 直立 4×4=16 胜过横放 2×6=12
        let lay = SheetLayout::compute(0, 0, false, 0);
        assert!(!lay.rotated);
        assert_eq!(lay.cols as i32, 4);
        assert_eq!(lay.rows as i32, 4);
        // 网格居中: (102 - 4×25) / 2 = 1mm
        assert!((lay.off_x - 1.0).abs() < 1e-9);
    }

    #[test]
    fn layout_2in_on_6in_rotates() {
        // 2寸 35×49 上 6寸纸: 横放 2×4=8 胜过直立 2×3=6 → 旋转
        let lay = SheetLayout::compute(1, 0, false, 0);
        assert!(lay.rotated);
        assert_eq!(lay.cols as i32, 2);
        assert_eq!(lay.rows as i32, 4);
    }

    #[test]
    fn border_shrinks_grid() {
        // 1寸 + 两侧各 3mm 边框 = footprint 31×41: 直立 3×3=9, 横放 2×3=6
        let lay = SheetLayout::compute(0, 0, true, 3);
        assert!(!lay.rotated);
        assert_eq!(lay.cols as i32 * lay.rows as i32, 9);
        // 关闭边框时不参与 footprint
        let off = SheetLayout::compute(0, 0, false, 3);
        assert_eq!((off.fw, off.fh), (25.0, 35.0));
    }

    #[test]
    fn compose_dimensions_and_pixel_size() {
        // 300dpi 换算: 25mm → 295px, 35mm → 413px, 102×152mm 纸 → 1205×1795px
        let lay = SheetLayout::compute(0, 0, false, 0);
        assert_eq!((lay.qw * MM_TO_PX).round() as u32, 295);
        assert_eq!((lay.qh * MM_TO_PX).round() as u32, 413);
        assert_eq!((lay.pw * MM_TO_PX).round() as u32, 1205);
        assert_eq!((lay.ph * MM_TO_PX).round() as u32, 1795);

        // 合成: 随便一张 5:7 的图, 出来必须是整纸尺寸
        let img = RgbaImage::from_pixel(250, 350, image::Rgba([200, 30, 30, 255]));
        let sheet = compose_sheet(&img, 0, 0, false, 0).unwrap();
        assert_eq!(sheet.dimensions(), (1205, 1795));
    }

    #[test]
    fn crop_ratio_clamps() {
        let img = RgbaImage::from_pixel(100, 100, image::Rgba([0, 0, 0, 255]));
        // 越界归一化值被钳进整图
        let cropped = crop_ratio(&img, -0.5, -0.5, 2.0, 2.0);
        assert_eq!(cropped.dimensions(), (100, 100));
        // 正常选框: 宽高按归一化换算
        let cropped = crop_ratio(&img, 0.1, 0.2, 0.5, 0.5);
        assert_eq!(cropped.dimensions(), (50, 50));
    }
}
