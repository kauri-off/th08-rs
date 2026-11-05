//! th08.pak from th08.dat: every entry in table order, zstd frames, self-verified against `Archive::read`.

use std::time::{Duration, Instant};

use th08_formats::archive::Archive;
use th08_formats::pack::{self, Kind, Pack};

use crate::{Ctl, Error, Result, Stage};

pub struct Built {
    pub bytes: Vec<u8>,
    // Every entry's original bytes, in table order.
    pub data: Vec<(String, Vec<u8>)>,
    pub t_dat: Duration,
    pub t_enc: Duration,
}

impl Built {
    pub fn unpacked(&self) -> usize {
        self.data.iter().map(|d| d.1.len()).sum()
    }
}

// th08.dat bytes → archive; anything else is not game data.
pub fn archive(dat: Vec<u8>) -> Result<Archive> {
    Archive::from_bytes(dat).map_err(|e| {
        Error::NotGameData(format!(
            "th08.dat is not an Imperishable Night archive ({e})"
        ))
    })
}

// zstd frame of `raw`; window covers a whole frame so solid frames match across files.
#[cfg(feature = "zstd")]
pub fn zstd_frame(raw: &[u8], level: i32) -> std::io::Result<Vec<u8>> {
    let mut c = zstd::bulk::Compressor::new(level)?;
    let wlog = (usize::BITS - raw.len().max(1 << 10).leading_zeros()).clamp(10, 27);
    c.set_parameter(zstd::zstd_safe::CParameter::WindowLog(wlog))?;
    c.include_checksum(true)?;
    c.compress(raw)
}

// The pack bytes of `ar`, frames of at most `frame_limit` decoded bytes compressed by `compress`; checked by `verify`.
pub fn build(
    ar: &Archive,
    frame_limit: usize,
    compress: &mut dyn FnMut(&[u8]) -> std::io::Result<Vec<u8>>,
    ctl: Ctl,
) -> Result<Built> {
    let t = Instant::now();
    let data: Vec<(String, Vec<u8>)> = ar
        .names()
        .iter()
        .map(|n| {
            ctl.check()?;
            let b = ar
                .read(n)
                .map_err(|e| Error::NotGameData(format!("th08.dat: {n}: {e}")))?;
            Ok((n.clone(), b))
        })
        .collect::<Result<_>>()?;
    let t_dat = t.elapsed();
    let files: Vec<(&str, &[u8])> = data
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    let total: usize = data.iter().map(|d| d.1.len()).sum();
    let t = Instant::now();
    let mut err = None;
    let mut done = 0;
    ctl.report(Stage::Content, 0, total as u64);
    let bytes = pack::write(Kind::Content, &files, frame_limit, &mut |b: &[u8]| {
        if err.is_some() {
            return b.to_vec();
        }
        if let Err(e) = ctl.check() {
            err = Some(e);
            return b.to_vec();
        }
        let z = compress(b).unwrap_or_else(|e| {
            err = Some(e.into());
            b.to_vec()
        });
        done += b.len();
        ctl.report(Stage::Content, done as u64, total as u64);
        z
    });
    if let Some(e) = err {
        return Err(e);
    }
    let t_enc = t.elapsed();
    let built = Built {
        bytes,
        data,
        t_dat,
        t_enc,
    };
    verify(&built.bytes, &built.data)?;
    Ok(built)
}

// Self-verify: same names in order, every entry = the original bytes, manifest hashes match.
pub fn verify(bytes: &[u8], data: &[(String, Vec<u8>)]) -> Result<Pack> {
    let p =
        Pack::from_bytes(bytes.to_vec()).map_err(|e| Error::Failed(format!("th08.pak: {e}")))?;
    let fail = |m: String| Err(Error::Failed(format!("th08.pak self-check: {m}")));
    if p.names().len() != data.len() || p.names().iter().zip(data).any(|(a, b)| *a != b.0) {
        return fail("entry names / order differ".into());
    }
    for (e, (n, b)) in p.entries().iter().zip(data) {
        if e.sha256 != pack::sha256(b) {
            return fail(format!("{n}: manifest hash"));
        }
        if p.read(n).ok().as_ref() != Some(b) {
            return fail(format!("{n}: decoded bytes differ"));
        }
    }
    Ok(p)
}

// A th08.pak to take as it is: a content pack whose every entry reads back against its hash.
pub fn check_pack(bytes: Vec<u8>, ctl: Ctl) -> Result<Pack> {
    let p = Pack::from_bytes(bytes)
        .map_err(|e| Error::NotGameData(format!("th08.pak is not a port content pack ({e})")))?;
    if p.kind() != Kind::Content {
        return Err(Error::NotGameData("th08.pak is a BGM pack".into()));
    }
    let n = p.names().len() as u64;
    for (i, name) in p.names().iter().enumerate() {
        ctl.check()?;
        p.read(name)
            .map_err(|e| Error::NotGameData(format!("th08.pak: {e}")))?;
        ctl.report(Stage::Content, i as u64 + 1, n);
    }
    Ok(p)
}

// Refuses a pack whose Logic entries are not the vanilla th08.dat ones.
pub fn require_vanilla(p: &Pack) -> Result<()> {
    let diff = th08_core::content::vanilla_mismatch(p);
    if diff.is_empty() {
        Ok(())
    } else {
        Err(Error::Version(diff))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn sample_dat() -> Vec<u8> {
        let a: Vec<u8> = (0..5000u32).map(|i| (i * 7 % 13) as u8).collect();
        let b = b"ECL".repeat(300);
        th08_formats::archive::pack(&[
            ("Stage1.anm", &a),
            ("ecldata1.ecl", &b),
            ("x.png", &[1, 2, 3]),
        ])
    }

    #[test]
    fn builds_and_verifies() {
        let ar = archive(sample_dat()).unwrap();
        let b = build(&ar, 1 << 20, &mut |r| zstd_frame(r, 3), Ctl::default()).unwrap();
        let p = verify(&b.bytes, &b.data).unwrap();
        assert_eq!(p.read("stage1.anm").unwrap().len(), 5000);
        assert!(matches!(require_vanilla(&p), Err(Error::Version(_))));
        assert!(check_pack(b.bytes.clone(), Ctl::default()).is_ok());
        let mut bad = b.bytes;
        bad[pack::HEADER_LEN] ^= 0xFF;
        assert!(matches!(
            check_pack(bad, Ctl::default()),
            Err(Error::NotGameData(_))
        ));
    }

    #[test]
    fn refuses_other_files() {
        assert!(matches!(
            archive(b"RIFF....WAVE".to_vec()),
            Err(Error::NotGameData(_))
        ));
        assert!(matches!(
            check_pack(vec![0; 64], Ctl::default()),
            Err(Error::NotGameData(_))
        ));
    }

    #[test]
    fn cancel_stops() {
        let ar = archive(sample_dat()).unwrap();
        let c = std::sync::atomic::AtomicBool::new(true);
        let r = build(
            &ar,
            1 << 20,
            &mut |r| zstd_frame(r, 3),
            Ctl::new(&c, &|_, _, _| {}),
        );
        assert!(matches!(r, Err(Error::Cancelled)));
    }
}
