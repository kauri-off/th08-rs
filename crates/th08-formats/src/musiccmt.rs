//! musiccmt.txt, the music-room title and comment file (16 §5.1).

use crate::sjis::SjisString;

// Entry array 32 × 0x292, up to 7 comment lines per entry.
pub const MAX_ENTRIES: usize = 32;
pub const COMMENT_LINES: usize = 7;

// One musiccmt.txt entry (0x292 bytes): Shift-JIS bytes as stored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    // +0x00 file name after `@` (`bgm/…mid`).
    pub file: Vec<u8>,
    // +0x40 title.
    pub title: SjisString,
    // +0x82 + k·0x42, up to 7 comment lines.
    pub comments: Vec<SjisString>,
}

fn is_eol(c: u8) -> bool {
    c == b'\r' || c == b'\n'
}

// Parsing: skip to `@`, file name, title line, then up to 7 lines until the next `@`; CR/LF runs after each line
// are skipped, so blank lines are dropped. The original's copies are unbounded; entries past 32 are dropped here.
pub fn parse_musiccmt(data: &[u8]) -> Vec<Entry> {
    let mut p = 0;
    let line = |p: &mut usize| {
        let start = *p;
        while *p < data.len() && !is_eol(data[*p]) {
            *p += 1;
        }
        let s = data[start..*p].to_vec();
        while *p < data.len() && is_eol(data[*p]) {
            *p += 1;
        }
        s
    };
    let mut out = Vec::new();
    while out.len() < MAX_ENTRIES {
        while p < data.len() && data[p] != b'@' {
            p += 1;
        }
        if p >= data.len() {
            break;
        }
        p += 1;
        let file = line(&mut p);
        let title = SjisString(line(&mut p));
        let mut comments = Vec::new();
        while comments.len() < COMMENT_LINES && p < data.len() && data[p] != b'@' {
            comments.push(SjisString(line(&mut p)));
        }
        out.push(Entry {
            file,
            title,
            comments,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn musiccmt_parse() {
        let mut d = b"junk\r\n@bgm/th08_01.mid\r\nTitle A\r\n\r\nc0\r\nc1\r\n".to_vec();
        d.extend_from_slice(
            b"@bgm/th08_00.mid\nTitle B\n1\n2\n3\n4\n5\n6\n7\n8\n@bgm/x.mid\r\nT\r\n",
        );
        let e = parse_musiccmt(&d);
        assert_eq!(e.len(), 3);
        assert_eq!(e[0].file, b"bgm/th08_01.mid");
        assert_eq!(e[0].title.as_bytes(), b"Title A");
        // The blank line is dropped.
        assert_eq!(
            e[0].comments,
            vec![SjisString(b"c0".to_vec()), SjisString(b"c1".to_vec())]
        );
        // At most 7 comment lines; the 8th is skipped on the way to the next `@`.
        assert_eq!(e[1].comments.len(), 7);
        assert_eq!(e[1].comments[6].as_bytes(), b"7");
        assert_eq!(e[2].file, b"bgm/x.mid");
        assert!(e[2].comments.is_empty());
        // The array holds 32 entries.
        assert_eq!(parse_musiccmt(&b"@f\nt\n".repeat(40)).len(), 32);
    }
}
