//! Guarded live container, independent of the immutable decoded snapshot.
//! Callers own FD/path signature checks; output decoding is unchanged.
use crate::input::{ReadSeek, Source};
use std::cell::Cell;
use std::fs::File;
use std::io::{Result, Seek, SeekFrom};
use std::os::unix::fs::FileExt;
pub(crate) struct Physical<'a> {
    file: &'a File,
    snapshot: Option<&'a crate::input::Snapshot>,
}
impl<'a> Physical<'a> {
    pub(crate) fn new(file: &'a File) -> Self {
        Self {
            file,
            snapshot: None,
        }
    }
    pub(crate) fn captured(file: &'a File, snapshot: &'a crate::input::Snapshot) -> Self {
        Self {
            file,
            snapshot: Some(snapshot),
        }
    }
    pub(crate) fn captured_admission(
        &self,
        path: &std::path::Path,
    ) -> Option<crate::Result<Option<&crate::edit_container::SeekTable>>> {
        self.snapshot
            .map(|snapshot| snapshot.proof.admit(self.file, path))
    }
    pub(crate) fn len(&self) -> Result<u64> {
        Ok(self.file.metadata()?.len())
    }
    pub(crate) fn check_before_publish(
        &self,
        path: &std::path::Path,
        metadata: &std::fs::Metadata,
    ) -> crate::Result<()> {
        crate::kernel_copy::check_source(self.file, path, metadata)
    }
    pub(crate) fn copy_whole(
        &self,
        path: &std::path::Path,
        metadata: &std::fs::Metadata,
        output: &mut File,
    ) -> crate::Result<u64> {
        crate::kernel_copy::full(
            self.file,
            path,
            metadata,
            output,
            self.snapshot.and_then(crate::input::Snapshot::raw_capture),
        )
    }
    pub(crate) fn family(&self) -> Result<&'static str> {
        if let Some(snapshot) = self.snapshot {
            return Ok(snapshot.proof.family);
        }
        let mut magic = [0; 4];
        let n = self.read_at(&mut magic, 0)?;
        Ok(if n >= 2 && magic[..2] == [0x1f, 0x8b] {
            "gzip"
        } else if n == 4
            && (u32::from_le_bytes(magic) == 0xfd2fb528
                || u32::from_le_bytes(magic) & 0xfffffff0 == 0x184d2a50)
        {
            "zstd"
        } else {
            "raw"
        })
    }
    pub(crate) fn read_at(&self, output: &mut [u8], offset: u64) -> Result<usize> {
        self.file.read_at(output, offset)
    }
    pub(crate) fn read_exact_at(&self, output: &mut [u8], offset: u64) -> Result<()> {
        self.file.read_exact_at(output, offset)
    }
    pub(crate) fn reader<'s>(
        &'s self,
        offset: u64,
        failed: &'s Cell<bool>,
    ) -> Result<Box<dyn ReadSeek + 's>> {
        let mut input: Box<dyn ReadSeek> = Box::new(Source {
            file: self.file.try_clone()?,
            failed,
        });
        input.seek(SeekFrom::Start(offset))?;
        Ok(input)
    }
}
