//! DB（Differentiable Binarization）检测后处理
//! 对齐 RapidOCR 3.0.0 DBPostProcess 配置：thresh=0.3, box_thresh=0.5,
//! use_dilation=true（2x2 全 1 核），min_size=3

/// 检测框（原图坐标）。
/// x/y/w/h 为轴对齐外接框（显示用）；cx/cy/rw/rh/angle 为旋转外接矩形（PCA 主方向，识别裁剪用）
#[derive(Debug, Clone)]
pub struct DetBox {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub cx: f32,
    pub cy: f32,
    pub rw: f32,
    pub rh: f32,
    pub angle: f32,
}

const DB_THRESH: f32 = 0.3;
const BOX_THRESH: f32 = 0.5;
const MIN_SIZE: u32 = 3;

/// 概率图 → 检测框列表（映射回原图坐标）
///
/// `prob` 为 det 输出（已含 sigmoid，[H*W] 行主序），`prob_w/prob_h` 为其尺寸，
/// `img_w/img_h` 为原图尺寸（用于坐标回映）。
pub fn db_postprocess(
    prob: &[f32],
    prob_w: u32,
    prob_h: u32,
    img_w: u32,
    img_h: u32,
) -> Vec<DetBox> {
    let expected = (prob_w as usize).saturating_mul(prob_h as usize);
    if prob.len() != expected {
        // 模型输出形状与输入不一致(换错模型/onnx 输出被降采样): 无法解读概率图,
        // 返回空结果而非越界写/崩溃
        log::warn!(
            "det 输出形状不匹配: prob.len()={} 期望 {}x{}={}, 本次识别返回空",
            prob.len(),
            prob_w,
            prob_h,
            expected
        );
        return vec![];
    }
    // 1. 二值化
    let bin = binarize(prob, prob_w, prob_h, DB_THRESH);
    // 2. 3x3 膨胀 ×2（近似 unclip_ratio=1.6 的框外扩，确保框圈住文字）
    let dil = dilate3x3(&bin, prob_w, prob_h);
    let dil = dilate3x3(&dil, prob_w, prob_h);
    // 3. 连通域 → 外接框（过滤过小/低分区域）
    let boxes = connected_bboxes(prob, &dil, prob_w, prob_h, MIN_SIZE, BOX_THRESH);
    if boxes.is_empty() {
        return vec![];
    }
    // 4. 回映原图坐标（det 输入按比例缩放，比例 = 输入图宽/原图宽）
    let scale_x = img_w as f32 / prob_w as f32;
    let scale_y = img_h as f32 / prob_h as f32;
    boxes
        .into_iter()
        .map(|b| DetBox {
            x: (b.x as f32 * scale_x).floor() as u32,
            y: (b.y as f32 * scale_y).floor() as u32,
            w: ((b.w as f32 * scale_x).ceil() as u32).max(1),
            h: ((b.h as f32 * scale_y).ceil() as u32).max(1),
            cx: b.cx * scale_x,
            cy: b.cy * scale_y,
            rw: b.rw * scale_x,
            rh: b.rh * scale_y,
            angle: b.angle,
        })
        .collect()
}

/// 概率图 → 0/255 二值图
fn binarize(prob: &[f32], w: u32, h: u32, thresh: f32) -> Vec<u8> {
    let mut bin = vec![0u8; (w * h) as usize];
    // 按 bin 长度截断, 双保险防输出形状异常时越界写
    for (i, v) in prob.iter().enumerate().take(bin.len()) {
        if *v > thresh {
            bin[i] = 255;
        }
    }
    bin
}

/// 3x3 全 1 核膨胀
fn dilate3x3(bin: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let mut v = 0u8;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && ny >= 0 && nx < w as i32 && ny < h as i32
                        && bin[(ny as u32 * w + nx as u32) as usize] > 0
                    {
                        v = 255;
                    }
                }
            }
            out[(y * w + x) as usize] = v;
        }
    }
    out
}

