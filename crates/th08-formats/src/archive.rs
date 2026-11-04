//! th08.dat "PBGZ" archive reader (01 §2–§7) plus the cipher/LZSS encoders the score.dat and replay savers need (15 §2.1, §3.5).

use std::collections::HashMap;

use crate::reader::{Reader, Truncated, u32_at};

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("not a PBGZ archive")]
    NotPbgz,
    #[error("bad PBGZ header (count {count}, table offset {table_off:#x})")]
    BadHeader { count: i32, table_off: usize },
    #[error("bad table")]
    BadTable,
    #[error("{0} not in archive")]
    NotFound(String),
    #[error("archive table: {0}")]
    Truncated(#[from] Truncated),
    // Content read through a th08.pak instead of th08.dat (pack.rs).
    #[error(transparent)]
    Pack(#[from] crate::pack::PackError),
}

pub type Result<T> = std::result::Result<T, ArchiveError>;

// decrypt() parameters of 01 §2: start key, key step (both mod 256), block size and byte limit.
struct CipherKey {
    key: u8,
    step: u8,
    block: usize,
    limit: usize,
}

const fn ck(key: u8, step: u8, block: usize, limit: usize) -> CipherKey {
    CipherKey {
        key,
        step,
        block,
        limit,
    }
}

// 01 §4 (header dwords at +4) and 01 §5 (file table, before LZSS).
const HEADER_KEY: CipherKey = ck(0x1b, 0x37, 0x0c, 0x400);
const TABLE_KEY: CipherKey = ck(0x3e, 0x9b, 0x80, 0x400);

// edz type char → cipher, table at exe 0x4C78E0 (01 §6); 'M' and '*' are unused by th08.dat.
fn payload_key(tag: u8) -> Option<CipherKey> {
    Some(match tag {
        b'M' => ck(0x1b, 0x37, 0x40, 0x2800),
        b'T' => ck(0x51, 0xe9, 0x40, 0x3000),
        b'A' => ck(0xc1, 0x51, 0x1400, 0x2000),
        b'J' => ck(0x03, 0x19, 0x1400, 0x7800),
        b'E' => ck(0xab, 0xcd, 0x200, 0x1000),
        b'W' => ck(0x12, 0x34, 0x400, 0x2800),
        b'-' => ck(0x35, 0x97, 0x80, 0x2800),
        b'*' => ck(0x99, 0x37, 0x400, 0x1000),
        _ => return None,
    })
}

fn decrypt(src: &[u8], k: &CipherKey) -> Vec<u8> {
    crypt(src, k, false)
}

// Crypt_XorBlockReverse 0x43E1D0 (01 §2), also used by score.dat (15 §2.1); encrypt 0x43E4A0 is the inverse permutation.
pub fn block_decrypt(src: &[u8], key: u8, step: u8, block: usize, limit: usize) -> Vec<u8> {
    crypt(src, &ck(key, step, block, limit), false)
}

pub fn block_encrypt(src: &[u8], key: u8, step: u8, block: usize, limit: usize) -> Vec<u8> {
    crypt(src, &ck(key, step, block, limit), true)
}

// 01 §2 steps 1–4: the key runs on across blocks; the tail and bytes past `limit` are copied as-is.
fn crypt(src: &[u8], k: &CipherKey, enc: bool) -> Vec<u8> {
    let len = src.len();
    let mut out = src.to_vec();
    let rem = len % k.block;
    let tail = if rem < k.block / 4 { rem } else { 0 } + (len & 1);
    // n is signed in the original: a tail longer than the data (short odd input) deciphers nothing.
    let mut left = len.saturating_sub(tail);
    let mut budget = k.limit.cast_signed();
    let mut key = k.key;
    let mut pos = 0;
    while left > 0 && budget > 0 {
        // The spec's shrunk block persists, but a shrink only happens on the last block, so the min is equivalent.
        let b = left.min(k.block);
        let mut s = pos;
        for half in 0..2 {
            let mut d = pos + b - 1 - half;
            let n = if half == 0 { b.div_ceil(2) } else { b / 2 };
            for _ in 0..n {
                if enc {
                    out[s] = src[d] ^ key;
                } else {
                    out[d] = src[s] ^ key;
                }
                key = key.wrapping_add(k.step);
                s += 1;
                d = d.wrapping_sub(2);
            }
        }
        pos += b;
        left -= b;
        budget -= k.block.cast_signed();
    }
    out
}

// MSB-first bit reader (01 §3).
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    mask: u8,
    cur: u8,
}

