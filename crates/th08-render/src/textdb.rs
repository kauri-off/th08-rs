//! Translated game text (I18N §2.3): `TextRef` key → UTF-8 text. A miss renders the vanilla Shift-JIS bytes exactly
//! as before, so with nothing mounted every `TextCmd` rasterises the original string (10 §6).

use std::collections::HashMap;
use th08_out::drawlist::TextSite;
use th08_out::textref::{SjisString, TextRef, sprintf};

use crate::text::Overflow;

// The renderer half of a mounted presentation pack (I18N G2): its texts, its font file, overflow per site.
#[derive(Clone, Debug, Default)]
pub struct PackText {
    pub db: TextDb,
    // TTF / OTF bytes of the pack's text fonts in chain order (FileFont, `[fonts] text` then `fallback`); empty = the
    // system font.
    pub fonts: Vec<Vec<u8>>,
    // `[targets]` of pack.toml; a site not listed uses `Overflow::default()`.
    pub overflow: HashMap<TextSite, Overflow>,
}

impl PackText {
    pub fn overflow(&self, site: TextSite) -> Overflow {
        self.overflow.get(&site).copied().unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default)]
pub struct TextDb {
    // Key string (`TextRef::key`) → translation.
    map: HashMap<String, String>,
}

impl TextDb {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    // Filled by a presentation pack mount (I18N G2); an empty text means "keep vanilla".
    pub fn insert(&mut self, key: String, text: String) {
        if !text.is_empty() {
            self.map.insert(key, text);
        }
    }

    pub fn get(&self, key: &TextRef) -> Option<&str> {
        self.map.get(&key.key()?).map(String::as_str)
    }

    // Text to rasterise for a command: the translation, a builtin format filled with resolved arguments, or the
    // decoded vanilla bytes.
    pub fn resolve(&self, key: &TextRef, raw: &SjisString) -> String {
        if self.map.is_empty() {
            return raw.decode();
        }
        match key {
            TextRef::Builtin { id, args } if !args.is_empty() => {
                let fmt = self.get(key).unwrap_or(id.text());
                let out = sprintf(fmt.as_bytes(), args, |t| {
                    self.resolve(&t.key, &t.raw).into_bytes()
                });
                String::from_utf8_lossy(&out).into_owned()
            }
            _ => self.get(key).map_or_else(|| raw.decode(), str::to_owned),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use th08_out::builtin::Builtin;
    use th08_out::textref::{FmtArg, SpellField, Text};

    fn name() -> Text {
        Text::new(
            TextRef::Spell {
                card: 3,
                field: SpellField::Name,
            },
            SjisString(vec![0x83, 0x41]),
        )
    }

    #[test]
    fn empty_db_is_vanilla() {
        let db = TextDb::new();
        let t = Builtin::SpNoLine.fmt(vec![
            FmtArg::Text(Text::dynamic_str("０１２")),
            FmtArg::Text(name()),
        ]);
        assert_eq!(db.resolve(&t.key, &t.raw), t.raw.decode());
        assert_eq!(db.resolve(&TextRef::Dynamic, &t.raw), t.raw.decode());
    }

    #[test]
    fn translated_keys_and_arguments() {
        let mut db = TextDb::new();
        db.insert("spell/3/name".into(), "Firefly".into());
        db.insert("builtin/menu.top.0".into(), "Start".into());
        let m = Builtin::MenuTop(0).txt();
        assert_eq!(db.resolve(&m.key, &m.raw), "Start");
        let n = name();
        assert_eq!(db.resolve(&n.key, &n.raw), "Firefly");
        // Untranslated format, translated argument.
        let t = Builtin::SpNoLine.fmt(vec![
            FmtArg::Text(Text::dynamic_str("０１２")),
            FmtArg::Text(name()),
        ]);
        assert_eq!(db.resolve(&t.key, &t.raw), "Ｎｏ．０１２　　Firefly");
        db.insert("builtin/spell.no_line".into(), "No. %s  %s".into());
        assert_eq!(db.resolve(&t.key, &t.raw), "No. ０１２  Firefly");
        let c = Builtin::ResultCards.fmt(vec![FmtArg::Int(1), FmtArg::Int(22)]);
        assert_eq!(db.resolve(&c.key, &c.raw), "  1/ 22（キャラ切り替え↓↑）");
    }
}
