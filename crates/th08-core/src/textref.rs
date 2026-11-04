//! Display-text keys on `TextCmd` (I18N §2.3, ARCH_B §1.4): where a string came from, next to its vanilla bytes.
//! Core only builds keys; translations are looked up on the render side and never read here.

use crate::builtin::Builtin;
use th08_formats::sjis;
pub use th08_formats::sjis::SjisString;

// Card record text fields (ECL op 122, 07 §11.1; CATK +0x10 / +0x40 / +0x70 / +0xB0, 15 §2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpellField {
    Name,
    Owner,
    Comment(u8),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TextRef {
    // Text op (3/16/19/20) number `nth` of msg entry `entry` in file order (10 §5.1); `file` indexes `MSG_FILES`.
    // Entries sharing an offset use the lowest index (10 §2), so one text has one key.
    Msg { file: u8, entry: u16, nth: u16 },
    // Card id = ECL op 122 +0x0E, 0-based (07 §11.1).
    Spell { card: u16, field: SpellField },
    // 0-based physical line (LF-split) of a text line in `END_FILES[file]` (16 §1.3).
    Ending { file: u8, line: u16 },
    // musiccmt.txt entry `track` (0-based): title row and comment line k (16 §5.1).
    MusicTitle { track: u8 },
    MusicCmt { track: u8, line: u8 },
    // Exe literal or printf format with its arguments (`builtin.rs`).
    Builtin { id: Builtin, args: Vec<FmtArg> },
    // Player names, digits, padding: raw bytes only, never translated.
    Dynamic,
}

// printf argument of a `Builtin` format: an integer, or a text with its own key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FmtArg {
    Int(i64),
    Text(Text),
}

// A displayed string: translation key plus the vanilla Shift-JIS bytes the original renders.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Text {
    pub key: TextRef,
    pub raw: SjisString,
}

impl Text {
    pub fn new(key: TextRef, raw: SjisString) -> Self {
        Self { key, raw }
    }

    pub fn dynamic(raw: SjisString) -> Self {
        Self::new(TextRef::Dynamic, raw)
    }

    // Port-built text that is never translated (digits, padding).
    pub fn dynamic_str(s: &str) -> Self {
        Self::dynamic(sjis::encode(s))
    }
}

impl Builtin {
    // The literal as a text.
    pub fn txt(self) -> Text {
        Text::new(
            TextRef::Builtin {
                id: self,
                args: Vec::new(),
            },
            sjis::encode(self.text()),
        )
    }

    // The printf format filled with the vanilla bytes of `args` (`sprintf`).
    pub fn fmt(self, args: Vec<FmtArg>) -> Text {
        let raw = sprintf(sjis::encode(self.text()).as_bytes(), &args, |t| {
            t.raw.as_bytes().to_vec()
        });
        Text::new(TextRef::Builtin { id: self, args }, SjisString(raw))
    }
}

// The msg files of `msg::FILES` (10 §1), each once.
pub const MSG_FILES: [&str; 33] = [
    "msg1a", "msg1b", "msg1c", "msg1d", "msg2a", "msg2b", "msg2c", "msg2d", "msg3a", "msg3b",
    "msg3c", "msg3d", "msg4dm", "msg4ab", "msg4ac", "msg4ba", "msg4bd", "msg5a", "msg5b", "msg5c",
    "msg5d", "msg6a", "msg6b", "msg6c", "msg6d", "msg7a", "msg7b", "msg7c", "msg7d", "msg8a",
    "msg8b", "msg8c", "msg8d",
];

// Ending scripts (16 §1.2 step 5) and the staff-roll files they chain to.
pub const END_FILES: [&str; 14] = [
    "end00a.end",
    "end00b.end",
    "end00c.end",
    "end01a.end",
    "end01b.end",
    "end01c.end",
    "end02a.end",
    "end02b.end",
    "end02c.end",
    "end03a.end",
    "end03b.end",
    "end03c.end",
    "staff00.end",
    "staff00b.end",
];

pub fn end_file(name: &str) -> Option<u8> {
    END_FILES.iter().position(|&n| n == name).map(|i| i as u8)
}

impl TextRef {
    // Stable key string of a translation table (ARCH_B §1.4 form): `msg/msg4a.dat/3/17`, `spell/112/name`,
    // `end/end00a.end/5`, `music/3/title`, `musiccmt/3/2`, `builtin/menu.top.0`. Dynamic has none.
    pub fn key(&self) -> Option<String> {
        Some(match self {
            Self::Msg { file, entry, nth } => {
                format!(
                    "msg/{}.dat/{entry}/{nth}",
                    MSG_FILES.get(usize::from(*file))?
                )
            }
            Self::Spell { card, field } => match field {
                SpellField::Name => format!("spell/{card}/name"),
                SpellField::Owner => format!("spell/{card}/owner"),
                SpellField::Comment(k) => format!("spell/{card}/comment{k}"),
            },
            Self::Ending { file, line } => {
                format!("end/{}/{line}", END_FILES.get(usize::from(*file))?)
            }
            Self::MusicTitle { track } => format!("music/{track}/title"),
            Self::MusicCmt { track, line } => format!("musiccmt/{track}/{line}"),
            Self::Builtin { id, .. } => format!("builtin/{}", id.key()),
            Self::Dynamic => return None,
        })
    }
}

