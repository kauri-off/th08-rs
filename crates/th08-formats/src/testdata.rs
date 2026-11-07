//! Test-only access to the original th08.dat (TH08_DIR); tests that need it skip without it.

use crate::archive::Archive;
use th08_testkit::game_dir;

// Case-insensitive file lookup in the game directory.
pub fn game_file(name: &str) -> Option<Vec<u8>> {
    let dir = game_dir()?;
    let path = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map_or(dir.join(name), |e| e.path());
    std::fs::read(path).ok()
}

pub fn archive() -> Option<Archive> {
    game_dir()?;
    let bytes = game_file("th08.dat").expect("TH08_DIR set but th08.dat unreadable");
    Some(Archive::from_bytes(bytes).expect("th08.dat"))
}

// Counting byte source standing in for the game RNG.
pub struct Counter(pub u8, pub usize);

impl crate::ByteSource for Counter {
    fn next_byte(&mut self) -> u8 {
        self.1 += 1;
        self.0 = self.0.wrapping_mul(17).wrapping_add(5);
        self.0
    }
}
