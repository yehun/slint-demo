use mime_type::{MimeFormat, MimeType};

use crate::error::{Error, Result};
use crate::{PlatformFile, PlatformFileEntry, PlatformFileFormat, PlatformFileMode};
use std::{fmt, fs, path::{Path, PathBuf}};

/// 统一路径的两种形态:
/// Local = 桌面/通用文件系统路径; Content = Android MediaStore content URI.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PathKind {
    Local(PathBuf),
    #[cfg(target_os = "android")]
    Content(String),
}

/// 统一路径. 本地路径走 std::fs, Android content URI 走 JNI ContentResolver,
/// 调用方无感知、无 cfg 分支.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlatformPath {
    pub(crate) kind: PathKind,
}

impl From<&Path> for PlatformPath {
    fn from(path: &Path) -> Self {
        Self {
            kind: PathKind::Local(path.to_path_buf()),
        }
    }
}

impl From<PathBuf> for PlatformPath {
    fn from(path: PathBuf) -> Self {
        Self {
            kind: PathKind::Local(path),
        }
    }
}

impl From<PathKind> for PlatformPath {
    fn from(value: PathKind) -> Self {
        Self {
            kind: value
        }
    }
}

impl PlatformPath {
    /// 从字符串解析: content:// 走 Content 模式 (非 Android 平台回退为本地路径并告警),
    /// file:// 前缀剥掉, 其余按本地路径处理.
    pub fn new(s: impl AsRef<str>) -> Self {
        let s = s.as_ref();
        let kind = if s.starts_with("content://") {
            #[cfg(target_os = "android")]
            {
                PathKind::Content(s.to_string())
            }
            #[cfg(not(target_os = "android"))]
            {
                log::warn!("Content URI '{}' not supported on this platform, treating as local path", s);
                let s = s.strip_prefix("file://").unwrap_or(s);
                PathKind::Local(PathBuf::from(s))
            }
        } else {
            let s = s.strip_prefix("file://").unwrap_or(s);
            PathKind::Local(PathBuf::from(s))
        };
        Self { kind }
    }

    pub fn from_local(path: impl Into<PathBuf>) -> Self {
        Self {
            kind: PathKind::Local(path.into()),
        }
    }

    /// 在 base_path 下创建文件 (目录不存在则先建).
    /// Android 走 MediaStore insert, 返回 content URI.
    pub fn create_file(
        base_path: &PlatformPath,
        relative_path: &str,
        file_name: &str,
        #[cfg_attr(not(target_os = "android"), allow(unused_variables))]
        file_format: PlatformFileFormat
    ) -> Result<Self> {
        #[cfg(not(target_os = "android"))]
        {
            let base_path = PathBuf::from(base_path.to_string()).join(relative_path);
            if !base_path.exists() {
                fs::create_dir_all(&base_path)?;
            }
            let file_path = base_path.join(file_name);
            let kind = PathKind::Local(file_path);
            Ok(PlatformPath::from(kind))
        }
        #[cfg(target_os = "android")]
        {
            let uri = crate::android::jni_content_create_uri(
                &base_path.to_string(),
                relative_path,
                file_name,
                file_format
            )?;
            let kind = PathKind::Content(uri);
            Ok(PlatformPath::from(kind))
        }
    }

    /// 列出目录下的文件 (仅一级, 不递归).
    /// Android 的 MediaStore 列表查询暂未实现, content URI 返回 NotSupported.
    pub fn list_files(&self, relative_path: &str) -> Result<Vec<PlatformFileEntry>> {
        match &self.kind {
            PathKind::Local(file_path) => {
                let target_path = if relative_path.is_empty() {
                    &file_path
                } else {
                    &file_path.join(relative_path)
                };
                let mut file_list: Vec<PlatformFileEntry> = vec![];
                if let Ok(file_reader) = fs::read_dir(target_path) {
                    for entry in file_reader {
                        let Ok(entry) = entry else { continue };
                        let path = entry.path();
                        if !path.is_file() {
                            continue;
                        }
                        let Some(file_name) = path.file_name() else {
                            continue;
                        };
                        let file_name = file_name.to_string_lossy().to_string();
                        let file_path = target_path.join(&file_name);
                        let mime_type = file_path.extension().map(|x| {
                            x.to_str().map(|s| MimeType::from_ext(s)).flatten()
                        }).flatten();
                        let file_size = fs::metadata(&file_path).map(|x| x.len()).unwrap_or(0);
                        file_list.push(PlatformFileEntry {
                            name: file_name.into(),
                            size: file_size,
                            path: PlatformPath::from(file_path),
                            mime_type: mime_type,
                            volume: None,
                        })
                    }
                }
                file_list.sort_by(|a, b| b.name.cmp(&a.name));
                Ok(file_list)
            },
            #[cfg(target_os = "android")]
            PathKind::Content(_uri) => {
                Err(Error::NotSupported)
            },
        }
    }

    pub fn is_local(&self) -> bool {
        matches!(self.kind, PathKind::Local(_))
    }

    pub fn is_content(&self) -> bool {
        #[cfg(target_os = "android")]
        {
            matches!(self.kind, PathKind::Content(_))
        }
        #[cfg(not(target_os = "android"))]
        {
            false
        }
    }