/// 连通域 → 检测框（8 邻域 BFS）。
/// 过滤：面积 < `min_size` 或框内平均概率 < `box_thresh`（score_mode=fast 近似）。
/// 每个连通域：轴对齐外接框（显示）+ PCA 主方向旋转外接矩形（倾斜文本识别裁剪用）。
fn connected_bboxes(
    prob: &[f32],
    bin: &[u8],
    w: u32,
    h: u32,
    min_size: u32,
    box_thresh: f32,
) -> Vec<DetBox> {
    let total = (w * h) as usize;
    let mut visited = vec![false; total];
    let mut boxes = Vec::new();
    for start in 0..total {
        if bin[start] == 0 || visited[start] {
            continue;
        }
        // BFS 收集同一 label 的所有像素
        let mut queue = vec![start];
        visited[start] = true;
        let mut pixels: Vec<(u32, u32)> = Vec::new();
        let mut min_x = w;
        let mut min_y = h;
        let mut max_x = 0u32;
        let mut max_y = 0u32;
        let mut area = 0u32;
        let mut score_sum = 0f32;
        while let Some(idx) = queue.pop() {
            let x = idx as u32 % w;
            let y = idx as u32 / w;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            area += 1;
            score_sum += prob[idx];
            pixels.push((x, y));
            for (dx, dy) in [
                (-1i32, -1i32),
                (-1, 0),
                (-1, 1),
                (0, -1),
                (0, 1),
                (1, -1),
                (1, 0),
                (1, 1),
            ] {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx >= 0 && ny >= 0 && nx < w as i32 && ny < h as i32 {
                    let ni = (ny as u32 * w + nx as u32) as usize;
                    if !visited[ni] && bin[ni] > 0 {
                        visited[ni] = true;
                        queue.push(ni);
                    }
                }
            }
        }
        let mean_score = score_sum / area as f32;
        if area >= min_size && mean_score > box_thresh {
            let (cx, cy, rw, rh, angle) = min_area_rect_pca(&pixels);
            boxes.push(DetBox {
                x: min_x,
                y: min_y,
                w: max_x - min_x + 1,
                h: max_y - min_y + 1,
                cx,
                cy,
                rw,
                rh,
                angle,
            });
        }
    }
    boxes
}

/// 用 PCA 计算像素集合的旋转外接矩形（中心、宽高、主方向角）
/// 文本行是细长形，协方差主方向即文字方向
fn min_area_rect_pca(pixels: &[(u32, u32)]) -> (f32, f32, f32, f32, f32) {
    let n = pixels.len() as f32;
    if n == 0.0 {
        return (0.0, 0.0, 0.0, 0.0, 0.0);
    }
    let mut mx = 0f32;
    let mut my = 0f32;
    for (x, y) in pixels {
        mx += *x as f32;
        my += *y as f32;
    }
    mx /= n;
    my /= n;

    let mut sxx = 0f32;
    let mut syy = 0f32;
    let mut sxy = 0f32;
    for (x, y) in pixels {
        let dx = *x as f32 - mx;
        let dy = *y as f32 - my;
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    // 协方差矩阵 [sxx sxy; sxy syy] 的主特征方向
    let theta = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let cos = theta.cos();
    let sin = theta.sin();

    // 沿主方向投影求宽高
    let mut min_u = f32::MAX;
    let mut max_u = f32::MIN;
    let mut min_v = f32::MAX;
    let mut max_v = f32::MIN;
    for (x, y) in pixels {
        let dx = *x as f32 - mx;
        let dy = *y as f32 - my;
        let u = dx * cos + dy * sin;
        let v = -dx * sin + dy * cos;
        min_u = min_u.min(u);
        max_u = max_u.max(u);
        min_v = min_v.min(v);
        max_v = max_v.max(v);
    }
    let rw = (max_u - min_u).max(1.0);
    let rh = (max_v - min_v).max(1.0);
    (mx, my, rw, rh, theta)
}
