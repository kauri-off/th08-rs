//! Core content views: logic reads game data only through `LogicView`, presentation-only data through `PresentView`.

use std::collections::HashMap;
use std::sync::Arc;

use th08_formats::archive::{Archive, ArchiveError};
use th08_formats::pack::{HEADER_LEN, Index, Kind, Layer, Pack, PackError};

use crate::assets::Source;

// Logic-content fingerprint: hash over the sorted (key, bytes) of the Logic-layer overrides.
// Vanilla has no overrides, so its fingerprint is empty; written into session metadata (`meta.txt`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fingerprint(Vec<u8>);

impl Fingerprint {
    pub fn is_vanilla(&self) -> bool {
        self.0.is_empty()
    }

    // Rule for a pack: vanilla iff its Logic entries are exactly the vanilla table (name and
    // sha256); otherwise sha256 over the sorted differing names, each with its pack hash (none when missing).
    fn of_pack(pack: &Pack) -> Self {
        let mut diff = logic_diff(pack);
        if diff.is_empty() {
            return Self::default();
        }
        diff.sort_unstable();
        let mut buf = Vec::new();
        for (name, h) in diff {
            buf.extend_from_slice(name.as_bytes());
            buf.push(0);
            buf.extend_from_slice(h.map_or(&[][..], |h| &h[..]));
        }
        Self(th08_formats::pack::sha256(&buf).to_vec())
    }
}

// Logic entries that differ from the vanilla table (missing, changed or extra), each with its pack hash.
fn logic_diff(pack: &Pack) -> Vec<(&str, Option<&[u8; 32]>)> {
    let mut diff: Vec<(&str, Option<&[u8; 32]>)> = Vec::new();
    for (name, hex) in crate::vanilla::LOGIC_SHA256 {
        match pack.entry(name) {
            Some(e) if e.layer == Layer::Logic && hex_eq(hex, &e.sha256) => {}
            e => diff.push((name, e.map(|e| &e.sha256))),
        }
    }
    for e in pack.entries().iter().filter(|e| e.layer == Layer::Logic) {
        if !crate::vanilla::LOGIC_SHA256.iter().any(|v| v.0 == e.name) {
            diff.push((&e.name, Some(&e.sha256)));
        }
    }
    diff
}

// Names of the pack's Logic entries that are not the vanilla th08.dat ones (empty = this game version).
pub fn vanilla_mismatch(pack: &Pack) -> Vec<String> {
    logic_diff(pack)
        .into_iter()
        .map(|(n, _)| n.to_owned())
        .collect()
}

fn hex_eq(hex: &str, h: &[u8; 32]) -> bool {
    hex.len() == 64
        && h.iter()
            .enumerate()
            .all(|(i, b)| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16) == Ok(*b))
}

impl std::fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

// The content pack's file name; preferred over th08.dat by `Content::load`.
pub const CONTENT_PACK: &str = "th08.pak";

// A game folder / bundle holds one of these (any letter case), the pack first.
pub const CONTENT_FILES: [&str; 2] = [CONTENT_PACK, "th08.dat"];

// Whether `names` hold game content (th08.pak or th08.dat), as the launchers' folder checks ask.
pub fn has_content<S: AsRef<str>>(names: &[S]) -> bool {
    names.iter().any(|n| {
        CONTENT_FILES
            .iter()
            .any(|c| n.as_ref().eq_ignore_ascii_case(c))
    })
}

// Where the content bytes live: the original th08.dat, or a th08.pak repacked from it (same bytes per entry).
pub enum Content {
    Archive(Archive),
    Pack(Pack),
}

impl Content {
    // Start-up: th08.pak when the source has one, else th08.dat.
    pub async fn load(source: &Source) -> anyhow::Result<Self> {
        if source
            .names()
            .iter()
            .any(|n| n.eq_ignore_ascii_case(CONTENT_PACK))
        {
            return Ok(Self::Pack(Pack::from_bytes(
                source.read(CONTENT_PACK).await?,
            )?));
        }
        Ok(Self::Archive(Archive::from_bytes(
            source.read("th08.dat").await?,
        )?))
    }

