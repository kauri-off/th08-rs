//! th08.pak: the port-only content container (docs/arch/ASSETS_FORMAT.md §3.1, not a spec format); each entry holds
//! the bytes `Archive::read` returns for that th08.dat entry (01 §5–§6), grouped into zstd frames.
//!
//! Layout (all LE): 32-byte header | frame data | manifest. Header: magic `T8PK`, version u32, kind u32, manifest
//! length u32, manifest offset u64, first 8 bytes of the manifest's sha256. Manifest: u32 frame count, per frame
//! (codec u8, offset u64, stored length u64, decoded size u64); u32 entry count, per entry (name length u16, name,
//! layer u8, frame u32, offset u64 and size u64 inside the decoded frame, sha256 of the entry's bytes).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};

use crate::reader::{Reader, Truncated};

pub const MAGIC: &[u8; 4] = b"T8PK";
pub const VERSION: u32 = 1;
pub const HEADER_LEN: usize = 32;
// Decoded frames kept for reuse; anmmgr reads each .anm twice in a row (logic, then THTX pixels).
const FRAME_CACHE: usize = 2;

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("not a T8PK pack")]
    NotPack,
    #[error("unsupported pack version {0}")]
    Version(u32),
    #[error("bad pack manifest: {0}")]
    BadManifest(&'static str),
    #[error("pack manifest hash mismatch")]
    ManifestHash,
    #[error("pack manifest: {0}")]
    Truncated(#[from] Truncated),
    #[error("{0} not in pack")]
    NotFound(String),
    #[error("pack frame {frame}: {msg}")]
    Decode { frame: u32, msg: String },
    #[error("{0}: decoded bytes do not match the manifest sha256")]
    Hash(String),
}

pub type Result<T> = std::result::Result<T, PackError>;

// Pack kind (header field): th08.dat content now; the BGM pack (AS2) later.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Content = 1,
    Bgm = 2,
}

// ARCH_B §1.2 layers: Logic entries feed the game state (and the fingerprint), Present ones only pixels/samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Logic = 0,
    Present = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    Store = 0,
    Zstd = 1,
}

// Layer of a th08.dat entry by name (ASSETS_FORMAT §1.1): jpg/png surfaces, se_*.wav and .mid are presentation-only;
// everything else (.anm, .ecl, .std, msg*.dat, .sht, .end, .rpy, .fmt, .ver, .txt) is read by logic.
pub fn layer_of(name: &str) -> Layer {
    let ext = name.rsplit_once('.').map_or("", |(_, e)| e);
    if ["jpg", "png", "wav", "mid"]
        .iter()
        .any(|p| ext.eq_ignore_ascii_case(p))
    {
        Layer::Present
    } else {
        Layer::Logic
    }
}

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub codec: Codec,
    // Absolute file offset and stored length of the frame's bytes; `size` = decoded length.
    pub offset: u64,
    pub len: u64,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    // Lower-case, as `Archive::names` (01 §5 lookups are case-insensitive).
    pub name: String,
    pub layer: Layer,
    pub frame: u32,
    pub offset: u64,
    pub size: u64,
    // sha256 of the original bytes (`Archive::read(name)`).
    pub sha256: [u8; 32],
}

pub struct Pack {
    data: Vec<u8>,
    kind: Kind,
    frames: Vec<Frame>,
    entries: Vec<Entry>,
    names: Vec<String>,
    index: HashMap<String, usize>,
    // Per entry: its decoded bytes were checked against `sha256` once.
    verified: Vec<AtomicBool>,
    cache: Mutex<Vec<(u32, Arc<Vec<u8>>)>>,
}

fn u64_of(r: &mut Reader) -> Result<u64> {
    Ok(u64::from_le_bytes(r.array()?))
}

fn usize_of(v: u64) -> Result<usize> {
    usize::try_from(v).map_err(|_| PackError::BadManifest("offset overflow"))
}