impl Bits<'_> {
    fn bit(&mut self) -> u32 {
        if self.mask == 0x80 {
            self.cur = self.data.get(self.pos).copied().unwrap_or(0);
            self.pos += 1;
        }
        let b = u32::from(self.cur & self.mask != 0);
        self.mask >>= 1;
        if self.mask == 0 {
            self.mask = 0x80;
        }
        b
    }

    fn bits(&mut self, n: u32) -> u32 {
        (0..n).fold(0, |a, _| (a << 1) | self.bit())
    }
}

// Lzss_Decompress 0x4740E0 (01 §3): 0x2000-byte dictionary, cursor starts at 1, 13-bit offset / 4-bit length+3.
// The original never re-clears its global dictionary; 01 §3 says clearing it per call is byte-exact.
pub fn lzss_decompress(input: &[u8], size_hint: usize) -> Vec<u8> {
    let mut ring = [0u8; 0x2000];
    let mut rp = 1usize;
    let mut out = Vec::with_capacity(size_hint);
    let mut bs = Bits {
        data: input,
        pos: 0,
        mask: 0x80,
        cur: 0,
    };
    loop {
        // Past the end the spec reads 0 bits, i.e. a 0 flag and offset 0 = end of stream; stopping here is equivalent.
        if bs.pos > input.len() {
            break;
        }
        if bs.bit() != 0 {
            let c = bs.bits(8) as u8;
            out.push(c);
            ring[rp] = c;
            rp = (rp + 1) & 0x1fff;
        } else {
            let p = bs.bits(13) as usize;
            if p == 0 {
                break;
            }
            let l = bs.bits(4) as usize + 3;
            for i in 0..l {
                let c = ring[(p + i) & 0x1fff];
                out.push(c);
                ring[rp] = c;
                rp = (rp + 1) & 0x1fff;
            }
        }
    }
    out
}

struct BitWriter {
    out: Vec<u8>,
    cur: u8,
    mask: u8,
}

impl BitWriter {
    fn bits(&mut self, v: u32, n: u32) {
        for i in (0..n).rev() {
            if v >> i & 1 != 0 {
                self.cur |= self.mask;
            }
            self.mask >>= 1;
            if self.mask == 0 {
                self.out.push(self.cur);
                (self.cur, self.mask) = (0, 0x80);
            }
        }
    }
}

// LZSS encoder producing streams `lzss_decompress` reads back (greedy, hash chains over 3-byte prefixes).
// 01 §3: any valid stream is compatible with the decoder; byte-identical output is not required.
pub fn lzss_compress(input: &[u8]) -> Vec<u8> {
    const WINDOW: usize = 0x2000 - 18;
    let mut w = BitWriter {
        out: Vec::new(),
        cur: 0,
        mask: 0x80,
    };
    let key =
        |i: usize| (input[i] as usize) << 16 | (input[i + 1] as usize) << 8 | input[i + 2] as usize;
    let mut head: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut i = 0;
    while i < input.len() {
        let mut best = (0, 0);
        if i + 3 <= input.len()
            && let Some(c) = head.get(&key(i))
        {
            for &j in c.iter().rev() {
                if i - j > WINDOW {
                    break;
                }
                // Offset 0 is the end marker (01 §3), so a match at dictionary position 0 cannot be coded.
                if (j + 1) & 0x1fff == 0 {
                    continue;
                }
                let n = (0..18.min(input.len() - i))
                    .take_while(|&k| input[j + k] == input[i + k])
                    .count();
                if n > best.1 {
                    best = (j, n);
                    if n == 18 {
                        break;
                    }
                }
            }
        }
        let n = if best.1 >= 3 {
            w.bits(0, 1);
            w.bits(((best.0 + 1) & 0x1fff) as u32, 13);
            w.bits(best.1 as u32 - 3, 4);
            best.1
        } else {
            w.bits(1, 1);
            w.bits(u32::from(input[i]), 8);
            1
        };
        for k in i..i + n {
            if k + 3 <= input.len() {
                head.entry(key(k)).or_default().push(k);
            }
        }
        i += n;
    }
    w.bits(0, 1);
    w.bits(0, 13);
    if w.mask != 0x80 {
        w.out.push(w.cur);
    }
    w.out
}

// File-table entry (01 §5): absolute data offset, decompressed size incl. the 4-byte edz wrapper, and the next
// entry's offset (the table offset for the last one) as the end of the compressed data.
struct Entry {
    offset: usize,
    size: usize,
    end: usize,
}

pub struct Archive {
    data: Vec<u8>,
    entries: HashMap<String, Entry>,
    names: Vec<String>,
}

// 01 §4: the header dwords are stored biased (count + 123456, table offset + 345678, table size + 567891).
const COUNT_BIAS: u32 = 123_456;
const TABLE_OFF_BIAS: u32 = 345_678;
const TABLE_SIZE_BIAS: u32 = 567_891;

