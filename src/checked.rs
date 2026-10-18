//! checked byte cursor. Bounds derive from input, never budgets.
use crate::{Error, Result};

pub(crate) fn add(a: usize, b: usize) -> Result<usize> {
    a.checked_add(b)
        .ok_or_else(|| Error::new("overflow", "offset addition"))
}
pub(crate) fn mul(a: usize, b: usize) -> Result<usize> {
    a.checked_mul(b)
        .ok_or_else(|| Error::new("overflow", "size multiplication"))
}
pub(crate) fn bytes(data: &[u8], offset: usize, len: usize) -> Result<&[u8]> {
    data.get(offset..add(offset, len)?).ok_or_else(|| {
        Error::new(
            "truncated",
            format!("range {offset}+{len} exceeds {} bytes", data.len()),
        )
    })
}
pub(crate) fn number(data: &[u8], offset: usize, len: usize, little: bool) -> Result<u64> {
    let data = bytes(data, offset, len)?;
    let mut value = 0;
    if little {
        for (i, byte) in data.iter().enumerate() {
            value |= u64::from(*byte) << (i * 8);
        }
    } else {
        for byte in data {
            value = (value << 8) | u64::from(*byte);
        }
    }
    Ok(value)
}
pub(crate) struct Cursor<'a> {
    pub data: &'a [u8],
    pub offset: usize,
    pub little: bool,
}
impl<'a> Cursor<'a> {
    pub fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let value = bytes(self.data, self.offset, len)?;
        self.offset = add(self.offset, len)?;
        Ok(value)
    }
    pub fn uint(&mut self, len: usize) -> Result<u64> {
        let value = number(self.data, self.offset, len, self.little)?;
        self.offset = add(self.offset, len)?;
        Ok(value)
    }
    pub fn tag(&mut self, value: &[u8]) -> Result<()> {
        if self.take(value.len())? != value {
            return Err(Error::new("invalid_sdna", "section tag"));
        }
        Ok(())
    }
    pub fn aligned(&mut self) -> Result<()> {
        let count = (4 - self.offset % 4) % 4;
        self.take(count)?;
        Ok(())
    }
    pub fn string(&mut self) -> Result<Vec<u8>> {
        let rest = self
            .data
            .get(self.offset..)
            .ok_or_else(|| Error::new("truncated", "string offset"))?;
        let len = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| Error::new("invalid_sdna", "unterminated name"))?;
        let result = self.take(len)?.to_vec();
        self.take(1)?;
        Ok(result)
    }
    pub fn remaining_records(&self, count: usize, width: usize) -> Result<()> {
        bytes(self.data, self.offset, mul(count, width)?)?;
        Ok(())
    }
}
