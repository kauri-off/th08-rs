//! Core content views (ARCH_B_MODDING §1.2, decisions D1 / D5): logic reads game data only through `LogicView`,
//! presentation-only data (pixels, samples, MIDI) through `PresentView`; both over the th08.dat archive (01 §2–§7).

use th08_formats::archive::Archive;

// Logic-content fingerprint (ARCH_B D5): hash over the sorted (key, bytes) of the Logic-layer overrides.
// Vanilla has no overrides, so its fingerprint is empty; written into session metadata (`meta.txt`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fingerprint(Vec<u8>);

impl Fingerprint {
    pub fn is_vanilla(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

// Every logic-relevant content read (.ecl, .std, .sht, .anm, msg, .rpy, .ver, .fmt …); today the vanilla archive.
pub struct LogicView {
    archive: Archive,
    fingerprint: Fingerprint,
}

impl LogicView {
    pub fn new(archive: Archive) -> Self {
        Self {
            archive,
            fingerprint: Fingerprint::default(),
        }
    }

    // Archive lookup by bare name, case-insensitive (01 §5).
    pub fn read(&self, name: &str) -> th08_formats::archive::Result<Vec<u8>> {
        self.archive.read(name)
    }

    pub fn fingerprint(&self) -> &Fingerprint {
        &self.fingerprint
    }

    // The presentation layers over the same content (today: the same archive).
    pub fn present(&self) -> PresentView<'_> {
        PresentView(&self.archive)
    }
}

// Presentation-only reads (jpg surfaces, se_*.wav, .mid); a later presentation pack may replace them freely.
#[derive(Clone, Copy)]
pub struct PresentView<'a>(&'a Archive);

impl PresentView<'_> {
    pub fn read(&self, name: &str) -> th08_formats::archive::Result<Vec<u8>> {
        self.0.read(name)
    }

    pub fn names(&self) -> &[String] {
        self.0.names()
    }
}