impl Archive {
    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        if data.len() < 16 || &data[0..4] != b"PBGZ" {
            return Err(ArchiveError::NotPbgz);
        }
        let hdr = decrypt(&data[4..16], &HEADER_KEY);
        // 01 §4: the open fails unless count > 0 and table offset < file size (signed count).
        let count = u32_at(&hdr, 0).wrapping_sub(COUNT_BIAS).cast_signed();
        let table_off = u32_at(&hdr, 4).wrapping_sub(TABLE_OFF_BIAS) as usize;
        let table_size = u32_at(&hdr, 8).wrapping_sub(TABLE_SIZE_BIAS) as usize;
        if count <= 0 || table_off >= data.len() {
            return Err(ArchiveError::BadHeader { count, table_off });
        }
        let count = count.cast_unsigned() as usize;
        let table = lzss_decompress(&decrypt(&data[table_off..], &TABLE_KEY), table_size);
        let mut raw = Vec::with_capacity(count);
        let mut p = 0;
        for _ in 0..count {
            let nul = table
                .get(p..)
                .and_then(|t| t.iter().position(|&c| c == 0))
                .ok_or(ArchiveError::BadTable)?;
            // PbgArchive_FindEntry 0x474C70 compares names case-insensitively (01 §5).
            let name = String::from_utf8_lossy(&table[p..p + nul]).to_ascii_lowercase();
            p += nul + 1;
            let mut r = Reader::at(&table, p);
            raw.push((name, r.u32()? as usize, r.u32()? as usize));
            // Skips offset, size and the unread reserved dword.
            p += 12;
        }
        let mut entries = HashMap::new();
        let mut names = Vec::new();
        for i in 0..raw.len() {
            let end = raw.get(i + 1).map_or(table_off, |r| r.1);
            let (name, offset, size) = raw[i].clone();
            names.push(name.clone());
            entries.insert(name, Entry { offset, size, end });
        }
        Ok(Self {
            data,
            entries,
            names,
        })
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    // FileSystem_OpenFile 0x43E660, archive path (01 §6, §7): LZSS, then the edz type cipher; the wrapper is dropped.
    // Callers pass bare names; the original strips any directory part before the lookup (01 §5).
    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        let file = self.read_stored(name)?;
        // 01 §6 step 4: without `edz` + a known type char the data is used as-is.
        match file.get(..4) {
            Some([b'e', b'd', b'z', t]) => match payload_key(*t) {
                Some(k) => Ok(decrypt(&file[4..], &k)),
                None => Ok(file),
            },
            _ => Ok(file),
        }
    }

    // The entry as stored after LZSS (01 §6 steps 1–3), `edz` wrapper and payload cipher intact; input for `pack`.
    pub fn read_stored(&self, name: &str) -> Result<Vec<u8>> {
        let e = self
            .entries
            .get(&name.to_ascii_lowercase())
            .ok_or_else(|| ArchiveError::NotFound(name.to_string()))?;
        Ok(lzss_decompress(&self.data[e.offset..e.end], e.size))
    }
}

// `edz` wrapper of 01 §6: "edz" + type char, then the payload under that type's cipher; None for an unknown type.
pub fn wrap_edz(tag: u8, payload: &[u8]) -> Option<Vec<u8>> {
    let k = payload_key(tag)?;
    let mut out = vec![b'e', b'd', b'z', tag];
    out.extend(crypt(payload, &k, true));
    Some(out)
}

// PBGZ writer (01 §4–§5): stored bytes (see `read_stored` / `wrap_edz`) LZSS-compressed in order, then the
// encrypted file table, then the biased header dwords. Any valid layout loads; byte-identical output to th08.dat
// is not a goal (01 §3).
pub fn pack(files: &[(&str, &[u8])]) -> Vec<u8> {
    pack_with(files.len() as u32, 0, files)
}