    fn read(&self, name: &str) -> th08_formats::archive::Result<Vec<u8>> {
        match self {
            Self::Archive(a) => a.read(name),
            Self::Pack(p) => p.read(name).map_err(|e| match e {
                PackError::NotFound(n) => ArchiveError::NotFound(n),
                e => e.into(),
            }),
        }
    }

    fn names(&self) -> &[String] {
        match self {
            Self::Archive(a) => a.names(),
            Self::Pack(p) => p.names(),
        }
    }
}

// The BGM pack's file name: Ogg Vorbis per thbgm.fmt track, preferred over thbgm.dat.
pub const BGM_PACK: &str = "thbgm.pak";

// thbgm.pak's header + manifest, read by range (no stream data); None when the source has no thbgm.pak.
pub async fn bgm_pack_index(source: &Source) -> Option<anyhow::Result<Index>> {
    if !source
        .names()
        .iter()
        .any(|n| n.eq_ignore_ascii_case(BGM_PACK))
    {
        return None;
    }
    Some(
        async {
            let header = source.read_range(BGM_PACK, 0, HEADER_LEN).await?;
            let (off, len) = Index::manifest_range(&header)?;
            let manifest = source.read_range(BGM_PACK, off, len).await?;
            let index = Index::parse(&header, &manifest)?;
            anyhow::ensure!(
                index.kind == Kind::Bgm && index.bgm.is_some(),
                "{BGM_PACK}: not a BGM pack"
            );
            Ok(index)
        }
        .await,
    )
}

// Config_Load's thbgm.dat header probe: the 16 header bytes, or None when the file is missing.
// With a thbgm.pak the stored original header answers, so `config::load` decides exactly as with thbgm.dat.
pub async fn bgm_header(source: &Source) -> Option<Vec<u8>> {
    match bgm_pack_index(source).await {
        Some(Ok(index)) => return index.bgm.map(|b| b.header.to_vec()),
        Some(Err(e)) => log::warn!("{BGM_PACK}: {e:#}; probing thbgm.dat"),
        None => {}
    }
    source.read_range("thbgm.dat", 0, 16).await.ok()
}

impl From<Archive> for Content {
    fn from(a: Archive) -> Self {
        Self::Archive(a)
    }
}

// Every logic-relevant content read (.ecl, .std, .sht, .anm, msg, .rpy, .ver, .fmt …).
pub struct LogicView {
    content: Content,
    fingerprint: Fingerprint,
    // Presentation pack images; never read by `read`, so the fingerprint and every logic byte stay vanilla.
    overlay: Option<Arc<Overlay>>,
}

impl LogicView {
    pub fn new(content: impl Into<Content>) -> Self {
        let content = content.into();
        let fingerprint = match &content {
            Content::Archive(_) => Fingerprint::default(),
            Content::Pack(p) => Fingerprint::of_pack(p),
        };
        Self {
            content,
            fingerprint,
            overlay: None,
        }
    }

    // Mounts a presentation pack over the presentation reads.
    pub fn mount(&mut self, overlay: Arc<Overlay>) {
        self.overlay = Some(overlay);
    }

    // Lookup by bare name, case-insensitive.
    pub fn read(&self, name: &str) -> th08_formats::archive::Result<Vec<u8>> {
        self.content.read(name)
    }

    pub fn fingerprint(&self) -> &Fingerprint {
        &self.fingerprint
    }

    // The presentation layers over the same content.
    pub fn present(&self) -> PresentView<'_> {
        PresentView {
            content: &self.content,
            overlay: self.overlay.as_deref(),
        }
    }
}

// Presentation-only reads (jpg surfaces, se_*.wav, .mid, THTX pixels) and the mounted pack's image overrides.
#[derive(Clone, Copy)]
pub struct PresentView<'a> {
    content: &'a Content,
    overlay: Option<&'a Overlay>,
}

