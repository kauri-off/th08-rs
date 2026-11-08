//! One line-breaking pass for Japanese UI text: BudouX phrases and kinsoku through invisible break controls.
use crate::theme;

const JOIN: char = '\u{2060}';
const BREAK: char = '\u{200b}';

// `text` with lines allowed to break only between `theme::pieces`; text without Japanese stays as it is.
pub fn breakable(text: &str) -> String {
    if !text.chars().any(theme::is_ja) {
        // The JP face centres its ellipsis; Latin and Cyrillic text wants it on the baseline.
        return text.replace('…', "...");
    }
    let mut out = String::with_capacity(text.len() * 2);
    for (n, piece) in theme::pieces(text).into_iter().enumerate() {
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
}
