//! score.dat (15 §2): the boot-loaded version list for ExeCheck and the port's save storage; the codec and records
//! are th08_formats::scoredat.

pub use th08_formats::scoredat::*;

// Supervisor+0x360 / +0x35C: the version list buffer (15 §2.10), kept for the whole run (Supervisor_OnRegister
// 0x4461A0; the archive copy is the same for every caller); unset → ExeCheck step 1 (pass).
#[derive(Clone, Debug, Default)]
pub struct VerList(Option<Vec<u8>>);

impl VerList {
    // Supervisor_OnRegister 0x4461A0: set once at boot.
    pub fn set(&mut self, list: Vec<u8>) {
        if self.0.is_none() {
            self.0 = Some(list);
        }
    }

    pub fn get(&self) -> Option<&[u8]> {
        self.0.as_deref()
    }

    // ExeCheck 0x44858D (15 §2.10) against the boot-loaded list: true = pass (0), false = fail (−1).
    pub fn exe_check(&self, tag: &[u8], size: u32, sum: u32) -> bool {
        exe_check_in(self.get(), tag, size, sum)
    }

    // ScoreDat_Load chunk walk (15 §2.1) with the boot-loaded version list.
    pub fn parse(&self, file: &[u8]) -> Result<ScoreDat> {
        ScoreDat::parse(file, self.get())
    }
}