impl PresentView<'_> {
    // The vanilla bytes; a pack never changes them (sizes stay the archive's).
    pub fn read(&self, name: &str) -> th08_formats::archive::Result<Vec<u8>> {
        self.content.read(name)
    }

    pub fn names(&self) -> &[String] {
        self.content.names()
    }

    // A standalone .jpg / .png: the vanilla bytes (they give the texture size) and the pack's override.
    pub fn image(&self, name: &str) -> anyhow::Result<(Vec<u8>, Option<ImageBytes>)> {
        let over = self.overlay.and_then(|o| o.file(name));
        Ok((self.read(name)?, over))
    }

    // Override of an ANM entry's THTX pixels by its image name (+0x1C); '@' targets never.
    pub fn chunk_image(&self, chunk: &str) -> Option<Arc<[u8]>> {
        self.overlay.and_then(|o| o.chunk(chunk))
    }
}

// An encoded .png / .jpg of a presentation pack, shared with the renderer's create command.
pub type ImageBytes = Arc<[u8]>;

// Why a pack file is not mounted.
#[derive(Debug, PartialEq, Eq)]
pub enum Reject {
    // A key core logic reads (ecl, std, msg, anm, sht, end, …: `pack::layer_of` = Logic).
    Logic(String),
    // A presentation key that is not a texture (wav, mid): packs override textures only.
    NotTexture(String),
}

impl std::fmt::Display for Reject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Logic(n) => write!(
                f,
                "{n}: logic key, rejected (presentation packs never change logic)"
            ),
            Self::NotTexture(n) => write!(f, "{n}: not a texture, ignored"),
        }
    }
}

// Image layers of a presentation pack: `images/<chunk image name>` replaces the THTX pixels
// of every ANM entry with that name, `files/<name>.jpg|png` a standalone image. Keys are lowercase with '/'.
#[derive(Clone, Debug, Default)]
pub struct Overlay {
    chunks: HashMap<String, Arc<[u8]>>,
    files: HashMap<String, Arc<[u8]>>,
}

impl Overlay {
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty() && self.files.is_empty()
    }

    pub fn len(&self) -> usize {
        self.chunks.len() + self.files.len()
    }

    // `path` relative to the pack root. `images/…` → chunk override, anything else → a file override by base name
    // (`files/info00.jpg`, a stray `ecldata1.ecl`); logic keys and non-textures are refused.
    pub fn add(&mut self, path: &str, bytes: Vec<u8>) -> Result<(), Reject> {
        let path = path.replace('\\', "/").to_ascii_lowercase();
        let base = path.rsplit('/').next().unwrap_or(&path).to_string();
        if th08_formats::pack::layer_of(&base) == Layer::Logic {
            return Err(Reject::Logic(path));
        }
        if !(base.ends_with(".png") || base.ends_with(".jpg")) {
            return Err(Reject::NotTexture(path));
        }
        match path.strip_prefix("images/") {
            Some(chunk) => self
                .chunks
                .insert(chunk_key(chunk).to_string(), bytes.into()),
            None => self.files.insert(base, bytes.into()),
        };
        Ok(())
    }

    // By chunk image name (`data/ascii/ascii.png`, thcrap's `ascii/ascii.png`), then by its base name.
    fn chunk(&self, name: &str) -> Option<Arc<[u8]>> {
        if name.starts_with('@') {
            return None;
        }
        let key = name.replace('\\', "/").to_ascii_lowercase();
        let key = chunk_key(&key);
        self.chunks
            .get(key)
            .or_else(|| self.chunks.get(key.rsplit('/').next()?))
            .cloned()
    }

    fn file(&self, name: &str) -> Option<Arc<[u8]>> {
        self.files.get(&name.to_ascii_lowercase()).cloned()
    }
}

