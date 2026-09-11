use anyhow::{Context, Result};

use crate::service::ClipboardService;

/// Desktop 端剪贴板服务 (arboard)
pub struct PlatformClipboardService {
    clipboard: std::sync::Mutex<arboard::Clipboard>,
}

impl PlatformClipboardService {
    pub fn new() -> Self {
        Self {
            clipboard: std::sync::Mutex::new(
                arboard::Clipboard::new()
                    .expect("初始化剪贴板失败 — 请确保当前环境支持剪贴板")
            ),
        }
    }
}

impl ClipboardService for PlatformClipboardService {
    fn get_text(&self) -> Result<String> {
        let mut clip = self.clipboard.lock().map_err(|e| anyhow::anyhow!("锁失败: {e}"))?;
        clip.get_text()
            .map(|s| {
                if s.is_empty() { Ok(String::new()) } else { Ok(s) }
            })
            .unwrap_or_else(|e| Err(anyhow::anyhow!("读取失败: {e}")))
            .context("从剪贴板读取文本")
    }

    fn set_text(&self, text: &str) -> Result<()> {
        let mut clip = self.clipboard.lock().map_err(|e| anyhow::anyhow!("锁失败: {e}"))?;
        clip.set_text(text.to_owned())
            .map_err(|e| anyhow::anyhow!("写入失败: {e}"))
            .context("写入文本到剪贴板")
    }
}
