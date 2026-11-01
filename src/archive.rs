//! th08.dat "PBGZ" archive (01 §2–§7): th08_formats::archive, plus the test helper over TH08_DIR.

pub use th08_formats::archive::*;

#[cfg(test)]
pub fn test_archive() -> Option<Archive> {
    let dir = crate::settings::test_game_dir()?;
    let path = crate::assets::find_file(&dir, "th08.dat");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    Some(Archive::from_bytes(bytes).expect("th08.dat"))
}
