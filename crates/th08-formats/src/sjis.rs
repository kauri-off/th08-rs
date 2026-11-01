//! Shift-JIS (cp932) game text kept as bytes (msg 10 §2, musiccmt 16 §5.1, .end 16 §1.3, spell names 15 §2.5).

use std::fmt;
use std::ops::Deref;

// Borrowed Shift-JIS bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SjisStr<'a>(pub &'a [u8]);

// Owned Shift-JIS bytes.
#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct SjisString(pub Vec<u8>);

pub fn decode(bytes: &[u8]) -> String {
    encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned()
}

impl<'a> SjisStr<'a> {
    pub fn as_bytes(&self) -> &'a [u8] {
        self.0
    }
    pub fn decode(&self) -> String {
        decode(self.0)
    }
    pub fn to_owned(&self) -> SjisString {
        SjisString(self.0.to_vec())
    }
}

// Port-built text (literals, numbers) to Shift-JIS; every such string is cp932-representable (checked in debug).
pub fn encode(s: &str) -> SjisString {
    let (b, _, bad) = encoding_rs::SHIFT_JIS.encode(s);
    debug_assert!(!bad, "not Shift-JIS representable: {s:?}");
    SjisString(b.into_owned())
}

impl SjisString {
    // Appends port-built text (`encode`).
    pub fn push_str(&mut self, s: &str) {
        self.0.extend_from_slice(&encode(s).0);
    }
    pub fn push_bytes(&mut self, b: &[u8]) {
        self.0.extend_from_slice(b);
    }
    pub fn as_sjis(&self) -> SjisStr<'_> {
        SjisStr(&self.0)
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
    pub fn decode(&self) -> String {
        decode(&self.0)
    }
}

impl Deref for SjisStr<'_> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.0
    }
}

impl Deref for SjisString {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for SjisString {
    fn from(b: Vec<u8>) -> Self {
        Self(b)
    }
}

impl From<&[u8]> for SjisString {
    fn from(b: &[u8]) -> Self {
        Self(b.to_vec())
    }
}

impl fmt::Debug for SjisStr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.decode(), f)
    }
}

impl fmt::Debug for SjisString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.decode(), f)
    }
}

impl fmt::Display for SjisStr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.decode())
    }
}

impl fmt::Display for SjisString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.decode())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_round() {
        let s = SjisString(vec![0x83, 0x76, 0x83, 0x8C, b'A']);
        assert_eq!(s.decode(), "プレA");
        assert_eq!(s.as_sjis().len(), 5);
        assert_eq!(format!("{s}"), "プレA");
        assert_eq!(encode("プレA"), s);
        let mut t = encode("Ｎｏ．");
        t.push_bytes(&[0x83, 0x76]);
        t.push_str("　");
        assert_eq!(t.decode(), "Ｎｏ．プ　");
    }
}
