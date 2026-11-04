//! th08.pak / thbgm.pak: the port's zstd-framed content container holding th08.dat entries or BGM tracks.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};

use crate::reader::{Reader, Truncated};

// Layout (LE): 32-byte header (magic, version, kind, manifest length u32 + offset u64, sha256[..8] of the manifest),
// frame data, manifest: frames (codec, offset, stored length, decoded size), entries (name, layer, frame, offset,
// size, sha256). A BGM pack holds one Ogg Vorbis stream per track and ends with the thbgm.dat trailer.
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

// Pack kind (header field): th08.dat content (th08.pak) or thbgm.dat tracks (thbgm.pak).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Content = 1,
    Bgm = 2,
}

// Layers: Logic entries feed the game state (and the fingerprint), Present ones only pixels/samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Logic = 0,
    Present = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    Store = 0,
    Zstd = 1,
    // One Ogg Vorbis stream (BGM pack); `size` = decoded PCM bytes before the cut to `loop_end`. Not decoded here.
    Vorbis = 2,
}

// Layer of a th08.dat entry by name: jpg/png surfaces, se_*.wav and .mid are presentation-only;
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
    // Lower-case, as `Archive::names` (lookups are case-insensitive).
    pub name: String,
    pub layer: Layer,
    pub frame: u32,
    pub offset: u64,
    pub size: u64,
    // sha256 of the original bytes (`Archive::read(name)`).
    pub sha256: [u8; 32],
}

// BGM pack trailer: what thbgm.dat itself held, so the header probe is answered exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BgmInfo {
    pub header: [u8; 16],
    pub dat_len: u64,
    pub dat_sha256: [u8; 32],
}

// The parsed header + manifest, without the frame data (a BGM player reads only this, then one stream by range).
#[derive(Clone, Debug)]
pub struct Index {
    pub kind: Kind,
    pub frames: Vec<Frame>,
    pub entries: Vec<Entry>,
    pub bgm: Option<BgmInfo>,
}

pub struct Pack {
    data: Vec<u8>,
    index: Index,
    names: Vec<String>,
    lookup: HashMap<String, usize>,
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

impl Index {
    // Manifest (offset, length) from the 32-byte header.
    pub fn manifest_range(header: &[u8]) -> Result<(u64, usize)> {
        if header.len() < HEADER_LEN || &header[..4] != MAGIC {
            return Err(PackError::NotPack);
        }
        let mut h = Reader::at(header, 4);
        let version = h.u32()?;
        if version != VERSION {
            return Err(PackError::Version(version));
        }
        h.u32()?;
        let mlen = h.u32()? as usize;
        Ok((u64_of(&mut h)?, mlen))
    }

    // Header + the manifest bytes at `manifest_range`; frame ranges are checked against the manifest offset.
    pub fn parse(header: &[u8], manifest: &[u8]) -> Result<Self> {
        let (moff, mlen) = Self::manifest_range(header)?;
        if manifest.len() != mlen {
            return Err(PackError::BadManifest("manifest range"));
        }
        let mut h = Reader::at(header, 8);
        let kind = match h.u32()? {
            1 => Kind::Content,
            2 => Kind::Bgm,
            _ => return Err(PackError::BadManifest("kind")),
        };
        let msha: [u8; 8] = Reader::at(header, 24).array()?;
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
                2 => Codec::Vorbis,
                _ => return Err(PackError::BadManifest("codec")),
            };
            let f = Frame {
                codec,
                offset: u64_of(&mut r)?,
                len: u64_of(&mut r)?,
                size: u64_of(&mut r)?,
            };
            let end = f.offset.checked_add(f.len);
            if f.offset < HEADER_LEN as u64 || end.is_none_or(|e| e > moff) {
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
            // A Vorbis entry is the stream's PCM cut to loop_end, so it may be longer than the decoded size field.
            if f.codec != Codec::Vorbis
                && e.offset.checked_add(e.size).is_none_or(|end| end > f.size)
            {
                return Err(PackError::BadManifest("entry range"));
            }
            entries.push(e);
        }
        let bgm = if kind == Kind::Bgm {
            Some(BgmInfo {
                header: r.array()?,
                dat_len: u64_of(&mut r)?,
                dat_sha256: r.array()?,
            })
        } else {
            None
        };
        if r.pos() != manifest.len() {
            return Err(PackError::BadManifest("trailing bytes"));
        }
        Ok(Self {
            kind,
            frames,
            entries,
            bgm,
        })
    }