// `count` and the table-offset bias are free so tests can build invalid headers.
fn pack_with(count: u32, table_off_delta: i32, files: &[(&str, &[u8])]) -> Vec<u8> {
    let (h, t) = (HEADER_KEY, TABLE_KEY);
    let mut out = b"PBGZ".to_vec();
    out.resize(16, 0);
    let mut table = Vec::new();
    for (name, raw) in files {
        table.extend_from_slice(name.as_bytes());
        table.push(0);
        table.extend_from_slice(&(out.len() as u32).to_le_bytes());
        table.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        table.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&lzss_compress(raw));
    }
    let table_off = (out.len() as u32).wrapping_add_signed(table_off_delta);
    let table_size = table.len() as u32;
    out.extend_from_slice(&crypt(&lzss_compress(&table), &t, true));
    let hdr: Vec<u8> = [
        count.wrapping_add(COUNT_BIAS),
        table_off.wrapping_add(TABLE_OFF_BIAS),
        table_size.wrapping_add(TABLE_SIZE_BIAS),
    ]
    .iter()
    .flat_map(|v| v.to_le_bytes())
    .collect();
    out[4..16].copy_from_slice(&crypt(&hdr, &h, true));
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn lzss_and_cipher_round_trip() {
        let data: Vec<u8> = (0..20000u32)
            .map(|i| (i * 7 % 251) as u8 ^ (i / 300) as u8)
            .chain(std::iter::repeat_n(0, 5000))
            .collect();
        let c = super::lzss_compress(&data);
        assert!(c.len() < data.len());
        assert_eq!(super::lzss_decompress(&c, data.len()), data);
        let e = super::block_encrypt(&data, 0x59, 0x79, 0x100, 0xC00);
        assert_ne!(e, data);
        assert_eq!(super::block_decrypt(&e, 0x59, 0x79, 0x100, 0xC00), data);
    }

    fn build(count: u32, table_off_delta: i32, files: &[(&str, &[u8])]) -> Vec<u8> {
        super::pack_with(count, table_off_delta, files)
    }

    #[test]
    fn header_checks() {
        let f: &[(&str, &[u8])] = &[("a.txt", b"hello")];
        assert!(super::Archive::from_bytes(build(1, 0, f)).is_ok());
        assert!(super::Archive::from_bytes(build(0, 0, f)).is_err());
        // Count −1.
        assert!(super::Archive::from_bytes(build(u32::MAX, 0, f)).is_err());
        assert!(super::Archive::from_bytes(build(1, 0x10000, f)).is_err());
    }

    #[test]
    fn edz_wrapper_optional() {
        let key = super::payload_key(b'T').unwrap();
        let body = b"some text payload, long enough to span a cipher block or two......".repeat(4);
        let mut wrapped = b"edzT".to_vec();
        wrapped.extend(super::block_encrypt(
            &body, key.key, key.step, key.block, key.limit,
        ));
        let f: &[(&str, &[u8])] = &[
            ("w.txt", &wrapped),
            ("plain.bin", b"no wrapper here"),
            ("odd.bin", b"edzQrest"),
            ("x", b"ed"),
        ];
        let a = super::Archive::from_bytes(build(f.len() as u32, 0, f)).unwrap();
        assert_eq!(a.read("W.TXT").unwrap(), body);
        assert_eq!(a.read("plain.bin").unwrap(), b"no wrapper here");
        assert_eq!(a.read("odd.bin").unwrap(), b"edzQrest");
        assert_eq!(a.read("x").unwrap(), b"ed");
    }

    // 01 §4–§6: pack → load gives every file back, wrapped ones decrypted.
    #[test]
    fn pack_round_trip() {
        let body = b"payload under the J cipher ".repeat(300);
        let w = super::wrap_edz(b'J', &body).unwrap();
        assert!(super::wrap_edz(b'Q', &body).is_none());
        let f: &[(&str, &[u8])] = &[("Data.ANM", &w), ("plain.txt", b"plain"), ("e.bin", b"")];
        let a = super::Archive::from_bytes(super::pack(f)).unwrap();
        assert_eq!(a.names(), ["data.anm", "plain.txt", "e.bin"]);
        assert_eq!(a.read("data.anm").unwrap(), body);
        assert_eq!(a.read_stored("data.anm").unwrap(), w);
        assert_eq!(a.read("plain.txt").unwrap(), b"plain");
        assert_eq!(a.read("e.bin").unwrap(), b"");
        assert!(matches!(a.read("x"), Err(super::ArchiveError::NotFound(_))));
    }

    // Repack of real th08.dat entries (one per edz type in use): stored bytes and payloads survive.
    #[test]
    fn repack_real_files() {
        let Some(ar) = crate::testdata::archive() else {
            return;
        };
        let names = [
            "ecldata1.ecl",
            "text.anm",
            "msg1a.dat",
            "stage1.std",
            "ply00a.sht",
            "musiccmt.txt",
            "demorpy0.rpy",
        ];
        let stored: Vec<Vec<u8>> = names.iter().map(|n| ar.read_stored(n).unwrap()).collect();
        let f: Vec<(&str, &[u8])> = names
            .iter()
            .copied()
            .zip(stored.iter().map(Vec::as_slice))
            .collect();
        let b = super::Archive::from_bytes(super::pack(&f)).unwrap();
        for n in names {
            assert_eq!(b.read_stored(n).unwrap(), ar.read_stored(n).unwrap(), "{n}");
            assert_eq!(b.read(n).unwrap(), ar.read(n).unwrap(), "{n}");
        }
    }
}
