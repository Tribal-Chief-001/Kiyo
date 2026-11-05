//! `kiyo-core`: Zero-copy PDF document model, cross-reference streaming, and manipulation.

use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use anyhow::Result;
use memmap2::Mmap;

/// PDF document backed by zero-copy virtual memory mapping.
pub struct Document {
    pub file_size: u64,
    pub mmap: Arc<Mmap>,
}

impl Document {
    /// Opens and memory-maps a PDF document from disk.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path)?;
        let file_size = file.metadata()?.len();
        let mmap = unsafe { memmap2::MmapOptions::new().map(&file)? };

        #[cfg(target_os = "linux")]
        unsafe {
            libc::madvise(
                mmap.as_ptr() as *mut libc::c_void,
                mmap.len(),
                libc::MADV_WILLNEED | libc::MADV_RANDOM,
            );
        }

        Ok(Self {
            file_size,
            mmap: Arc::new(mmap),
        })
    }
}