    pub fn as_local(&self) -> Option<&Path> {
        match &self.kind {
            PathKind::Local(p) => Some(p),
            #[cfg(target_os = "android")]
            PathKind::Content(_) => None,
        }
    }

    pub fn as_content(&self) -> Option<&str> {
        #[cfg(target_os = "android")]
        match &self.kind {
            PathKind::Content(s) => return Some(s),
            _ => {}
        }
        None
    }

    pub fn file_name(&self) -> Result<String> {
        match &self.kind {
            PathKind::Local(p) => {
                p.file_name().map(|x| x.to_string_lossy().to_string()).ok_or(Error::InvalidUri)
            },
            #[cfg(target_os = "android")]
            PathKind::Content(uri) => {
                let (name, _size) = crate::android::jni_content_query_file_info(uri)?;
                Ok(name)
            }
        }
    }

    pub fn file_size(&self) -> Result<u64> {
        match &self.kind {
            PathKind::Local(p) => Ok(fs::metadata(p)?.len()),
            #[cfg(target_os = "android")]
            PathKind::Content(uri) => {
                let (_name, size) = crate::android::jni_content_query_file_info(uri)?;
                Ok(size)
            },
        }
    }

    pub fn mime_type(&self) -> Result<mime_type::MimeType> {
        match &self.kind {
            PathKind::Local(p) => {
                p.extension()
                    .map(|x| {
                        let ext = x.to_string_lossy().to_string();
                        mime_type::MimeType::from_ext(&ext)
                    }).flatten().ok_or(Error::InvalidUri)
            },
            #[cfg(target_os = "android")]
            PathKind::Content(uri) => {
                crate::android::jni_content_query_mime_type(uri).map(|x| {
                    mime_type::MimeType::from_mime(&x).ok_or(Error::InvalidUri)
                }).flatten()
            },
        }
    }

    pub fn exists(&self) -> bool {
        match &self.kind {
            PathKind::Local(p) => p.exists(),
            #[cfg(target_os = "android")]
            PathKind::Content(uri) => {
                crate::android::jni_content_exists(uri)
            }
        }
    }

    pub fn join(&self, name: &str) -> Result<Self> {
        let kind = match &self.kind {
            PathKind::Local(p) => {
                Ok(PathKind::Local(p.join(name)))
            },
            #[cfg(target_os = "android")]
            PathKind::Content(uri) => {
                crate::android::jni_content_join(uri, name)
                    .map(|uri| PathKind::Content(uri))
            },
        };
        kind.map(|x| PlatformPath::from(x))
    }

    pub fn parent(&self) -> Option<PlatformPath> {
        let kind = match &self.kind {
            PathKind::Local(p) => {
                p.parent().map(|x| PathKind::Local(x.to_path_buf()))
            },
            #[cfg(target_os = "android")]
            PathKind::Content(uri) => {
                crate::android::jni_content_parent(uri)
                    .map(|uri| PathKind::Content(uri))
                    .ok()
            },
        };
        kind.map(|x| PlatformPath::from(x))
    }

    pub fn create_dir_all(&self) -> Result<()> {
        match &self.kind {
            PathKind::Local(p) => {
                std::fs::create_dir_all(p)?;
            },
            #[cfg(target_os = "android")]
            PathKind::Content(_uri) => {
                // Content URI 不支持目录创建, 返回错误而非 panic
                return Err(Error::NotSupported);
            },
        }
        Ok(())
    }

    pub fn remove(&self) -> Result<()> {
        match &self.kind {
            PathKind::Local(p) => fs::remove_file(p)?,
            #[cfg(target_os = "android")]
            PathKind::Content(uri) => {
                crate::android::jni_content_delete(uri)?
            },
        }
        Ok(())
    }

    pub fn remove_all(&self) -> Result<()> {
        match &self.kind {
            PathKind::Local(p) => {
                if !p.exists() {
                    return Ok(());
                }
                if !p.is_dir() {
                    return Ok(());
                }
                for entry in fs::read_dir(p)? {
                    let entry = entry?;
                    let entry_path = entry.path();

                    if entry_path.is_dir() {
                        fs::remove_dir_all(&entry_path)?;
                    } else {
                        fs::remove_file(&entry_path)?;
                    }
                }
            },
            #[cfg(target_os = "android")]
            PathKind::Content(uri) => {
                crate::android::jni_content_delete(uri)?
            },
        }
        Ok(())
    }

    pub fn open_file(&self, mode: PlatformFileMode) -> Result<PlatformFile> {
        PlatformFile::open(self, mode)
    }

    pub fn read_file(&self) -> Result<PlatformFile> {
        self.open_file(PlatformFileMode::Read)
    }

    pub fn write_file(&self) -> Result<PlatformFile> {
        self.open_file(PlatformFileMode::WriteTruncate)
    }

    pub fn append_file(&self) -> Result<PlatformFile> {
        self.open_file(PlatformFileMode::WriteAppend)
    }

    pub fn to_string(&self) -> String {
        match &self.kind {
            PathKind::Local(p) => p.to_string_lossy().to_string(),
            #[cfg(target_os = "android")]
            PathKind::Content(uri) => uri.to_string(),
        }
    }
}

impl fmt::Display for PlatformPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            PathKind::Local(p) => write!(f, "{}", p.display()),
            #[cfg(target_os = "android")]
            PathKind::Content(s) => write!(f, "{s}"),
        }
    }
}
