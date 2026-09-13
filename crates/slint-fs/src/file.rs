use std::io;
use std::fs::File;
use std::io::{Read, Write, Seek};
use crate::error::Result;
use crate::file_access::PlatformFileMode;
#[cfg(target_os = "android")]
use crate::file_descriptor::FileDescriptor;
use crate::file_path::{PathKind, PlatformPath};

#[cfg(unix)]
type FileFd = std::os::fd::RawFd;
#[cfg(windows)]
type FileFd = std::os::windows::io::RawHandle;

#[derive(Debug)]
enum FileInner {
    Local(File),
    #[cfg(target_os = "android")]
    Content(FileDescriptor),
}

#[derive(Debug)]
pub struct PlatformFile {
    path: PlatformPath,
    inner: FileInner,
}

impl PlatformFile {
    pub fn open(path: &PlatformPath, mode: PlatformFileMode) -> Result<Self> {
        let inner = match &path.kind {
            PathKind::Local(p) => {
                let options = mode.to_file_options();
                FileInner::Local(options.open(p)?)
            },
            #[cfg(target_os = "android")]
            PathKind::Content(uri) => {
                let fd = crate::android::jni_content_open_fd(uri, mode.to_mode())?;
                FileInner::Content(FileDescriptor::new(fd, true))
            }
        };
        Ok(Self {
            path: path.clone(),
            inner
        })
    }


    pub fn as_raw_fd(&self) -> FileFd {
        match &self.inner {
            FileInner::Local(f) => {
                #[cfg(unix)]
                {
                    use std::os::fd::AsRawFd;
                    f.as_raw_fd()
                }
                #[cfg(windows)]
                {
                    use std::os::windows::io::AsRawHandle;
                    f.as_raw_handle()
                }
            },
            #[cfg(target_os = "android")]
            FileInner::Content(fd) => fd.as_raw_fd(),
        }
    }

    #[cfg_attr(not(target_os = "android"), allow(unused_variables))]
    pub fn create(path: &PlatformPath, mode: PlatformFileMode) -> Result<Self> {
        let inner = match &path.kind {
            PathKind::Local(p) => FileInner::Local(File::create(p)?),
            #[cfg(target_os = "android")]
            PathKind::Content(uri) => {
                let fd = crate::android::jni_content_open_fd(uri, mode.to_mode())?;
                FileInner::Content(FileDescriptor::new(fd, true))
            }
        };
        Ok(Self {
            path: path.clone(),
            inner
        })
    }

    pub fn path(&self) -> &PlatformPath {
        &self.path
    }

    pub fn len(&self) -> Result<u64> {
        match &self.path.kind {
            PathKind::Local(p) => Ok(std::fs::metadata(p)?.len()),
            #[cfg(target_os = "android")]
            PathKind::Content(uri) => {
                let (_name, size) = crate::android::jni_content_query_file_info(uri)?;
                Ok(size)
            },
        }
    }

    pub fn sync_all(&self) -> Result<()> {
        match &self.inner {
            FileInner::Local(f) => f.sync_all()?,
            #[cfg(target_os = "android")]
            FileInner::Content(_) => {}
        }
        Ok(())
    }

    pub fn read_string(&mut self) -> Result<String> {
        let mut text = String::new();
        let _n = self.read_to_string(&mut text)?;
        Ok(text)
    }

    pub fn read_bytes(&mut self) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        let _n = self.read_to_end(&mut data)?;
        Ok(data)
    }
}

impl Read for PlatformFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.inner {
            FileInner::Local(f) => f.read(buf),
            #[cfg(target_os = "android")]
            FileInner::Content(c) => c.read(buf),
        }
    }
}

impl PlatformFile {
    /// Duplicates the underlying handle. The clone shares the file offset with
    /// the original, so it can be used to re-open a decoder without
    /// re-acquiring content URI permissions on Android.
    pub fn try_clone(&self) -> Result<Self> {
        let inner = match &self.inner {
            FileInner::Local(f) => FileInner::Local(f.try_clone()?),
            #[cfg(target_os = "android")]
            FileInner::Content(c) => FileInner::Content(c.try_clone()?),
        };
        Ok(Self {
            path: self.path.clone(),
            inner,
        })
    }
}

impl Write for PlatformFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match &mut self.inner {
            FileInner::Local(f) => f.write(buf),
            #[cfg(target_os = "android")]
            FileInner::Content(c) => c.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.inner {
            FileInner::Local(f) => f.flush(),
            #[cfg(target_os = "android")]
            FileInner::Content(_file_descriptor) => {
                if self.path.is_content() {
                    let uri = self.path.to_string();
                    let _result = crate::android::jni_content_flush(
                        &uri,
                        true
                    );
                }
                Ok(())
            },
        }
    }
}

impl Seek for PlatformFile {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        match &mut self.inner {
            FileInner::Local(f) => f.seek(pos),
            #[cfg(target_os = "android")]
            FileInner::Content(c) => c.seek(pos),
        }
    }
}
