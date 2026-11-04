//! .end staff-roll script lexer (16 §1.3): `@` commands, int/string arguments, text lines.

use crate::sjis::SjisString;

// Script buffer and read pointer (Ending_LoadScript 0x4295E0).
#[derive(Clone, Debug, Default)]
pub struct Script {
    pub data: Vec<u8>,
    pub ip: usize,
}

impl Script {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data, ip: 0 }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn at_end(&self) -> bool {
        self.ip >= self.data.len()
    }

    // 0-based line (LF count before the read pointer): the key of a text line.
    pub fn line_no(&self) -> usize {
        self.data[..self.ip.min(self.data.len())]
            .iter()
            .filter(|&&b| b == b'\n')
            .count()
    }

    // Bytes past the end read as 0.
    pub fn byte(&self, p: usize) -> u8 {
        self.data.get(p).copied().unwrap_or(0)
    }

    // Integer argument: `atol`, then skip to the next NUL and over any NULs.
    pub fn int_arg(&mut self) -> i32 {
        let mut p = self.ip;
        while matches!(self.byte(p), b' ' | b'\t') {
            p += 1;
        }
        let neg = match self.byte(p) {
            b'-' => {
                p += 1;
                true
            }
            b'+' => {
                p += 1;
                false
            }
            _ => false,
        };
        let mut v: i32 = 0;
        while self.byte(p).is_ascii_digit() {
            v = v
                .wrapping_mul(10)
                .wrapping_add(i32::from(self.byte(p) - b'0'));
            p += 1;
        }
        self.ip = p;
        self.skip_arg();
        if neg { v.wrapping_neg() } else { v }
    }

    // String argument up to its NUL terminator (file names: ASCII).
    pub fn str_arg(&mut self) -> String {
        let start = self.ip;
        let mut p = start;
        while p < self.data.len() && !matches!(self.byte(p), 0 | b'\n' | b'\r') {
            p += 1;
        }
        let s = String::from_utf8_lossy(&self.data[start..p]).into_owned();
        self.ip = p;
        self.skip_arg();
        s
    }

    pub fn skip_arg(&mut self) {
        while self.ip < self.data.len() && !matches!(self.byte(self.ip), 0 | b'\n') {
            self.ip += 1;
        }
        while self.ip < self.data.len() && self.byte(self.ip) == 0 {
            self.ip += 1;
        }
    }

    // After a command (and a text line): skip to the end of the line and over all CR/LF.
    pub fn skip_line(&mut self) {
        while self.ip < self.data.len() && self.byte(self.ip) != b'\n' {
            self.ip += 1;
        }
        while self.ip < self.data.len() && matches!(self.byte(self.ip), b'\n' | b'\r') {
            self.ip += 1;
        }
    }

    // Text line: copied two bytes at a time until 0 / LF / CR at an even position, while the copy plus 2
    // stays below `cap` bytes (the line buffer size).
    pub fn text_line(&mut self, cap: usize) -> SjisString {
        let mut buf = Vec::new();
        while buf.len() + 2 < cap {
            let c = self.byte(self.ip);
            if self.ip >= self.data.len() || matches!(c, 0 | b'\n' | b'\r') {
                break;
            }
            buf.push(c);
            buf.push(self.byte(self.ip + 1));
            self.ip += 2;
        }
        SjisString(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_and_lines() {
        let mut s = Script::new(b"@s -12\0\0x\n@Ffile.end\0\nABC\r\n".to_vec());
        assert_eq!((s.byte(0), s.byte(1)), (b'@', b's'));
        s.ip = 2;
        assert_eq!(s.int_arg(), -12);
        assert_eq!(s.byte(s.ip), b'x');
        s.skip_line();
        s.ip += 2;
        assert_eq!(s.str_arg(), "file.end");
        s.skip_line();
        // Odd-length line: the pair copy takes the CR along.
        assert_eq!(s.text_line(64).as_bytes(), b"ABC\r");
        assert_eq!(s.byte(s.ip), b'\n');
        s.skip_line();
        assert!(s.at_end());
    }
}
