//! Pack font chain (I18N G3): the translation's own fonts (patch.js `fonts`, th08.js `font` face) ordered for the
//! pack's `[fonts]`: the th08 face first, then each other declared font that adds glyphs the text still lacks; the
//! system font stays the last fallback at run time. Also the per-script glyph report.

use std::collections::BTreeMap;

use crate::thpatch::FontDecl;

// A declared font that parsed: its names (ttf name table IDs 1, 4, 16) and licence fields (0, 13, 14).
pub struct FontInfo {
    pub decl: FontDecl,
    pub bytes: Vec<u8>,
    pub families: Vec<String>,
    pub copyright: String,
    pub licence: String,
    // A licence that allows redistribution is named in the full name fields (see `open_licence`).
    pub open: bool,
}

impl FontInfo {
    // `OPEN` licence named in the font's full copyright + licence fields; anything else (e.g. Adobe's commercial
    // MyriadPro) is the user's own download only.
    pub fn open_licence(&self) -> bool {
        self.open
    }
}

fn has(face: &ttf_parser::Face<'_>, c: char) -> bool {
    face.glyph_index(c).is_some_and(|g| g.0 != 0)
}

// Licences that allow redistribution (OFL, Apache, Ubuntu Font Licence, public domain, GPL with font exception).
const OPEN: [&str; 7] = [
    "open font license",
    "scripts.sil.org/ofl",
    "openfontlicense",
    "apache license",
    "ubuntu font",
    "public domain",
    "font exception",
];

pub fn load(decls: &[FontDecl]) -> (Vec<FontInfo>, Vec<String>) {
    let mut out = Vec::new();
    let mut bad = Vec::new();
    for d in decls {
        let Some(path) = &d.file else { continue };
        let Ok(bytes) = std::fs::read(path) else {
            bad.push(format!("{}: unreadable", d.name));
            continue;
        };
        let Ok(face) = ttf_parser::Face::parse(&bytes, 0) else {
            bad.push(format!("{}: not a parsable font", d.name));
            continue;
        };
        let name = |id: u16| -> Vec<String> {
            face.names()
                .into_iter()
                .filter(|n| n.name_id == id)
                .filter_map(|n| n.to_string())
                .collect()
        };
        let mut families = name(1);
        families.extend(name(16));
        families.extend(name(4));
        families.dedup();
        // One line, at most 160 characters (some fonts carry the whole licence text in name 13).
        let short = |s: String| -> String {
            let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
            if one.chars().count() > 160 {
                format!("{}…", one.chars().take(160).collect::<String>())
            } else {
                one
            }
        };
        let full = format!(
            "{} {} {}",
            name(0).join(" "),
            name(13).join(" "),
            name(14).join(" ")
        )
        .to_ascii_lowercase();
        let open = OPEN.iter().any(|k| full.contains(k));
        let copyright = short(name(0).into_iter().next().unwrap_or_default());
        let licence = short(format!(
            "{} {}",
            name(13).into_iter().next().unwrap_or_default(),
            name(14).into_iter().next().unwrap_or_default()
        ));
        out.push(FontInfo {
            decl: d.clone(),
            bytes,
            families,
            copyright,
            licence,
            open,
        });
    }
    (out, bad)
}

// Unicode script of a character, coarse (the scripts thpatch languages use); RTL ones are flagged by `rtl`.
pub fn script_of(c: char) -> &'static str {
    match u32::from(c) {
        0x0000..=0x024F | 0x1E00..=0x1EFF | 0x2C60..=0x2C7F | 0xA720..=0xA7FF => "Latin",
        0x0370..=0x03FF | 0x1F00..=0x1FFF => "Greek",
        0x0400..=0x052F | 0x1C80..=0x1C8F | 0x2DE0..=0x2DFF | 0xA640..=0xA69F => "Cyrillic",
        0x0530..=0x058F => "Armenian",
        0x0590..=0x05FF | 0xFB1D..=0xFB4F => "Hebrew",
        0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => {
            "Arabic"
        }
        0x0900..=0x097F => "Devanagari",
        0x0980..=0x09FF => "Bengali",
        0x0E00..=0x0E7F => "Thai",
        0x10A0..=0x10FF => "Georgian",
        0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF => "Hangul",
        0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9F => "Kana",
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF => "Han",
        0x2000..=0x2BFF | 0x3000..=0x303F | 0xFF00..=0xFF65 | 0xFFE0..=0xFFEF => "Common",
        _ => "Other",
    }
}

pub fn rtl(script: &str) -> bool {
    matches!(script, "Arabic" | "Hebrew")
}

// The chosen chain (indices into the fonts) and, per character, the first chain font that has it.
pub struct Chain {
    pub fonts: Vec<usize>,
    pub reason: Vec<String>,
    pub covered: BTreeMap<char, Option<usize>>,
}

fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

// Chain for the characters `chars`: the font whose family is `face` (th08.js) first, then every other font, in
// declaration order, that covers a character no earlier chain font has.
pub fn choose(fonts: &[FontInfo], face: Option<&str>, chars: &[char]) -> Chain {
    let parsed: Vec<Option<ttf_parser::Face<'_>>> = fonts
        .iter()
        .map(|f| ttf_parser::Face::parse(&f.bytes, 0).ok())
        .collect();
    let mut order: Vec<usize> = (0..fonts.len()).collect();
    let face_idx = face.and_then(|want| {
        let w = norm(want);
        fonts
            .iter()
            .position(|f| f.families.iter().any(|n| norm(n) == w))
    });
    if let Some(i) = face_idx {
        order.retain(|&j| j != i);
        order.insert(0, i);
    }
    let mut chain = Chain {
        fonts: Vec::new(),
        reason: Vec::new(),
        covered: chars.iter().map(|&c| (c, None)).collect(),
    };
    for i in order {
        let Some(face) = &parsed[i] else { continue };
        let newly: Vec<char> = chain
            .covered
            .iter()
            .filter(|(c, k)| k.is_none() && has(face, **c))
            .map(|(c, _)| *c)
            .collect();
        if newly.is_empty() && Some(i) != face_idx {
            continue;
        }
        let k = chain.fonts.len();
        for c in &newly {
            chain.covered.insert(*c, Some(k));
        }
        chain.reason.push(if Some(i) == face_idx {
            format!("th08.js face, {} glyphs", newly.len())
        } else {
            format!("adds {} glyphs", newly.len())
        });
        chain.fonts.push(i);
    }
    chain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts() {
        assert_eq!(script_of('a'), "Latin");
        assert_eq!(script_of('ё'), "Cyrillic");
        assert_eq!(script_of('ب'), "Arabic");
        assert!(rtl(script_of('ש')));
        assert_eq!(script_of('漢'), "Han");
        assert_eq!(script_of('　'), "Common");
    }

    #[test]
    fn names_normalised() {
        assert_eq!(norm("PT Sans-Narrow"), norm("ptsans narrow"));
    }
}
