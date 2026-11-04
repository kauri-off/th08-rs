//! th08 file formats: pure parsers/writers of the original data files (rewrite step 2, REWRITE_PLAN.md).
//! Bytes in, plain structs out; no IO, no game state, no rendering. Game text stays Shift-JIS bytes (`sjis`).
#![deny(clippy::disallowed_methods)]
#![forbid(unsafe_code)]

pub mod anm;
pub mod archive;
pub mod cfg;
pub mod ecl;
pub mod end;
pub mod img;
pub mod midi;
pub mod msg;
pub mod musiccmt;
pub mod pack;
pub mod reader;
pub mod replay;
pub mod scoredat;
pub mod sht;
pub mod sjis;
pub mod stdfile;
pub mod thbgm;

#[cfg(test)]
mod testdata;

// Random bytes a writer draws (Rng_Range16(256) in the original savers, 15 §2.1, §3.6), in call order.
pub trait ByteSource {
    fn next_byte(&mut self) -> u8;
}
