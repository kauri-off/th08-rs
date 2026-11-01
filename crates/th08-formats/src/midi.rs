//! Standard MIDI file container as `Midi_Open` 0x4442E0 reads it (02 §5); the sequencer is the game's (th08 midi.rs).

use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum MidiError {
    #[error("not a MIDI file")]
    NotMidi(#[source] midly::Error),
    #[error("SMPTE timing is not used by th08")]
    Smpte,
    #[error(transparent)]
    Track(midly::Error),
    #[error("division 0")]
    Division0,
}

// Midi_Open 0x4442E0: header fields and every `MTrk` body copied into its own buffer.
#[derive(Clone, Debug)]
pub struct Song {
    pub format: u16,
    pub division: u16,
    pub tracks: Vec<Arc<[u8]>>,
}

pub fn parse(file: &[u8]) -> Result<Song, MidiError> {
    let (header, tracks) = midly::parse(file).map_err(MidiError::NotMidi)?;
    let midly::Timing::Metrical(division) = header.timing else {
        return Err(MidiError::Smpte);
    };
    let format = match header.format {
        midly::Format::SingleTrack => 0,
        midly::Format::Parallel => 1,
        midly::Format::Sequential => 2,
    };
    let tracks = tracks
        .map(|t| t.map(|e| Arc::<[u8]>::from(e.unread())))
        .collect::<Result<Vec<_>, _>>()
        .map_err(MidiError::Track)?;
    if division.as_int() == 0 {
        return Err(MidiError::Division0);
    }
    Ok(Song {
        format,
        division: division.as_int(),
        tracks,
    })
}
