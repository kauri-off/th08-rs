//! ANM container parser (03 §2–§4): `parse` = entry chain, sprites, scripts (logic); `textures` = THTX pixels (presentation).

#[derive(Debug, thiserror::Error)]
pub enum AnmError {
    #[error("bad THTX magic")]
    BadThtxMagic,
    #[error("unknown THTX format {0}")]
    UnknownThtxFormat(u16),
    #[error("anm version mismatch")]
    VersionMismatch,
}

pub type Result<T> = std::result::Result<T, AnmError>;

// Sprite record, 20 bytes at entry_start + sprite_offset (03 §2.2); x, y, w, h in logical texture pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sprite {
    // Always the file-wide cumulative sprite index; the loader ignores it (03 §2.2).
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

// One script of the entry's script table (03 §2.1); scripts are addressed by cumulative index (03 §2.4).
#[derive(Clone, Debug, PartialEq)]
pub struct Script {
    // Ignored by the original loader (03 §2.1).
    pub id: i32,
    // Instruction bytes from the table offset up to the next script (scripts are contiguous, 05 intro).
    pub data: Vec<u8>,
}

// Embedded THTX texture block (03 §4), decoded to RGBA8.
pub struct Thtx {
    // Texture format index (03 §3).
    pub format: u16,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

// One texture entry of the chain (03 §2.1), logic data only; its pixels are `textures(..)[entry]`.
pub struct Chunk {
    // Image name at +0x1C; a leading '@' means a blank render-target texture (03 §2.1).
    pub name: String,
    // Logical size at +0x0C/+0x10: sprite UVs are normalised by it, not by the THTX size (03 §2.2).
    pub width: u32,
    pub height: u32,
    pub sprites: Vec<Sprite>,
    pub scripts: Vec<Script>,
    // THTX offset +0x30 is non-zero: the pixels come from `textures`, not from this logic record (03 §2.1).
    pub has_texture: bool,
}

fn u32at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn f32at(b: &[u8], o: usize) -> f32 {
    f32::from_bits(u32at(b, o))
}

fn u16at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

// THTX header (03 §4): magic, u16 0, u16 format, u16 width, u16 height, u32 size, then packed rows at +0x10.
pub fn decode_thtx(b: &[u8]) -> Result<Thtx> {
    if b.len() < 16 || &b[0..4] != b"THTX" {
        return Err(AnmError::BadThtxMagic);
    }
    let format = u16at(b, 6);
    let (width, height) = (u16at(b, 8) as u32, u16at(b, 10) as u32);
    let px = &b[16..];
    let n = (width * height) as usize;
    let mut rgba = Vec::with_capacity(n * 4);
    // 5/6-bit channels widened to 8 bits by bit replication for the RGBA8 upload (03 §3 allows full precision).
    let x5 = |v: u16| ((v << 3) | (v >> 2)) as u8;
    let x6 = |v: u16| ((v << 2) | (v >> 4)) as u8;
    match format {
        // 03 §3 format table: 1 A8R8G8B8, 2 A1R5G5B5, 3 R5G6B5, 4 R8G8B8, 5 A4R4G4B4 (little-endian words).
        1 => {
            for p in px[..n * 4].chunks_exact(4) {
                rgba.extend([p[2], p[1], p[0], p[3]]);
            }
        }
        2 => {
            for p in px[..n * 2].chunks_exact(2) {
                let v = u16::from_le_bytes([p[0], p[1]]);
                rgba.extend([
                    x5((v >> 10) & 31),
                    x5((v >> 5) & 31),
                    x5(v & 31),
                    if v & 0x8000 != 0 { 255 } else { 0 },
                ]);
            }
        }
        3 => {
            for p in px[..n * 2].chunks_exact(2) {
                let v = u16::from_le_bytes([p[0], p[1]]);
                rgba.extend([x5(v >> 11), x6((v >> 5) & 63), x5(v & 31), 255]);
            }
        }
        4 => {
            for p in px[..n * 3].chunks_exact(3) {
                rgba.extend([p[2], p[1], p[0], 255]);
            }
        }
        5 => {
            for p in px[..n * 2].chunks_exact(2) {
                let v = u16::from_le_bytes([p[0], p[1]]);
                let n4 = |s: u16| (((v >> s) & 15) * 17) as u8;
                rgba.extend([n4(8), n4(4), n4(0), n4(12)]);
            }
        }
        f => return Err(AnmError::UnknownThtxFormat(f)),
    }
    Ok(Thtx {
        format,
        width,
        height,
        rgba,
    })
}

// Anm_Preload 0x465860 (03 §2.1): entry k+1 at entry_k + next_offset (+0x38), 0 ends the chain; yields each entry's
// file offset. Version must be 3, otherwise the load fails.
fn entries(b: &[u8]) -> Result<Vec<usize>> {
    let mut v = Vec::new();
    let mut base = 0;
    loop {
        let c = &b[base..];
        if u32at(c, 0x28) != 3 {
            return Err(AnmError::VersionMismatch);
        }
        v.push(base);
        match u32at(c, 0x38) as usize {
            0 => return Ok(v),
            next => base += next,
        }
    }
}

// Logic half of Anm_Preload 0x465860 (03 §2.1–§2.2): entries, sprites, scripts; no pixels are touched.
pub fn parse(b: &[u8]) -> Result<Vec<Chunk>> {
    entries(b)?.into_iter().map(|base| entry(b, base)).collect()
}

fn entry(b: &[u8], base: usize) -> Result<Chunk> {
    let c = &b[base..];
    let (n_spr, n_scr) = (u32at(c, 0) as usize, u32at(c, 4) as usize);
    let (width, height) = (u32at(c, 0x0c), u32at(c, 0x10));
    let name_off = u32at(c, 0x1c) as usize;
    let thtx_off = u32at(c, 0x30) as usize;
    let next = u32at(c, 0x38) as usize;
    let name_end = c[name_off..].iter().position(|&x| x == 0).unwrap_or(0);
    let name = String::from_utf8_lossy(&c[name_off..name_off + name_end]).into_owned();
    let sprites = (0..n_spr)
        .map(|i| {
            let o = u32at(c, 0x40 + 4 * i) as usize;
            Sprite {
                id: u32at(c, o),
                x: f32at(c, o + 4),
                y: f32at(c, o + 8),
                w: f32at(c, o + 12),
                h: f32at(c, o + 16),
            }
        })
        .collect();
    // Script table {i32 id, u32 offset}[script_count] follows the sprite offsets (03 §2.1).
    let tbl = 0x40 + 4 * n_spr;
    let offs: Vec<(i32, usize)> = (0..n_scr)
        .map(|i| {
            (
                u32at(c, tbl + 8 * i) as i32,
                u32at(c, tbl + 8 * i + 4) as usize,
            )
        })
        .collect();
    // Script length is not stored: each script ends at the next script start, the THTX block or the next entry.
    let limit = if thtx_off != 0 {
        thtx_off
    } else if next != 0 {
        next
    } else {
        c.len()
    };
    let mut sorted: Vec<usize> = offs.iter().map(|o| o.1).collect();
    sorted.sort();
    let scripts = offs
        .into_iter()
        .map(|(id, o)| {
            let end = sorted
                .iter()
                .copied()
                .find(|&e| e > o)
                .unwrap_or(limit)
                .min(limit);
            Script {
                id,
                data: c[o..end].to_vec(),
            }
        })
        .collect();
    Ok(Chunk {
        name,
        width,
        height,
        sprites,
        scripts,
        has_texture: thtx_off != 0,
    })
}

// Pixel half of Anm_Preload / Anm_LoadEntryTexture 0x465CC0 (03 §2.1, §4): one decoded THTX per entry, `None` for
// entries without a THTX block ('@' render targets). Same entry order as `parse`.
pub fn textures(b: &[u8]) -> Result<Vec<Option<Thtx>>> {
    entries(b)?
        .into_iter()
        .map(|base| match u32at(b, base + 0x30) as usize {
            0 => Ok(None),
            off => decode_thtx(&b[base + off..]).map(Some),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pre-IMPL179 single-pass parser (sprites, scripts and decoded THTX together), kept as the split's reference.
    fn legacy(b: &[u8]) -> Vec<(String, u32, u32, Vec<Sprite>, Vec<Script>, Option<Thtx>)> {
        let mut out = Vec::new();
        let mut base = 0;
        loop {
            let c = &b[base..];
            let (n_spr, n_scr) = (u32at(c, 0) as usize, u32at(c, 4) as usize);
            assert_eq!(u32at(c, 0x28), 3);
            let (thtx_off, next) = (u32at(c, 0x30) as usize, u32at(c, 0x38) as usize);
            let name_off = u32at(c, 0x1c) as usize;
            let name_end = c[name_off..].iter().position(|&x| x == 0).unwrap_or(0);
            let name = String::from_utf8_lossy(&c[name_off..name_off + name_end]).into_owned();
            let sprites = (0..n_spr)
                .map(|i| {
                    let o = u32at(c, 0x40 + 4 * i) as usize;
                    let f = |k| f32at(c, o + k);
                    Sprite {
                        id: u32at(c, o),
                        x: f(4),
                        y: f(8),
                        w: f(12),
                        h: f(16),
                    }
                })
                .collect();
            let tbl = 0x40 + 4 * n_spr;
            let offs: Vec<(i32, usize)> = (0..n_scr)
                .map(|i| {
                    (
                        u32at(c, tbl + 8 * i) as i32,
                        u32at(c, tbl + 8 * i + 4) as usize,
                    )
                })
                .collect();
            let limit = [thtx_off, next, c.len()]
                .into_iter()
                .find(|&x| x != 0)
                .unwrap();
            let mut sorted: Vec<usize> = offs.iter().map(|o| o.1).collect();
            sorted.sort();
            let scripts = offs
                .into_iter()
                .map(|(id, o)| {
                    let end = sorted
                        .iter()
                        .copied()
                        .find(|&e| e > o)
                        .unwrap_or(limit)
                        .min(limit);
                    Script {
                        id,
                        data: c[o..end].to_vec(),
                    }
                })
                .collect();
            let tex = (thtx_off != 0).then(|| decode_thtx(&c[thtx_off..]).unwrap());
            out.push((name, u32at(c, 0x0c), u32at(c, 0x10), sprites, scripts, tex));
            if next == 0 {
                return out;
            }
            base += next;
        }
    }

    // The logic / pixel split yields the same sprites, scripts and pixel bytes as the single-pass parser, every file.
    #[test]
    fn split_matches_legacy() {
        let Some(ar) = crate::testdata::archive() else {
            return;
        };
        let (mut files, mut tex) = (0, 0);
        for n in ar.names().iter().filter(|n| n.ends_with(".anm")) {
            let b = ar.read(n).unwrap();
            let chunks = parse(&b).unwrap_or_else(|e| panic!("{n}: {e}"));
            let pixels = textures(&b).unwrap_or_else(|e| panic!("{n}: {e}"));
            let old = legacy(&b);
            assert_eq!((chunks.len(), pixels.len()), (old.len(), old.len()), "{n}");
            for ((c, p), (name, w, h, spr, scr, t)) in chunks.iter().zip(&pixels).zip(&old) {
                assert_eq!((&c.name, c.width, c.height), (name, *w, *h), "{n}");
                let bits = |v: &[Sprite]| -> Vec<[u32; 5]> {
                    v.iter()
                        .map(|s| {
                            [
                                s.id,
                                s.x.to_bits(),
                                s.y.to_bits(),
                                s.w.to_bits(),
                                s.h.to_bits(),
                            ]
                        })
                        .collect()
                };
                assert_eq!(bits(&c.sprites), bits(spr), "{n}");
                assert_eq!(&c.scripts, scr, "{n}");
                assert_eq!(c.has_texture, t.is_some(), "{n}");
                let key = |t: &Option<Thtx>| {
                    t.as_ref()
                        .map(|t| (t.format, t.width, t.height, t.rgba.clone()))
                };
                assert!(key(p) == key(t), "{n}: pixels differ");
            }
            tex += pixels.iter().filter(|t| t.is_some()).count();
            files += 1;
        }
        assert_eq!(files, 113);
        assert_eq!(tex, 306);
    }
}
