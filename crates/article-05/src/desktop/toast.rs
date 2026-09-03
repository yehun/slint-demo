// Desktop Toast 模拟

/// 模拟弹出 Toast (Desktop 端打印到 stderr)
pub fn show_toast(message: &str) -> String {
    eprintln!("[Toast模拟] {}", message);
    format!("Toast 模拟: {}", message)
}
