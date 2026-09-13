use std::fs::{File, OpenOptions};
use crate::{Result, Error};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[non_exhaustive]
pub enum PlatformFileMode {

    /// Opens the file in read-only mode.
    ///
    /// FileDescriptor mode: "r"
    Read,

    /// Opens the file in write-only mode.
    ///
    /// Until Android 10, this will always truncate existing contents.
    /// Since Android 10, this may or may not truncate existing contents.
    /// If the new file is smaller than the old one, **this may cause the file to become corrupted**.
    /// <https://issuetracker.google.com/issues/180526528>
    ///
    /// The reason this is marked as deprecated is because of that behavior,
    /// and it is not scheduled to be removed in the future.
    ///
    /// FileDescriptor mode: "w"
    #[deprecated(note = "This may or may not truncate existing contents. If the new file is smaller than the old one, this may cause the file to become corrupted.")]
    Write,

    /// Opens the file in write-only mode.
    /// The existing content is truncated (deleted), and new data is written from the beginning.
    ///
    /// FileDescriptor mode: "wt"
    WriteTruncate,

    /// Opens the file in write-only mode.
    /// The existing content is preserved, and new data is appended to the end of the file.
    ///
    /// FileDescriptor mode: "wa"
    WriteAppend,

    /// Opens the file in read-write mode.
    ///
    /// FileDescriptor mode: "rw"
    ReadWrite,

    /// Opens the file in read-write mode.
    /// The existing content is truncated (deleted), and new data is written from the beginning.
    ///
    /// FileDescriptor mode: "rwt"
    ReadWriteTruncate,
}

#[allow(unused)]
#[allow(deprecated)]
impl PlatformFileMode {

    pub(crate) fn to_mode(&self) -> &'static str {
        match self {
            PlatformFileMode::Read => "r",
            PlatformFileMode::Write => "w",
            PlatformFileMode::WriteTruncate => "wt",
            PlatformFileMode::WriteAppend => "wa",
            PlatformFileMode::ReadWriteTruncate => "rwt",
            PlatformFileMode::ReadWrite => "rw",
        }
    }

    pub(crate) fn from_mode(mode: &str) -> Result<Self> {
        match mode {
            "r" => Ok(Self::Read),
            "w" => Ok(Self::Write),
            "wt" => Ok(Self::WriteTruncate),
            "wa" => Ok(Self::WriteAppend),
            "rwt" => Ok(Self::ReadWriteTruncate),
            "rw" => Ok(Self::ReadWrite),
            mode => Err(Error::AccessError(mode.to_string()))
        }
    }

    pub(crate) fn to_file_options(&self) -> OpenOptions {
        let mut options = File::options();
        match self {
            PlatformFileMode::Read => {
                options.read(true);
            }
            PlatformFileMode::Write => {
                options.create(true).write(true);
            }
            PlatformFileMode::WriteTruncate => {
                options.create(true).write(true).truncate(true);
            }
            PlatformFileMode::WriteAppend => {
                options.create(true).append(true);
            }
            PlatformFileMode::ReadWrite => {
                options.create(true).read(true).write(true);
            }
            PlatformFileMode::ReadWriteTruncate => {
                options.create(true).read(true).write(true).truncate(true);
            }
        }
        options
    }
}
