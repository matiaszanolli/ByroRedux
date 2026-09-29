//! Positional (cursor-free) reads shared by every archive reader.
//!
//! The readers used to keep a `Mutex<File>` and run `seek` + `read_exact`
//! under it, because both calls move the handle's shared cursor. Positional
//! reads (`pread` on Unix, `ReadFile` with an `OVERLAPPED` offset on Windows)
//! take the offset as an argument, so concurrent extracts from one archive
//! need no user-space lock and stay correct on a shared handle.
//!
//! #4999 — "no lock" is not "no waiting" on every platform. On Unix `pread`
//! reads from one descriptor run concurrently. On Windows, `File::open`
//! returns a synchronous handle (no `FILE_FLAG_OVERLAPPED`), and the I/O
//! manager serialises every request on a synchronous file object, so reads
//! of one archive still queue behind each other in the kernel. Inflate runs
//! outside the read, so decompression stays parallel on both. Opening with
//! `FILE_FLAG_OVERLAPPED` (`OpenOptionsExt::custom_flags`) would be the
//! route to parallel reads on Windows if that ever matters.

use std::fs::File;
use std::io;

/// Read exactly `buf.len()` bytes starting at absolute byte `offset`.
pub(crate) trait ReadAt {
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()>;
}

#[cfg(unix)]
impl ReadAt for File {
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()> {
        std::os::unix::fs::FileExt::read_exact_at(self, buf, offset)
    }
}

#[cfg(windows)]
impl ReadAt for File {
    fn read_exact_at(&self, mut buf: &mut [u8], mut offset: u64) -> io::Result<()> {
        // `seek_read` also moves the handle's cursor on Windows, but nothing
        // reads through the cursor after `open`, so that side effect is inert.
        use std::os::windows::fs::FileExt;
        while !buf.is_empty() {
            match self.seek_read(buf, offset) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "failed to fill whole buffer",
                    ))
                }
                Ok(n) => {
                    buf = &mut buf[n..];
                    offset += n as u64;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

/// In-memory source for unit tests that build archive bodies as byte vectors.
#[cfg(test)]
impl ReadAt for [u8] {
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()> {
        let start = usize::try_from(offset).ok();
        let end = start.and_then(|s| s.checked_add(buf.len()));
        match (start, end) {
            (Some(s), Some(e)) if e <= self.len() => {
                buf.copy_from_slice(&self[s..e]);
                Ok(())
            }
            _ => Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "failed to fill whole buffer",
            )),
        }
    }
}