// `data/` prefix dropped: thcrap's `data/text/stg1txt.png` and the chunk name `data/text/stg1txt.png` share a key.
fn chunk_key(name: &str) -> &str {
    name.strip_prefix("data/").unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use th08_formats::pack::{Kind, write};

    // The vanilla table rebuilt as pack input: (name, bytes) cannot be recovered from hashes, so these tests use
    // a pack whose Logic entries are fake and check the rule's direction only; the real check is `real_pack`.
    #[test]
    fn non_vanilla_pack_changes_fingerprint() {
        let a = write(
            Kind::Content,
            &[("ecldata1.ecl", b"x")],
            1 << 20,
            &mut |b: &[u8]| b.to_vec(),
        );
        let b = write(
            Kind::Content,
            &[("ecldata1.ecl", b"y")],
            1 << 20,
            &mut |b: &[u8]| b.to_vec(),
        );
        let fa = LogicView::new(Content::Pack(Pack::from_bytes(a).unwrap()));
        let fb = LogicView::new(Content::Pack(Pack::from_bytes(b).unwrap()));
        assert!(!fa.fingerprint().is_vanilla());
        assert_ne!(fa.fingerprint(), fb.fingerprint());
    }

    // In-memory files for the probe tests.
    struct Mem(Vec<(&'static str, Vec<u8>)>);
    impl crate::assets::DataSource for Mem {
        fn names(&self) -> Vec<String> {
            self.0.iter().map(|f| f.0.to_string()).collect()
        }
        fn fetch<'a>(
            &'a self,
            name: &'a str,
            range: Option<(u64, usize)>,
        ) -> crate::assets::BoxFuture<'a, anyhow::Result<Vec<u8>>> {
            let r = self
                .0
                .iter()
                .find(|f| f.0.eq_ignore_ascii_case(name))
                .and_then(|f| match range {
                    None => Some(f.1.clone()),
                    Some((o, n)) => f.1.get(o as usize..o as usize + n).map(<[u8]>::to_vec),
                })
                .ok_or_else(|| anyhow::anyhow!("{name}: missing or short"));
            Box::pin(async move { r })
        }
    }

    // Probe: thbgm.pak answers with the stored thbgm.dat header; without it thbgm.dat; neither → None.
    #[test]
    fn bgm_header_probe() {
        use crate::testdata::block_on;
        use th08_formats::pack::{BgmInfo, write_bgm};
        let header = *b"ZWAV\x01\0\0\0\0\x08\0\0\0\0\0\0";
        let info = BgmInfo {
            header,
            dat_len: 16,
            dat_sha256: [0; 32],
        };
        let pak = write_bgm(&[], &info);
        let mut dat = header.to_vec();
        dat[12] = 9;
        let src = |f| Source::new(Mem(f));
        let both = src(vec![("THBGM.PAK", pak.clone()), ("thbgm.dat", dat.clone())]);
        assert_eq!(block_on(bgm_header(&both)).as_deref(), Some(&header[..]));
        let raw = src(vec![("thbgm.dat", dat.clone())]);
        assert_eq!(block_on(bgm_header(&raw)), Some(dat.clone()));
        // A broken pack falls back to thbgm.dat.
        let broken = src(vec![
            ("thbgm.pak", pak[..40].to_vec()),
            ("thbgm.dat", dat.clone()),
        ]);
        assert_eq!(block_on(bgm_header(&broken)), Some(dat));
        assert_eq!(block_on(bgm_header(&src(vec![]))), None);
        // A content pack under the BGM name is rejected.
        let content = write(
            Kind::Content,
            &[("a.ecl", b"x")],
            1 << 20,
            &mut |b: &[u8]| b.to_vec(),
        );
        assert!(
            block_on(bgm_pack_index(&src(vec![("thbgm.pak", content)])))
                .unwrap()
                .is_err()
        );
    }

    // Overlay keys: thcrap / chunk names share a key, '@' targets and logic keys are never overridden, and
    // mounting changes neither `read` nor the fingerprint.
    #[test]
    fn overlay_keys() {
        let mut o = Overlay::default();
        o.add("images/ascii/ascii.png", b"a".to_vec()).unwrap();
        o.add("images/front.png", b"f".to_vec()).unwrap();
        o.add("files/TH08LOGO.JPG", b"l".to_vec()).unwrap();
        assert_eq!(
            o.add("files/ecldata1.ecl", vec![]),
            Err(Reject::Logic("files/ecldata1.ecl".into()))
        );
        assert!(matches!(o.add("msg1a.dat", vec![]), Err(Reject::Logic(_))));
        assert!(matches!(
            o.add("images/text.anm", vec![]),
            Err(Reject::Logic(_))
        ));
        assert!(matches!(
            o.add("files/se_ok00.wav", vec![]),
            Err(Reject::NotTexture(_))
        ));
        assert_eq!(o.chunk("data/ascii/ascii.png").as_deref(), Some(&b"a"[..]));
        assert_eq!(o.chunk("data/front/front.png").as_deref(), Some(&b"f"[..]));
        assert_eq!(o.chunk("@text"), None);
        assert_eq!(o.file("th08logo.jpg").as_deref(), Some(&b"l"[..]));
        let pak = write(
            Kind::Content,
            &[("ecldata1.ecl", b"x"), ("th08logo.jpg", b"jpg")],
            1 << 20,
            &mut |b: &[u8]| b.to_vec(),
        );
        let plain = LogicView::new(Content::Pack(Pack::from_bytes(pak.clone()).unwrap()));
        let mut lv = LogicView::new(Content::Pack(Pack::from_bytes(pak).unwrap()));
        lv.mount(Arc::new(o));
        assert_eq!(lv.fingerprint(), plain.fingerprint());
        assert_eq!(lv.read("ecldata1.ecl").unwrap(), b"x");
        let (b, over) = lv.present().image("th08logo.jpg").unwrap();
        assert_eq!(
            (b.as_slice(), over.as_deref()),
            (&b"jpg"[..], Some(&b"l"[..]))
        );
        assert_eq!(plain.present().image("th08logo.jpg").unwrap().1, None);
    }

    #[test]
    fn hex_compare() {
        let mut h = [0u8; 32];
        h[0] = 0xAB;
        h[31] = 0x01;
        let hex = format!("ab{}01", "00".repeat(30));
        assert!(hex_eq(&hex, &h));
        h[31] = 2;
        assert!(!hex_eq(&hex, &h));
    }

    // Every th08.dat entry read through `Content::Pack` equals `Content::Archive`; a pack of the original data has
    // the vanilla fingerprint, one with a single Logic entry changed does not (skipped without TH08_DIR).
    #[test]
    fn real_pack() {
        let Some(ar) = crate::testdata::archive() else {
            return;
        };
        let data: Vec<(String, Vec<u8>)> = ar
            .names()
            .iter()
            .map(|n| (n.clone(), ar.read(n).unwrap()))
            .collect();
        let files: Vec<(&str, &[u8])> = data
            .iter()
            .map(|(n, b)| (n.as_str(), b.as_slice()))
            .collect();
        let store = &mut |b: &[u8]| b.to_vec();
        let pack = LogicView::new(Content::Pack(
            Pack::from_bytes(write(Kind::Content, &files, 16 << 20, store)).unwrap(),
        ));
        let arc = LogicView::new(ar);
        assert!(pack.fingerprint().is_vanilla());
        assert_eq!(pack.present().names(), arc.present().names());
        for (n, b) in &data {
            assert_eq!(&pack.read(n).unwrap(), b, "{n}");
            assert_eq!(&pack.present().read(n).unwrap(), b, "{n}");
        }
        let mut changed = files.clone();
        let i = changed.iter().position(|f| f.0.ends_with(".ecl")).unwrap();
        changed[i].1 = b"modded";
        let modded = LogicView::new(Content::Pack(
            Pack::from_bytes(write(Kind::Content, &changed, 16 << 20, store)).unwrap(),
        ));
        assert!(!modded.fingerprint().is_vanilla());
        // A Present-only change keeps the vanilla fingerprint.
        let mut pres = files.clone();
        let j = pres.iter().position(|f| f.0.ends_with(".jpg")).unwrap();
        pres[j].1 = b"not a jpeg";
        let p = LogicView::new(Content::Pack(
            Pack::from_bytes(write(Kind::Content, &pres, 16 << 20, store)).unwrap(),
        ));
        assert!(p.fingerprint().is_vanilla());
    }
}
