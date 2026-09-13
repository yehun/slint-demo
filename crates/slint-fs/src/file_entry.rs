use mime_type::MimeType;

use crate::PlatformPath;


#[derive(Debug)]
pub struct PlatformFileEntry {
    pub path: PlatformPath,
    pub name: String,
    pub size: u64,
    pub mime_type: Option<MimeType>,
    pub volume: Option<String>
}
