//! th08 audio output: the cpal mixer (`Audio: AudioOut`) executing the core's `SoundCmd`s and the SMF sequencer /
//! SoundFont synth for MIDI BGM (STEP10_CRATES.md §1).

pub mod audio;
pub mod bgm;
pub mod midi;