impl Pack {
    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        if data.len() < HEADER_LEN || &data[..4] != MAGIC {
            return Err(PackError::NotPack);
        }
        let mut h = Reader::at(&data, 4);
        let version = h.u32()?;
        if version != VERSION {
            return Err(PackError::Version(version));
        }
        let kind = match h.u32()? {
            1 => Kind::Content,
            2 => Kind::Bgm,
            _ => return Err(PackError::BadManifest("kind")),
        };
        let mlen = h.u32()? as usize;
        let moff = usize_of(u64_of(&mut h)?)?;
        let msha: [u8; 8] = h.array()?;
        let manifest = moff
            .checked_add(mlen)
            .and_then(|end| data.get(moff..end))
            .ok_or(PackError::BadManifest("manifest range"))?;
        if sha256(manifest)[..8] != msha {
            return Err(PackError::ManifestHash);
        }
        let mut r = Reader::new(manifest);
        let nframes = r.u32()? as usize;
        let mut frames = Vec::with_capacity(nframes.min(1 << 16));
        for _ in 0..nframes {
            let codec = match r.u8()? {
                0 => Codec::Store,
                1 => Codec::Zstd,
                _ => return Err(PackError::BadManifest("codec")),
            };
            let f = Frame {
                codec,
                offset: u64_of(&mut r)?,
                len: u64_of(&mut r)?,
                size: u64_of(&mut r)?,
            };
            let end = f.offset.checked_add(f.len);
            if f.offset < HEADER_LEN as u64 || end.is_none_or(|e| e > moff as u64) {
                return Err(PackError::BadManifest("frame range"));
            }
            if codec == Codec::Store && f.len != f.size {
                return Err(PackError::BadManifest("stored frame size"));
            }
            frames.push(f);
        }
        let nentries = r.u32()? as usize;
        let mut entries = Vec::with_capacity(nentries.min(1 << 16));
        for _ in 0..nentries {
            let n = r.u16()? as usize;
            let name = String::from_utf8_lossy(r.bytes(n)?).to_ascii_lowercase();
            let layer = match r.u8()? {
                0 => Layer::Logic,
                1 => Layer::Present,
                _ => return Err(PackError::BadManifest("layer")),
            };
            let e = Entry {
                name,
                layer,
                frame: r.u32()?,
                offset: u64_of(&mut r)?,
                size: u64_of(&mut r)?,
                sha256: r.array()?,
            };
            let f = frames
                .get(e.frame as usize)
                .ok_or(PackError::BadManifest("frame index"))?;
            if e.offset.checked_add(e.size).is_none_or(|end| end > f.size) {
                return Err(PackError::BadManifest("entry range"));
            }
            entries.push(e);
        }
        if r.pos() != manifest.len() {
            return Err(PackError::BadManifest("trailing bytes"));
        }
        let names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
        let index = names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i))
            .collect();
        let verified = entries.iter().map(|_| AtomicBool::new(false)).collect();
        Ok(Self {
            data,
            kind,
            frames,
            entries,
            names,
            index,
            verified,
            cache: Mutex::new(Vec::new()),
        })
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    // Entry names in manifest order (= th08.dat table order).
    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    pub fn entry(&self, name: &str) -> Option<&Entry> {
        self.index
            .get(&name.to_ascii_lowercase())
            .map(|&i| &self.entries[i])
    }

    // The entry's original bytes; the first read of each entry is checked against its manifest sha256.
    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        let i = *self
            .index
            .get(&name.to_ascii_lowercase())
            .ok_or_else(|| PackError::NotFound(name.to_string()))?;
        let e = &self.entries[i];
        let frame = self.frame(e.frame)?;
        // Ranges were validated against the frame's size in `from_bytes`, and `frame` checks the decoded length.
        let out = frame[e.offset as usize..(e.offset + e.size) as usize].to_vec();
        if !self.verified[i].load(Ordering::Relaxed) {
            if sha256(&out) != e.sha256 {
                return Err(PackError::Hash(e.name.clone()));
            }
            self.verified[i].store(true, Ordering::Relaxed);
        }
        Ok(out)
    }

    // Decoded frame `idx`, through the small most-recently-used cache.
    fn frame(&self, idx: u32) -> Result<Arc<Vec<u8>>> {
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(p) = cache.iter().position(|c| c.0 == idx) {
            let hit = cache.remove(p);
            cache.insert(0, hit.clone());
            return Ok(hit.1);
        }
        let f = &self.frames[idx as usize];
        let stored = &self.data[f.offset as usize..(f.offset + f.len) as usize];
        let size = usize_of(f.size)?;
        let bytes = match f.codec {
            Codec::Store => stored.to_vec(),
            Codec::Zstd => {
                let mut out = Vec::with_capacity(size);
                ruzstd::decoding::FrameDecoder::new()
                    .decode_all_to_vec(stored, &mut out)
                    .map_err(|e| PackError::Decode {
                        frame: idx,
                        msg: e.to_string(),
                    })?;
                out
            }
        };
        if bytes.len() != size {
            return Err(PackError::Decode {
                frame: idx,
                msg: format!("decoded {} bytes, manifest says {size}", bytes.len()),
            });
        }
        let bytes = Arc::new(bytes);
        cache.insert(0, (idx, bytes.clone()));
        cache.truncate(FRAME_CACHE);
        Ok(bytes)
    }
}

