use anyhow::Result;

/// 剪贴板操作结果
#[derive(Debug, Clone, PartialEq)]
pub enum ClipboardResult {
    Success,
    Empty,
    Error(String),
}

impl std::fmt::Display for ClipboardResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClipboardResult::Success => write!(f, "操作成功"),
            ClipboardResult::Empty => write!(f, "剪贴板为空"),
            ClipboardResult::Error(e) => write!(f, "{e}"),
        }
    }
}

/// 平台无关的剪贴板服务 trait
pub trait ClipboardService: Send + Sync {
    /// 读取剪贴板文本
    fn get_text(&self) -> Result<String>;

    /// 写入文本到剪贴板
    fn set_text(&self, text: &str) -> Result<()>;
}
