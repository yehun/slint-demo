/// 水平翻转 RGB 像素数据 (镜像效果)
///
/// 摄像头采集的画面是"别人看你的视角", 预览时用户习惯看到"镜子里的自己",
/// 所以需要水平翻转.
pub fn flip_rgb_horizontal(data: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut flipped = Vec::with_capacity(data.len());
    let width_usize = width as usize;
    let row_size = width_usize * 3;

    for y in 0..height as usize {
        let row_start = y * row_size;
        for x in (0..width_usize).rev() {
            let pixel_start = row_start + (x * 3);
            flipped.push(data[pixel_start]); // R
            flipped.push(data[pixel_start + 1]); // G
            flipped.push(data[pixel_start + 2]); // B
        }
    }

    flipped
}
