// 免 GUI 生成一张 300dpi 排版样张:
//
//   cargo run -p article-20 --features desktop --example make-sample -- /tmp/id-photo-6in.png
//
// 不挑文件、不弹对话框 —— 合成一张蓝底"测试照片", 直接走 core::compose_sheet
// 排满 6 寸纸并落盘, 用来肉眼核对排版公式(1寸 4×4=16 张, 居中)。

use article_20::core::compose_sheet;
use image::RgbaImage;

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "id-photo-6in.png".into());

    // 合成一张 5:7 的"测试照片": 蓝底 + 肤色头肩轮廓(足够看出比例与方向)
    // (不依赖 image 的 draw 系列 API, 直接手写像素循环 —— 圆方程判定)
    let mut photo = RgbaImage::from_pixel(500, 700, image::Rgba([64, 125, 200, 255]));
    let skin = image::Rgba([235, 195, 160, 255]);
    let in_head = |x: u32, y: u32| {
        let (dx, dy) = (x as i64 - 250, y as i64 - 270);
        dx * dx + dy * dy <= 135 * 135
    };
    let in_shoulder = |x: u32, y: u32| y >= 500 && x >= 60 && x < 440;
    for (x, y, p) in photo.enumerate_pixels_mut() {
        if in_head(x, y) || in_shoulder(x, y) {
            *p = skin;
        }
    }

    let sheet = compose_sheet(&photo, 0, 0, false, 0).expect("compose failed");
    sheet.save(&out).expect("save failed");
    println!("{} ({}×{} px)", out, sheet.width(), sheet.height());
}