// C printf subset of the exe formats (`%s`, `%d` with flags `-`/`0`, width, `.precision`; `%%`) over bytes, so it
// runs on Shift-JIS and UTF-8 alike ('%' is never a cp932 trail byte). `text` renders a text argument; a missing or
// mismatched argument prints nothing.
pub fn sprintf(fmt: &[u8], args: &[FmtArg], text: impl Fn(&Text) -> Vec<u8>) -> Vec<u8> {
    let mut out = Vec::new();
    let mut next = args.iter();
    let mut i = 0;
    while i < fmt.len() {
        let c = fmt[i];
        i += 1;
        if c != b'%' {
            out.push(c);
            continue;
        }
        let (mut left, mut zero) = (false, false);
        while let Some(&f) = fmt.get(i).filter(|f| matches!(f, b'-' | b'0')) {
            left |= f == b'-';
            zero |= f == b'0';
            i += 1;
        }
        let width = number(fmt, &mut i);
        let prec = if fmt.get(i) == Some(&b'.') {
            i += 1;
            Some(number(fmt, &mut i))
        } else {
            None
        };
        let conv = fmt.get(i).copied().unwrap_or(0);
        i += 1;
        let body = match (conv, next.next()) {
            (b'%', _) => b"%".to_vec(),
            (b'd', Some(FmtArg::Int(v))) => {
                let digits = v.unsigned_abs().to_string();
                let digits = format!("{digits:0>p$}", p = prec.unwrap_or(1));
                let sign = if *v < 0 { "-" } else { "" };
                let s = if zero && !left && prec.is_none() {
                    format!("{sign}{digits:0>w$}", w = width.saturating_sub(sign.len()))
                } else {
                    format!("{sign}{digits}")
                };
                s.into_bytes()
            }
            (b's', Some(FmtArg::Text(t))) => text(t),
            _ => Vec::new(),
        };
        // Width counts bytes, as the original's cp932 `sprintf` does.
        let pad = width.saturating_sub(body.len());
        if !left {
            out.extend(std::iter::repeat_n(b' ', pad));
        }
        out.extend_from_slice(&body);
        if left {
            out.extend(std::iter::repeat_n(b' ', pad));
        }
    }
    out
}

// Decimal digits at `*i` (printf width / precision), advancing `*i`.
fn number(fmt: &[u8], i: &mut usize) -> usize {
    let mut n = 0usize;
    while let Some(d) = fmt.get(*i).filter(|d| d.is_ascii_digit()) {
        n = n * 10 + usize::from(d - b'0');
        *i += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(fmt: &str, args: &[FmtArg]) -> String {
        String::from_utf8(sprintf(fmt.as_bytes(), args, |t| t.raw.as_bytes().to_vec())).unwrap()
    }

    #[test]
    fn printf_subset() {
        use FmtArg::Int;
        assert_eq!(p("%3d/%3d", &[Int(7), Int(123)]), "  7/123");
        assert_eq!(p("%.2d:%.2d", &[Int(5), Int(-1)]), "05:-01");
        assert_eq!(
            p("%6d|%-4d|%05d|%%", &[Int(-42), Int(3), Int(-7)]),
            "   -42|3   |-0007|%"
        );
        let t = FmtArg::Text(Text::dynamic(SjisString(b"ab".to_vec())));
        assert_eq!(p("[%s] %4s", &[t.clone(), t]), "[ab]   ab");
    }

    #[test]
    fn keys() {
        let m = TextRef::Msg {
            file: 13,
            entry: 0,
            nth: 4,
        };
        assert_eq!(m.key().as_deref(), Some("msg/msg4ab.dat/0/4"));
        let e = TextRef::Ending { file: 13, line: 2 };
        assert_eq!(e.key().as_deref(), Some("end/staff00b.end/2"));
        assert_eq!(TextRef::Dynamic.key(), None);
        assert_eq!(
            Builtin::SpHint { card: 210, line: 1 }
                .txt()
                .key
                .key()
                .as_deref(),
            Some("builtin/spell.hint.210.1")
        );
    }

    // Every msg::FILES name is in MSG_FILES.
    #[test]
    fn msg_files_cover_the_table() {
        for stage in 0..9 {
            for shot in 0..12 {
                let n = crate::msg::file_name(stage, shot);
                assert!(MSG_FILES.contains(&n.trim_end_matches(".dat")), "{n}");
            }
        }
    }

    #[test]
    fn builtin_keys_unique_and_sjis() {
        let all = Builtin::all();
        let keys: std::collections::HashSet<_> = all.iter().map(|b| b.key()).collect();
        assert_eq!(keys.len(), all.len());
        for b in all {
            assert_eq!(sjis::encode(b.text()).decode(), b.text(), "{b:?}");
        }
    }
}
