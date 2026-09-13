use std::io;
#[cfg(unix)]
use std::os::fd::RawFd;
use libc::c_int;

/// Content URI 基于 fd 的包装（实现标准 IO）
#[derive(Debug)]
pub struct FileDescriptor {
    fd: c_int,
    owned: bool,
}

impl FileDescriptor {
    pub fn new(fd: c_int, owned: bool) -> Self {
        FileDescriptor { fd, owned }
    }

    #[cfg(unix)]
    pub fn as_raw_fd(&self) -> RawFd {
        self.fd
    }

    /// Duplicates the underlying fd. Both fds share the same file description
    /// (offset included), so a cloned handle can be used to re-open a decoder
    /// without re-acquiring content URI permissions on Android.
    pub fn try_clone(&self) -> io::Result<Self> {
        #[cfg(unix)]
        let new_fd = unsafe { libc::dup(self.fd) };
        #[cfg(windows)]
        let new_fd = {
            let _ = self.fd;
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "try_clone not supported on windows",
            ));
        };
        if new_fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(FileDescriptor::new(new_fd, true))
    }
}


impl Drop for FileDescriptor {
    fn drop(&mut self) {
        if self.owned && self.fd >= 0 {
            unsafe {
                libc::close(self.fd)
            };
        }
    }
}

impl io::Read for FileDescriptor {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        #[cfg(unix)]
        let count = buf.len() as libc::size_t;
        #[cfg(windows)]
        let count: libc::c_uint = buf.len().try_into().map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "buffer too large"))?;

        let n = unsafe {
            libc::read(
                self.fd,
                buf.as_mut_ptr() as *mut _,
                count,
            )
        };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(n as usize)
    }
}

impl io::Write for FileDescriptor {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        #[cfg(unix)]
        let count = buf.len() as libc::size_t;
        #[cfg(windows)]
        let count: libc::c_uint = buf.len().try_into().map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "buffer too large"))?;

        let n = unsafe {
            libc::write(
                self.fd,
                buf.as_ptr() as *const _,
                count,
            )
        };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(n as usize)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl io::Seek for FileDescriptor {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        use io::SeekFrom::*;
        let (whence, off) = match pos {
            Start(o) => (libc::SEEK_SET, o as i64),
            End(o) => (libc::SEEK_END, o),
            Current(o) => (libc::SEEK_CUR, o),
        };
        #[cfg(unix)]
        let res = unsafe { libc::lseek(self.fd, off as libc::off_t, whence) };
        #[cfg(windows)]
        let res = {
            let off_l = libc::c_long::try_from(off).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "seek offset out of range for this platform",
                )
            })?;
            unsafe { libc::lseek(self.fd, off_l, whence) }
        };
        if res < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(res as u64)
        }
    }
}