// Writer (repack tool and tests): files in order, grouped into frames by extension (first-appearance order), a frame
// closed before it would exceed `frame_limit` decoded bytes; a file never spans frames. `compress` makes one zstd
// frame; a frame that does not shrink is stored.
pub fn write(
    kind: Kind,
    files: &[(&str, &[u8])],
    frame_limit: usize,
    compress: &mut dyn FnMut(&[u8]) -> Vec<u8>,
) -> Vec<u8> {
    let ext = |n: &str| {
        n.rsplit_once('.')
            .map_or("", |(_, e)| e)
            .to_ascii_lowercase()
    };
    let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, (n, _)) in files.iter().enumerate() {
        let x = ext(n);
        match groups.iter_mut().find(|g| g.0 == x) {
            Some(g) => g.1.push(i),
            None => groups.push((x, vec![i])),
        }
    }
    // Frame plan: member file indices per frame.
    let mut plan: Vec<Vec<usize>> = Vec::new();
    for (_, members) in &groups {
        let mut cur: Vec<usize> = Vec::new();
        let mut cur_len = 0;
        for &i in members {
            let len = files[i].1.len();
            if !cur.is_empty() && cur_len + len > frame_limit {
                plan.push(std::mem::take(&mut cur));
                cur_len = 0;
            }
            cur.push(i);
            cur_len += len;
        }
        if !cur.is_empty() {
            plan.push(cur);
        }
    }
    let mut out = vec![0u8; HEADER_LEN];
    let mut frames = Vec::with_capacity(plan.len());
    let mut placed: Vec<(u32, u64)> = vec![(0, 0); files.len()];
    for (fi, members) in plan.iter().enumerate() {
        let mut raw = Vec::new();
        for &i in members {
            placed[i] = (fi as u32, raw.len() as u64);
            raw.extend_from_slice(files[i].1);
        }
        let z = compress(&raw);
        let (codec, stored) = if z.len() < raw.len() {
            (Codec::Zstd, z)
        } else {
            (Codec::Store, raw.clone())
        };
        frames.push(Frame {
            codec,
            offset: out.len() as u64,
            len: stored.len() as u64,
            size: raw.len() as u64,
        });
        out.extend_from_slice(&stored);
    }
    let mut m = Vec::new();
    m.extend_from_slice(&(frames.len() as u32).to_le_bytes());
    for f in &frames {
        m.push(f.codec as u8);
        m.extend_from_slice(&f.offset.to_le_bytes());
        m.extend_from_slice(&f.len.to_le_bytes());
        m.extend_from_slice(&f.size.to_le_bytes());
    }
    m.extend_from_slice(&(files.len() as u32).to_le_bytes());
    for (i, (n, bytes)) in files.iter().enumerate() {
        let name = n.to_ascii_lowercase();
        m.extend_from_slice(&(name.len() as u16).to_le_bytes());
        m.extend_from_slice(name.as_bytes());
        m.push(layer_of(&name) as u8);
        m.extend_from_slice(&placed[i].0.to_le_bytes());
        m.extend_from_slice(&placed[i].1.to_le_bytes());
        m.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        m.extend_from_slice(&sha256(bytes));
    }
    let moff = out.len() as u64;
    out.extend_from_slice(&m);
    let mut h = Vec::with_capacity(HEADER_LEN);
    h.extend_from_slice(MAGIC);
    h.extend_from_slice(&VERSION.to_le_bytes());
    h.extend_from_slice(&(kind as u32).to_le_bytes());
    h.extend_from_slice(&(m.len() as u32).to_le_bytes());
    h.extend_from_slice(&moff.to_le_bytes());
    h.extend_from_slice(&sha256(&m)[..8]);
    out[..HEADER_LEN].copy_from_slice(&h);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zfast(b: &[u8]) -> Vec<u8> {
        ruzstd::encoding::compress_to_vec(b, ruzstd::encoding::CompressionLevel::Fastest)
    }

    fn sample() -> Vec<(String, Vec<u8>)> {
        let mut v = Vec::new();
        for i in 0..5u8 {
            v.push((
                format!("stg{i}enm.anm"),
                vec![i; 3000 + usize::from(i) * 100],
            ));
            v.push((
                format!("ecldata{i}.ecl"),
                (0..700u32).map(|x| (x * 7 + u32::from(i)) as u8).collect(),
            ));
        }
        v.push((
            "Title00.PNG".into(),
            (0..997u32).map(|x| (x * 131 % 251) as u8).collect(),
        ));
        v.push(("empty.ver".into(), Vec::new()));
        v
    }

    fn files(s: &[(String, Vec<u8>)]) -> Vec<(&str, &[u8])> {
        s.iter().map(|(n, b)| (n.as_str(), b.as_slice())).collect()
    }

    #[test]
    fn roundtrip_and_layout() {
        let s = sample();
        let bytes = write(Kind::Content, &files(&s), 7000, &mut zfast);
        let p = Pack::from_bytes(bytes).unwrap();
        assert_eq!(p.kind(), Kind::Content);
        let lower: Vec<String> = s.iter().map(|f| f.0.to_ascii_lowercase()).collect();
        assert_eq!(p.names(), lower.as_slice());
        for (n, b) in &s {
            assert_eq!(&p.read(n).unwrap(), b, "{n}");
            assert_eq!(&p.read(&n.to_ascii_uppercase()).unwrap(), b);
        }
        // anm group (5 files, 3000..3400 bytes) split at 7000 bytes: frames of 2, 2, 1; files never split.
        let anm_frames: Vec<u32> = p
            .entries()
            .iter()
            .filter(|e| e.name.ends_with(".anm"))
            .map(|e| e.frame)
            .collect();
        assert_eq!(anm_frames, [0, 0, 1, 1, 2]);
        assert!(
            p.entries()
                .iter()
                .all(|e| e.offset + e.size <= p.frames()[e.frame as usize].size)
        );
        assert_eq!(p.entry("title00.png").unwrap().layer, Layer::Present);
        assert_eq!(p.entry("ecldata0.ecl").unwrap().layer, Layer::Logic);
        assert!(matches!(p.read("missing.ecl"), Err(PackError::NotFound(_))));
    }

    #[test]
    fn incompressible_frames_are_stored() {
        let s = sample();
        let bytes = write(Kind::Content, &files(&s), 1 << 20, &mut |b: &[u8]| {
            vec![0; b.len() + 1]
        });
        let p = Pack::from_bytes(bytes).unwrap();
        assert!(p.frames().iter().all(|f| f.codec == Codec::Store));
        for (n, b) in &s {
            assert_eq!(&p.read(n).unwrap(), b);
        }
    }

    #[test]
    fn corruption_is_detected() {
        let s = sample();
        let good = write(Kind::Content, &files(&s), 1 << 20, &mut |b: &[u8]| {
            b.to_vec()
        });
        assert!(matches!(
            Pack::from_bytes(good[..20].to_vec()),
            Err(PackError::NotPack)
        ));
        let mut v = good.clone();
        v[4] = 9;
        assert!(matches!(Pack::from_bytes(v), Err(PackError::Version(9))));
        let mut v = good.clone();
        let last = v.len() - 1;
        v[last] ^= 1;
        assert!(matches!(Pack::from_bytes(v), Err(PackError::ManifestHash)));
        // A stored payload byte flipped: the manifest still loads, the entry's first read fails its hash.
        let mut v = good.clone();
        v[HEADER_LEN] ^= 0xFF;
        let p = Pack::from_bytes(v).unwrap();
        assert!(matches!(p.read(&s[0].0), Err(PackError::Hash(_))));
    }

    #[test]
    fn layers() {
        for n in [
            "front.anm",
            "ecldata1.ecl",
            "msg1.dat",
            "thbgm.fmt",
            "th08_0100a.ver",
            "musiccmt.txt",
            "demorpy0.rpy",
        ] {
            assert_eq!(layer_of(n), Layer::Logic, "{n}");
        }
        for n in ["result.jpg", "title00.png", "se_plst00.wav", "th08_01.mid"] {
            assert_eq!(layer_of(n), Layer::Present, "{n}");
        }
    }

    // Every th08.dat entry through a pack equals `Archive::read` (skipped without TH08_DIR).
    #[test]
    fn real_archive_roundtrip() {
        let Some(ar) = crate::testdata::archive() else {
            return;
        };
        let data: Vec<(String, Vec<u8>)> = ar
            .names()
            .iter()
            .map(|n| (n.clone(), ar.read(n).unwrap()))
            .collect();
        let p =
            Pack::from_bytes(write(Kind::Content, &files(&data), 16 << 20, &mut zfast)).unwrap();
        assert_eq!(p.names(), ar.names());
        for (n, b) in &data {
            assert_eq!(&p.read(n).unwrap(), b, "{n}");
        }
    }
}