    pub fn entry(&self, name: &str) -> Option<&Entry> {
        let name = name.to_ascii_lowercase();
        self.entries.iter().find(|e| e.name == name)
    }
}

impl Pack {
    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        let (moff, mlen) = Index::manifest_range(&data)?;
        let manifest = usize_of(moff)?
            .checked_add(mlen)
            .and_then(|end| data.get(moff as usize..end))
            .ok_or(PackError::BadManifest("manifest range"))?;
        let index = Index::parse(&data, manifest)?;
        let names: Vec<String> = index.entries.iter().map(|e| e.name.clone()).collect();
        let lookup = names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i))
            .collect();
        let verified = index
            .entries
            .iter()
            .map(|_| AtomicBool::new(false))
            .collect();
        Ok(Self {
            data,
            index,
            names,
            lookup,
            verified,
            cache: Mutex::new(Vec::new()),
        })
    }

    pub fn kind(&self) -> Kind {
        self.index.kind
    }

    pub fn index(&self) -> &Index {
        &self.index
    }

    // Stored bytes of frame `idx` (a BGM track's Ogg stream).
    pub fn stored(&self, idx: u32) -> Option<&[u8]> {
        let f = self.index.frames.get(idx as usize)?;
        self.data
            .get(f.offset as usize..(f.offset + f.len) as usize)
    }

    // Entry names in manifest order (= th08.dat table order).
    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn entries(&self) -> &[Entry] {
        &self.index.entries
    }

    pub fn frames(&self) -> &[Frame] {
        &self.index.frames
    }

    pub fn entry(&self, name: &str) -> Option<&Entry> {
        self.lookup
            .get(&name.to_ascii_lowercase())
            .map(|&i| &self.index.entries[i])
    }

    // The entry's original bytes; the first read of each entry is checked against its manifest sha256.
    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        let i = *self
            .lookup
            .get(&name.to_ascii_lowercase())
            .ok_or_else(|| PackError::NotFound(name.to_string()))?;
        let e = &self.index.entries[i];
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
        let f = &self.index.frames[idx as usize];
        let stored = &self.data[f.offset as usize..(f.offset + f.len) as usize];
        let size = usize_of(f.size)?;
        let bytes = match f.codec {
            Codec::Store => stored.to_vec(),
            Codec::Vorbis => {
                return Err(PackError::Decode {
                    frame: idx,
                    msg: "Vorbis stream: decoded by the audio crate".into(),
                });
            }
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

// Writer (repack tool and tests): files grouped into frames by extension (first-appearance order), a frame closed
// before it would exceed `frame_limit` decoded bytes; a file never spans frames; a frame that does not shrink is stored.
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
    for members in &plan {
        let fi = frames.len();
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
    let entries: Vec<Entry> = files
        .iter()
        .enumerate()
        .map(|(i, (n, bytes))| {
            let name = n.to_ascii_lowercase();
            Entry {
                layer: layer_of(&name),
                name,
                frame: placed[i].0,
                offset: placed[i].1,
                size: bytes.len() as u64,
                sha256: sha256(bytes),
            }
        })
        .collect();
    finish(kind, out, &frames, &entries, None)
}

// One thbgm.fmt track for `write_bgm`: its Ogg Vorbis stream, the decoded PCM byte count of that stream, and the
// original PCM `[0, loop_end)` it stands for (size and sha256 only).
pub struct BgmTrack<'a> {
    pub name: &'a str,
    pub ogg: &'a [u8],
    pub decoded_size: u64,
    pub pcm_size: u64,
    pub pcm_sha256: [u8; 32],
}

