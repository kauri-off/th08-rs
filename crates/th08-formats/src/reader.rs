//! Little-endian byte cursor and fixed-offset record accessors shared by the format parsers.

// A read that runs past the end of the input; `at` is the offset of the first missing byte's read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("read past end at {at:#x}")]
pub struct Truncated {
    pub at: usize,
}

pub type Result<T> = std::result::Result<T, Truncated>;

// Cursor over a byte slice; every read is bounds-checked and advances the position.
#[derive(Clone, Copy, Debug)]
pub struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Self { b, pos: 0 }
    }

    // Cursor at byte offset `off` (may lie past the end; the first read then fails).
    pub fn at(b: &'a [u8], off: usize) -> Self {
        Self { b, pos: off }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn seek(&mut self, off: usize) -> &mut Self {
        self.pos = off;
        self
    }

    pub fn skip(&mut self, n: usize) -> &mut Self {
        self.pos = self.pos.saturating_add(n);
        self
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let at = self.pos;
        let s = at
            .checked_add(n)
            .and_then(|end| self.b.get(at..end))
            .ok_or(Truncated { at })?;
        self.pos = at + n;
        Ok(s)
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let at = self.pos;
        let s = self.bytes(N)?;
        s.try_into().map_err(|_| Truncated { at })
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }
    pub fn u16(&mut self) -> Result<u16> {
        self.array().map(u16::from_le_bytes)
    }
    pub fn i16(&mut self) -> Result<i16> {
        self.array().map(i16::from_le_bytes)
    }
    pub fn u32(&mut self) -> Result<u32> {
        self.array().map(u32::from_le_bytes)
    }
    pub fn i32(&mut self) -> Result<i32> {
        self.array().map(i32::from_le_bytes)
    }
    pub fn f32(&mut self) -> Result<f32> {
        self.array().map(f32::from_le_bytes)
    }
    pub fn vec3(&mut self) -> Result<[f32; 3]> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }
    pub fn u16s<const N: usize>(&mut self) -> Result<[u16; N]> {
        let mut v = [0; N];
        for x in &mut v {
            *x = self.u16()?;
        }
        Ok(v)
    }
    pub fn u32s<const N: usize>(&mut self) -> Result<[u32; N]> {
        let mut v = [0; N];
        for x in &mut v {
            *x = self.u32()?;
        }
        Ok(v)
    }
}

// Fixed-offset reads of a record whose length is an invariant of its type (out of range = a port bug, panics).
pub(crate) fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

pub(crate) fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

pub(crate) fn put_u32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_truncation() {
        let b = [1, 0, 0xFE, 0xFF, 0, 0, 0x80, 0x3F, 9];
        let mut r = Reader::new(&b);
        assert_eq!(r.u16(), Ok(1));
        assert_eq!(r.i16(), Ok(-2));
        assert_eq!(r.f32(), Ok(1.0));
        assert_eq!(r.pos(), 8);
        assert_eq!(r.u32(), Err(Truncated { at: 8 }));
        assert_eq!(r.pos(), 8);
        assert_eq!(r.u8(), Ok(9));
        assert_eq!(Reader::at(&b, 2).i32(), Ok(0xFFFE));
        assert_eq!(
            Reader::at(&b, usize::MAX).bytes(2),
            Err(Truncated { at: usize::MAX })
        );
        assert_eq!((u16_at(&b, 2), u32_at(&b, 4)), (0xFFFE, 0x3F80_0000));
    }
}
