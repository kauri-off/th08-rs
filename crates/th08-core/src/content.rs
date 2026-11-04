//! Core content views (ARCH_B_MODDING §1.2, decisions D1 / D5): logic reads game data only through `LogicView`,
//! presentation-only data (pixels, samples, MIDI) through `PresentView`; both over the th08.dat archive (01 §2–§7)
//! or its repacked th08.pak (port-only container, docs/arch/ASSETS_FORMAT.md §3.1, §3.3).

use th08_formats::archive::{Archive, ArchiveError};
use th08_formats::pack::{HEADER_LEN, Index, Kind, Layer, Pack, PackError};

use crate::assets::Source;

// Logic-content fingerprint (ARCH_B D5): hash over the sorted (key, bytes) of the Logic-layer overrides.
// Vanilla has no overrides, so its fingerprint is empty; written into session metadata (`meta.txt`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fingerprint(Vec<u8>);

impl Fingerprint {
    pub fn is_vanilla(&self) -> bool {
        self.0.is_empty()
    }

    // ASSETS_FORMAT §3.3 rule for a pack: vanilla iff its Logic entries are exactly the vanilla table (name and
    // sha256); otherwise sha256 over the sorted differing names, each with its pack hash (none when missing).
    fn of_pack(pack: &Pack) -> Self {
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

// Where the content bytes live: the original th08.dat, or a th08.pak repacked from it (same bytes per entry).
pub enum Content {
    Archive(Archive),
    Pack(Pack),
}

impl Content {
    // Start-up (00 §2 archive open): th08.pak when the source has one, else th08.dat.
    pub async fn load(source: &Source) -> anyhow::Result<Self> {
        if source
            .names()
            .iter()
            .any(|n| n.eq_ignore_ascii_case("th08.pak"))
        {
            return Ok(Self::Pack(Pack::from_bytes(
                source.read("th08.pak").await?,
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

// The BGM pack's file name (ASSETS_FORMAT §3.1): Ogg Vorbis per thbgm.fmt track, preferred over thbgm.dat.
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

// Config_Load's thbgm.dat header probe (02 §1, 15 §1.2): the 16 header bytes, or None when the file is missing.
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
        }
    }

    // Lookup by bare name, case-insensitive (01 §5).
    pub fn read(&self, name: &str) -> th08_formats::archive::Result<Vec<u8>> {
        self.content.read(name)
    }

    pub fn fingerprint(&self) -> &Fingerprint {
        &self.fingerprint
    }

    // The presentation layers over the same content.
    pub fn present(&self) -> PresentView<'_> {
        PresentView(&self.content)
    }
}

// Presentation-only reads (jpg surfaces, se_*.wav, .mid); a later presentation pack may replace them freely.
#[derive(Clone, Copy)]
pub struct PresentView<'a>(&'a Content);

impl PresentView<'_> {
    pub fn read(&self, name: &str) -> th08_formats::archive::Result<Vec<u8>> {
        self.0.read(name)
    }

    pub fn names(&self) -> &[String] {
        self.0.names()
    }
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

    // 02 §1 probe: thbgm.pak answers with the stored thbgm.dat header; without it thbgm.dat; neither → None.
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