// thbgm.pak (kind Bgm): one Vorbis frame per track, in thbgm.fmt order, and the thbgm.dat trailer.
pub fn write_bgm(tracks: &[BgmTrack], info: &BgmInfo) -> Vec<u8> {
    let mut out = vec![0u8; HEADER_LEN];
    let mut frames = Vec::with_capacity(tracks.len());
    let mut entries = Vec::with_capacity(tracks.len());
    for (i, t) in tracks.iter().enumerate() {
        frames.push(Frame {
            codec: Codec::Vorbis,
            offset: out.len() as u64,
            len: t.ogg.len() as u64,
            size: t.decoded_size,
        });
        out.extend_from_slice(t.ogg);
        entries.push(Entry {
            name: t.name.to_ascii_lowercase(),
            layer: Layer::Present,
            frame: i as u32,
            offset: 0,
            size: t.pcm_size,
            sha256: t.pcm_sha256,
        });
    }
    finish(Kind::Bgm, out, &frames, &entries, Some(info))
}

// Appends the manifest to `out` (frame data after the reserved header) and fills in the header.
fn finish(
    kind: Kind,
    mut out: Vec<u8>,
    frames: &[Frame],
    entries: &[Entry],
    bgm: Option<&BgmInfo>,
) -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(&(frames.len() as u32).to_le_bytes());
    for f in frames {
        m.push(f.codec as u8);
        m.extend_from_slice(&f.offset.to_le_bytes());
        m.extend_from_slice(&f.len.to_le_bytes());
        m.extend_from_slice(&f.size.to_le_bytes());
    }
    m.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for e in entries {
        m.extend_from_slice(&(e.name.len() as u16).to_le_bytes());
        m.extend_from_slice(e.name.as_bytes());
        m.push(e.layer as u8);
        m.extend_from_slice(&e.frame.to_le_bytes());
        m.extend_from_slice(&e.offset.to_le_bytes());
        m.extend_from_slice(&e.size.to_le_bytes());
        m.extend_from_slice(&e.sha256);
    }
    if let Some(b) = bgm {
        m.extend_from_slice(&b.header);
        m.extend_from_slice(&b.dat_len.to_le_bytes());
        m.extend_from_slice(&b.dat_sha256);
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
    fn bgm_pack_roundtrip() {
        let info = BgmInfo {
            header: *b"ZWAV\x01\0\0\0\0\x08\0\0\0\0\0\0",
            dat_len: 123_456,
            dat_sha256: [7; 32],
        };
        let tracks = [
            BgmTrack {
                name: "th08_01.wav",
                ogg: b"OggS-one",
                decoded_size: 4100,
                pcm_size: 4000,
                pcm_sha256: [1; 32],
            },
            BgmTrack {
                name: "TH08_00.wav",
                ogg: b"OggS-two!",
                decoded_size: 900,
                pcm_size: 1000,
                pcm_sha256: [2; 32],
            },
        ];
        let bytes = write_bgm(&tracks, &info);
        let p = Pack::from_bytes(bytes.clone()).unwrap();
        assert_eq!(p.kind(), Kind::Bgm);
        assert_eq!(p.index().bgm.as_ref(), Some(&info));
        assert_eq!(p.names(), ["th08_01.wav", "th08_00.wav"]);
        let e = p.entry("th08_00.WAV").unwrap();
        assert_eq!((e.size, e.layer, e.sha256), (1000, Layer::Present, [2; 32]));
        assert_eq!(p.stored(e.frame), Some(&b"OggS-two!"[..]));
        assert!(matches!(
            p.read("th08_01.wav"),
            Err(PackError::Decode { .. })
        ));
        // Manifest-only parse, as the audio crate does it over ranged reads.
        let (off, len) = Index::manifest_range(&bytes[..HEADER_LEN]).unwrap();
        let idx = Index::parse(
            &bytes[..HEADER_LEN],
            &bytes[off as usize..off as usize + len],
        )
        .unwrap();
        assert_eq!(idx.entries, p.entries());
        assert_eq!(idx.entry("TH08_01.wav").unwrap().frame, 0);
        assert_eq!(idx.frames[1].offset, HEADER_LEN as u64 + 8);
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
