//! Line breaking of UI text: BudouX phrases, kinsoku, whole words, and the invisible break controls for toolkits.
use std::borrow::Cow;

// Kana, CJK ideographs and full-width forms: the characters Japanese line breaking applies to.
pub fn is_ja(c: char) -> bool {
    matches!(u32::from(c), 0x3000..=0x30ff | 0x3400..=0x9fff | 0xf900..=0xfaff | 0xff00..=0xffef)
}

// Characters a line never starts with, and never ends with (kinsoku shori).
fn no_start(c: char) -> bool {
    "、。，．・：；？！ー）」』】〕〉》｝’”…‥ぁぃぅぇぉっゃゅょゎァィゥェォッャュョヮヵヶ々%％)]},.!?:;".contains(c)
}

fn no_end(c: char) -> bool {
    "（「『【〔〈《｛‘“([{".contains(c)
}

// The pieces a line never breaks inside, in order, each with its trailing spaces: Japanese phrases (BudouX, then
// kinsoku), and every other word whole.
pub fn pieces(text: &str) -> Vec<&str> {
    thread_local! {
        static JA: budoux_phf_rs::Parser = budoux_phf_rs::Parser::japanese_parser();
    }
    let mut cuts = vec![0usize];
    let mut i = 0;
    for (at, c) in text.char_indices() {
        if at < i {
            continue;
        }
        if c.is_whitespace() {
            let end = text[at..]
                .find(|c: char| !c.is_whitespace())
                .map_or(text.len(), |e| at + e);
            if end < text.len() {
                cuts.push(end);
            }
            i = end;
            continue;
        }
        let end = text[at..]
            .find(char::is_whitespace)
            .map_or(text.len(), |e| at + e);
        let word = &text[at..end];
        if word.chars().any(is_ja) {
            let mut off = at;
            JA.with(|j| {
                j.parse_with(word, |chunk| {
                    if off > at {
                        cuts.push(off);
                    }
                    off += chunk.len();
                });
            });
        }
        i = end;
    }
    cuts.push(text.len());
    cuts.dedup();
    // Kinsoku: no cut before a closing mark or small kana, none after an opening bracket.
    cuts.retain(|&c| {
        c == 0
            || c == text.len()
            || !(text[c..].chars().next().is_some_and(no_start)
                || text[..c].chars().next_back().is_some_and(no_end))
    });
    cuts.windows(2).map(|w| &text[w[0]..w[1]]).collect()
}

// The pieces a label never breaks inside, without their spaces.
pub fn unbreakable(text: &str) -> Vec<&str> {
    pieces(text).into_iter().map(str::trim_end).collect()
}

// The JP face centres its ellipsis; Latin and Cyrillic text wants it on the baseline.
pub fn latin_dots(text: &str) -> Cow<'_, str> {
    if text.contains('…') && !text.chars().any(is_ja) {
        Cow::Owned(text.replace('…', "..."))
    } else {
        Cow::Borrowed(text)
    }
}

const JOIN: char = '\u{2060}';
const BREAK: char = '\u{200b}';

// `text` with lines allowed to break only between `pieces`; text without Japanese stays as it is.
pub fn breakable(text: &str) -> String {
    if !text.chars().any(is_ja) {
        return latin_dots(text).into_owned();
    }
    let mut out = String::with_capacity(text.len() * 2);
    for (n, piece) in pieces(text).into_iter().enumerate() {
        if n > 0 && !out.ends_with(char::is_whitespace) {
            out.push(BREAK);
        }
        let mut prev = None;
        for c in piece.chars() {
            if prev.is_some_and(|p: char| !p.is_whitespace() && !c.is_whitespace()) {
                out.push(JOIN);
            }
            out.push(c);
            prev = Some(c);
        }
    }
    out
}

// `s` without the controls `breakable` adds.
pub fn plain(s: &str) -> String {
    s.replace([JOIN, BREAK], "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_is_untouched() {
        assert_eq!(breakable("Master volume"), "Master volume");
    }

    #[test]
    fn japanese_round_trips_and_marks_phrases() {
        let t = "ゲーム本来のBGM音量はそのままです。";
        let b = breakable(t);
        assert_eq!(plain(&b), t);
        assert!(b.contains(BREAK), "{b:?}");
        assert!(!b.contains(&format!("{BREAK}。")), "kinsoku: {b:?}");
    }

    #[test]
    fn dots_only_outside_japanese() {
        assert_eq!(latin_dots("Quit…"), "Quit...");
        assert_eq!(latin_dots("終了…"), "終了…");
    }
}
