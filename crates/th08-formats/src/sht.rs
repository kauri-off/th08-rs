//! .sht player shot files: header, level table, shot entries (09 §2.1).

use crate::reader::{Reader, Truncated};

#[derive(Debug, thiserror::Error)]
pub enum ShtError {
    #[error("sht: {0}")]
    Truncated(#[from] Truncated),
    #[error("sht: unterminated level {0}")]
    Unterminated(usize),
}

pub type Result<T> = std::result::Result<T, ShtError>;

// Shot entry, 0x38 bytes; the list ends with i32 −1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShotEntry {
    // +0x00 / +0x02: fires when shot timer % interval == phase (laser entries: phase = laser slot).
    pub interval: i16,
    pub phase: i16,
    // +0x04 spawn offset, +0x0C hitbox w/h, px.
    pub offset: [f32; 2],
    pub size: [f32; 2],
    // +0x14 radians, +0x18 px/frame.
    pub angle: f32,
    pub speed: f32,
    pub damage: i16,
    // +0x1E: > 0 youkai-side (red at youkai max), < 0 human-side (time orbs at human max).
    pub side: i16,
    // +0x20: 0 player, k = option k−1.
    pub origin: i16,
    // +0x22: 3 piercing, 4/5 laser, 6 no hit animation.
    pub kind: i16,
    // +0x24 ANM script − 10 (bullet +10, hit animation +11).
    pub script: i16,
    // +0x26 sfx id, −1 none.
    pub sound: i16,
    // +0x28.. function indexes: init 0x4C7EE0, update 0x4C7F04, draw 0x4C7F1C, hit 0x4C7F24.
    pub init: i32,
    pub update: i32,
    pub draw: i32,
    pub hit: i32,
}

// Level table record +0x38 + 8k: file offset, power threshold.
#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    pub threshold: i32,
    pub entries: Vec<ShotEntry>,
}

#[derive(Clone, Debug, PartialEq)]
// Header, 0x38 bytes.
pub struct Sht {
    // +4 bombs per life (truncated on respawn).
    pub bombs: f32,
    // +8 deathbomb base / bomb-allowed counter.
    pub deathbomb: i32,
    // +0xC / +0x10 / +0x18 hitbox, graze and item box sizes, px.
    pub hitbox: f32,
    pub graze: f32,
    // +0x14 item attraction speed.
    pub attract_speed: f32,
    pub collect: f32,
    // +0x1C point-of-collection line y.
    pub poc_line: f32,
    // +0x20 not referenced by the original.
    pub unknown: i32,
    // +0x24 unfocused, +0x28 focused speed; +0x2C / +0x30 diagonal components.
    pub speed: f32,
    pub speed_focus: f32,
    pub diag: f32,
    pub diag_focus: f32,
    // +0x34 item fall multiplier (08 §8.4).
    pub fall: f32,
    pub levels: Vec<Level>,
}

// Port-only defaults (ply00a values) for tests without data.
impl Default for Sht {
    fn default() -> Self {
        Self {
            bombs: 3.0,
            deathbomb: 18,
            hitbox: 1.65,
            graze: 2.8,
            attract_speed: 10.0,
            collect: 24.0,
            poc_line: 128.0,
            unknown: 0,
            speed: 4.0,
            speed_focus: 2.0,
            diag: 4.0 / 2f32.sqrt(),
            diag_focus: 2.0 / 2f32.sqrt(),
            fall: 0.9,
            levels: Vec::new(),
        }
    }
}

impl Sht {
    // First level whose threshold > trunc(power); `forced` picks bomb level 6/7 (shot types 2/9).
    pub fn level(&self, power: f32, forced: Option<usize>) -> Option<&Level> {
        self.level_index(power, forced).map(|k| &self.levels[k])
    }

    pub fn level_index(&self, power: f32, forced: Option<usize>) -> Option<usize> {
        if let Some(k) = forced.filter(|&k| k < self.levels.len()) {
            return Some(k);
        }
        let p = power as i32;
        self.levels.iter().position(|l| l.threshold > p)
    }
}

pub const ENTRY_SIZE: usize = 0x38;

