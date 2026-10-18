// .sht player shot files (09 §2.1).
use anyhow::{Result, bail};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShotEntry {
    pub interval: i16,
    pub phase: i16,
    pub offset: [f32; 2],
    pub size: [f32; 2],
    pub angle: f32,
    pub speed: f32,
    pub damage: i16,
    pub side: i16,
    pub origin: i16,
    pub kind: i16,
    pub script: i16,
    pub sound: i16,
    pub init: i32,
    pub update: i32,
    pub draw: i32,
    pub hit: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    pub threshold: i32,
    pub entries: Vec<ShotEntry>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sht {
    pub bombs: f32,
    pub deathbomb: i32,
    pub hitbox: f32,
    pub graze: f32,
    pub attract_speed: f32,
    pub collect: f32,
    pub poc_line: f32,
    pub unknown: i32,
    pub speed: f32,
    pub speed_focus: f32,
    pub diag: f32,
    pub diag_focus: f32,
    pub fall: f32,
    pub levels: Vec<Level>,
}

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
    // First level whose threshold > trunc(power); `forced` picks a bomb level (shot types 2/9).
    pub fn level(&self, power: f32, forced: Option<usize>) -> Option<&Level> {
        if let Some(k) = forced.filter(|&k| k < self.levels.len()) {
            return self.levels.get(k);
        }
        let p = power as i32;
        self.levels.iter().find(|l| l.threshold > p)
    }
}

pub const ENTRY_SIZE: usize = 0x38;

pub fn parse(b: &[u8]) -> Result<Sht> {
    let rd = |o: usize, n: usize| {
        b.get(o..o + n)
            .ok_or_else(|| anyhow::anyhow!("sht: truncated at {o:#x}"))
    };
    let f = |o: usize| -> Result<f32> { Ok(f32::from_le_bytes(rd(o, 4)?.try_into()?)) };
    let i = |o: usize| -> Result<i32> { Ok(i32::from_le_bytes(rd(o, 4)?.try_into()?)) };
    let h = |o: usize| -> Result<i16> { Ok(i16::from_le_bytes(rd(o, 2)?.try_into()?)) };
    let n = u16::from_le_bytes(rd(2, 2)?.try_into()?) as usize;
    let mut levels = Vec::with_capacity(n);
    for k in 0..n {
        let off = i(0x38 + k * 8)? as usize;
        let threshold = i(0x3C + k * 8)?;
        let mut entries = Vec::new();
        let mut o = off;
        loop {
            if i(o)? == -1 {
                break;
            }
            entries.push(ShotEntry {
                interval: h(o)?,
                phase: h(o + 2)?,
                offset: [f(o + 4)?, f(o + 8)?],
                size: [f(o + 0xC)?, f(o + 0x10)?],
                angle: f(o + 0x14)?,
                speed: f(o + 0x18)?,
                damage: h(o + 0x1C)?,
                side: h(o + 0x1E)?,
                origin: h(o + 0x20)?,
                kind: h(o + 0x22)?,
                script: h(o + 0x24)?,
                sound: h(o + 0x26)?,
                init: i(o + 0x28)?,
                update: i(o + 0x2C)?,
                draw: i(o + 0x30)?,
                hit: i(o + 0x34)?,
            });
            o += ENTRY_SIZE;
            if entries.len() > 4096 {
                bail!("sht: unterminated level {k}");
            }
        }
        levels.push(Level { threshold, entries });
    }
    Ok(Sht {
        bombs: f(4)?,
        deathbomb: i(8)?,
        hitbox: f(0xC)?,
        graze: f(0x10)?,
        attract_speed: f(0x14)?,
        collect: f(0x18)?,
        poc_line: f(0x1C)?,
        unknown: i(0x20)?,
        speed: f(0x24)?,
        speed_focus: f(0x28)?,
        diag: f(0x2C)?,
        diag_focus: f(0x30)?,
        fall: f(0x34)?,
        levels,
    })
}

pub fn team(shot: u8) -> u8 {
    if shot < 4 { shot } else { (shot - 4) / 2 }
}

// Files per shot type 0–11 (09 §2): (anm, human sht, youkai sht).
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
        let Some(ar) = crate::archive::test_archive() else {
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
                if std::env::var("SHT_DUMP").is_ok() {
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
