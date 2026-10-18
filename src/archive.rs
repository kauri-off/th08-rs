use anyhow::{Context, Result, bail};
use std::collections::HashMap;

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

const HEADER_KEY: CipherKey = ck(0x1b, 0x37, 0x0c, 0x400);
const TABLE_KEY: CipherKey = ck(0x3e, 0x9b, 0x80, 0x400);

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
    let len = src.len();
    let mut out = src.to_vec();
    let rem = len % k.block;
    let tail = if rem < k.block / 4 { rem } else { 0 } + (len & 1);
    let mut left = len - tail;
    let mut budget = k.limit as isize;
    let mut key = k.key;
    let mut pos = 0;
    while left > 0 && budget > 0 {
        let b = left.min(k.block);
        let mut s = pos;
        for half in 0..2 {
            let mut d = pos + b - 1 - half;
            let n = if half == 0 { b.div_ceil(2) } else { b / 2 };
            for _ in 0..n {
                out[d] = src[s] ^ key;
                key = key.wrapping_add(k.step);
                s += 1;
                d = d.wrapping_sub(2);
            }
        }
        pos += b;
        left -= b;
        budget -= k.block as isize;
    }
    out
}

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
        let b = (self.cur & self.mask != 0) as u32;
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

fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

impl Archive {
    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        if &data[0..4] != b"PBGZ" {
            bail!("not a PBGZ archive");
        }
        let hdr = decrypt(&data[4..16], &HEADER_KEY);
        let count = u32le(&hdr, 0).wrapping_sub(123456) as usize;
        let table_off = u32le(&hdr, 4).wrapping_sub(345678) as usize;
        let table_size = u32le(&hdr, 8).wrapping_sub(567891) as usize;
        let table = lzss_decompress(&decrypt(&data[table_off..], &TABLE_KEY), table_size);
        let mut raw = Vec::with_capacity(count);
        let mut p = 0;
        for _ in 0..count {
            let nul = table[p..]
                .iter()
                .position(|&c| c == 0)
                .context("bad table")?;
            let name = String::from_utf8_lossy(&table[p..p + nul]).to_ascii_lowercase();
            p += nul + 1;
            raw.push((
                name,
                u32le(&table, p) as usize,
                u32le(&table, p + 4) as usize,
            ));
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

    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        let e = self
            .entries
            .get(&name.to_ascii_lowercase())
            .with_context(|| format!("{name} not in archive"))?;
        let file = lzss_decompress(&self.data[e.offset..e.end], e.size);
        if file.len() < 4 || &file[0..3] != b"edz" {
            bail!("{name}: missing edz wrapper");
        }
        let k = payload_key(file[3])
            .with_context(|| format!("{name}: unknown edz type {}", file[3]))?;
        Ok(decrypt(&file[4..], &k))
    }
}

#[cfg(test)]
pub fn test_archive() -> Option<Archive> {
    let dir = std::env::var("TH08_DIR").ok()?;
    Archive::from_bytes(std::fs::read(format!("{dir}/th08.dat")).ok()?).ok()
}
