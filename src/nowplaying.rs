//! "Now playing" toast: notes BGM starts on their way to the sound device and names them by the music-room titles.

use std::sync::{Arc, Mutex};
use th08_core::game::AudioOut;
use th08_core::sound::{MidiCmd, SoundCmd, WavCmd};
use th08_core::textref::TextRef;

// The sound device with a note of the last BGM file started (basename without extension).
pub struct Watch {
    inner: Box<dyn AudioOut>,
    // MIDI slots loaded by name, so a slot start can be named.
    slots: Vec<Option<String>>,
    started: Arc<Mutex<Option<String>>>,
}

impl Watch {
    pub fn new(inner: Box<dyn AudioOut>) -> (Self, Started) {
        let started = Arc::new(Mutex::new(None));
        let w = Self {
            inner,
            slots: vec![None; 32],
            started: started.clone(),
        };
        (w, Started(started))
    }

    fn note(&self, name: &str) {
        if let Ok(mut s) = self.started.lock() {
            *s = Some(stem(name).to_owned());
        }
    }
}

// The app's end: the track started since the last `take`.
#[derive(Clone)]
pub struct Started(Arc<Mutex<Option<String>>>);

impl Started {
    pub fn take(&self) -> Option<String> {
        self.0.lock().ok().and_then(|mut s| s.take())
    }
}

fn stem(name: &str) -> &str {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    base.rsplit_once('.').map_or(base, |(s, _)| s)
}

impl AudioOut for Watch {
    fn load(&mut self, archive: th08_core::content::PresentView<'_>) {
        self.inner.load(archive);
    }
    fn midi_available(&mut self) -> bool {
        self.inner.midi_available()
    }
    fn exec(&mut self, c: SoundCmd) {
        match &c {
            SoundCmd::Wav(WavCmd::Play(t)) => self.note(&t.name),
            SoundCmd::Midi(MidiCmd::PlayFile(name)) => self.note(name),
            SoundCmd::Midi(MidiCmd::Load { slot, name }) => {
                if let Some(s) = self.slots.get_mut(*slot) {
                    *s = Some(name.clone());
                }
            }
            SoundCmd::Midi(MidiCmd::PlaySlot(slot)) => {
                if let Some(Some(name)) = self.slots.get(*slot) {
                    self.note(&name.clone());
                }
            }
            _ => {}
        }
        self.inner.exec(c);
    }
    fn is_started(&self) -> bool {
        self.inner.is_started()
    }
    fn ensure_started(&mut self) {
        self.inner.ensure_started();
    }
    fn set_state(&mut self, s: th08_core::game::AudioState) {
        self.inner.set_state(s);
    }
    fn set_muted(&mut self, m: bool) {
        self.inner.set_muted(m);
    }
}

// Track titles by file stem: the pack's translation of music-room entry i, else its Shift-JIS title.
pub fn titles(musiccmt: &[u8], pack: &th08_render::textdb::TextDb) -> Vec<(String, String)> {
    th08_core::musicroom::parse_musiccmt(musiccmt)
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let file = String::from_utf8_lossy(&e.file);
            let title = u8::try_from(i)
                .ok()
                .and_then(|track| pack.get(&TextRef::MusicTitle { track }))
                .map_or_else(|| e.title.decode(), str::to_owned);
            (stem(&file).to_owned(), title.trim().to_owned())
        })
        .collect()
}

pub fn title<'a>(titles: &'a [(String, String)], stem: &str) -> Option<&'a str> {
    titles
        .iter()
        .find(|(s, _)| s == stem)
        .map(|(_, t)| t.as_str())
        .filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stems_and_titles() {
        assert_eq!(stem("bgm/th08_00.mid"), "th08_00");
        assert_eq!(stem("th08_01.wav"), "th08_01");
        let bytes = b"@bgm/th08_00.mid\nOne \nNo.1\n@bgm/th08_01.mid\nTwo\nNo.2\n";
        let mut db = th08_render::textdb::TextDb::new();
        db.insert("music/1/title".into(), "Translated".into());
        let t = titles(bytes, &db);
        assert_eq!(title(&t, "th08_00"), Some("One"));
        assert_eq!(title(&t, "th08_01"), Some("Translated"));
        assert_eq!(title(&t, "th08_99"), None);
    }

    struct Null;

    impl AudioOut for Null {
        fn load(&mut self, _: th08_core::content::PresentView<'_>) {}
        fn midi_available(&mut self) -> bool {
            false
        }
        fn exec(&mut self, _: SoundCmd) {}
        fn is_started(&self) -> bool {
            true
        }
        fn ensure_started(&mut self) {}
    }

    #[test]
    fn watch_names_starts() {
        let (mut w, started) = Watch::new(Box::new(Null));
        w.exec(SoundCmd::Midi(MidiCmd::Load {
            slot: 3,
            name: "bgm/th08_05.mid".into(),
        }));
        assert_eq!(started.take(), None);
        w.exec(SoundCmd::Midi(MidiCmd::PlaySlot(3)));
        assert_eq!(started.take().as_deref(), Some("th08_05"));
        assert_eq!(started.take(), None);
    }
}