pub fn parse(b: &[u8]) -> Result<Sht> {
    let n = usize::from(Reader::at(b, 2).u16()?);
    let mut levels = Vec::with_capacity(n);
    for k in 0..n {
        let mut t = Reader::at(b, 0x38 + k * 8);
        let mut o = t.u32()? as usize;
        let threshold = t.i32()?;
        let mut entries = Vec::new();
        loop {
            if Reader::at(b, o).i32()? == -1 {
                break;
            }
            let mut r = Reader::at(b, o);
            entries.push(ShotEntry {
                interval: r.i16()?,
                phase: r.i16()?,
                offset: [r.f32()?, r.f32()?],
                size: [r.f32()?, r.f32()?],
                angle: r.f32()?,
                speed: r.f32()?,
                damage: r.i16()?,
                side: r.i16()?,
                origin: r.i16()?,
                kind: r.i16()?,
                script: r.i16()?,
                sound: r.i16()?,
                init: r.i32()?,
                update: r.i32()?,
                draw: r.i32()?,
                hit: r.i32()?,
            });
            o += ENTRY_SIZE;
            // Port-only guard.
            if entries.len() > 4096 {
                return Err(ShtError::Unterminated(k));
            }
        }
        levels.push(Level { threshold, entries });
    }
    let mut r = Reader::at(b, 4);
    Ok(Sht {
        bombs: r.f32()?,
        deathbomb: r.i32()?,
        hitbox: r.f32()?,
        graze: r.f32()?,
        attract_speed: r.f32()?,
        collect: r.f32()?,
        poc_line: r.f32()?,
        unknown: r.i32()?,
        speed: r.f32()?,
        speed_focus: r.f32()?,
        diag: r.f32()?,
        diag_focus: r.f32()?,
        fall: r.f32()?,
        levels,
    })
}

// Team / character-pair index 0–3 for file names (09 §2).
pub fn team(shot: u8) -> u8 {
    if shot < 4 { shot } else { (shot - 4) / 2 }
}

// Files per shot type 0–11 (09 §2): (anm, human sht, youkai sht); solos use one file for both.
pub fn files(shot: u8) -> (String, String, String) {
    let team = team(shot);
    let anm = format!("player{team:02}.anm");
    let (a, s) = (format!("ply{team:02}a.sht"), format!("ply{team:02}as.sht"));
    match shot {
        0..=3 => (anm, a, s),
        _ if shot.is_multiple_of(2) => (anm, a.clone(), a),
        _ => (anm, s.clone(), s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_sht_files() {
        let Some(ar) = crate::testdata::archive() else {
            return;
        };
        for t in 0..4 {
            for suf in ["a", "as"] {
                let name = format!("ply{t:02}{suf}.sht");
                let s = parse(&ar.read(&name).unwrap()).unwrap();
                let n = if name == "ply02as.sht" { 8 } else { 6 };
                assert_eq!(s.levels.len(), n, "{name}");
                assert_eq!(s.bombs, 3.0);
                assert!(s.levels.iter().all(|l| !l.entries.is_empty()), "{name}");
                for l in &s.levels {
                    for e in &l.entries {
                        assert!(
                            (0..=8).contains(&e.init)
                                && [0, 1, 3, 4, 5].contains(&e.update)
                                && (0..=1).contains(&e.draw)
                                && (0..=2).contains(&e.hit),
                            "{name} {e:?}"
                        );
                        assert!(
                            e.interval > 0 && (0..=4).contains(&e.origin),
                            "{name} {e:?}"
                        );
                    }
                }
                if std::env::var("SHT_DUMP")
                    .is_ok_and(|v| !matches!(v.trim(), "0" | "false" | "off" | "no"))
                {
                    eprintln!(
                        "{name}: {:?}",
                        Sht {
                            levels: vec![],
                            ..s.clone()
                        }
                    );
                    for (k, l) in s.levels.iter().enumerate() {
                        eprintln!(" level {k} thr {}", l.threshold);
                        for e in &l.entries {
                            eprintln!("  {e:?}");
                        }
                    }
                }
            }
        }
        assert_eq!(files(5).1, "ply00as.sht");
        assert_eq!(files(10).0, "player03.anm");
    }
}
