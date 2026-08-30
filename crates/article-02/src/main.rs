// 第二幕: 地基与三端初始化 — desktop 入口

slint::include_modules!();

fn main() {
    #[cfg(feature = "desktop")]
    {
        let window = MainWindow::new().expect("创建窗口失败");
        window.show().expect("显示窗口失败");
        slint::run_event_loop().expect("事件循环异常");
    }
    #[cfg(not(feature = "desktop"))]
    {
        eprintln!("请使用 --features desktop 运行桌面端");
    }
}
