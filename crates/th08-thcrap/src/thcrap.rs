//! thcrap / Touhou Patch Center import (I18N G3; file list I18N §1.2, key rules I18N_STRINGS §3): a `lang_xx`
//! folder of JSON patch files → the flat `{"<TextRef key>": "<text>"}` tables of a presentation pack (I18N §2.5).
//! Patch files are untrusted data: only JSON is parsed, images are only decoded, nothing is executed.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;
use th08_core::builtin::Builtin;
use th08_core::drawlist::{Align, TextSite};
use th08_core::textref::{END_FILES, MSG_FILES, SpellField, TextRef};
use th08_formats::msg::{MsgFile, MsgOff};
use th08_formats::musiccmt::{self, Entry};
use th08_formats::sjis;

// Card records 0–221 (ECL op 122 +0x0E card id, 07 §11.1; 222 CATK records, 15 §2).
pub const CARDS: u16 = 222;
// Largest patch file read (the biggest th08 file, spellcomments.js, is ~60 KB).
const MAX_JSON: u64 = 16 << 20;
const MAX_IMAGE: u64 = 32 << 20;

// The vanilla text of th08.dat that thcrap keys refer to: msg files (10 §2), ending scripts (16 §1.3), musiccmt.txt
// (16 §5.1).
pub struct Vanilla {
    pub msg: Vec<Option<MsgFile>>,
    pub end: Vec<Option<Vec<u8>>>,
    pub music: Vec<Entry>,
}

impl Vanilla {
    // th08.dat or a th08.pak repacked from it (by its name).
    pub fn from_dat(path: &Path) -> Result<Self> {
        let raw = crate::vfs::read(path).with_context(|| format!("read {}", path.display()))?;
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        Self::from_bytes(&name, raw)
    }

    // The content file of a game folder (th08.pak first, then th08.dat, any letter case).
    pub fn from_game_dir(dir: &Path) -> Result<Self> {
        for n in th08_core::content::CONTENT_FILES {
            let found = crate::vfs::list(dir).into_iter().flatten().find(|p| {
                crate::vfs::is_file(p)
                    && p.file_name()
                        .is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case(n))
            });
            if let Some(p) = found {
                return Self::from_dat(&p);
            }
        }
        anyhow::bail!("{}: no th08.dat or th08.pak", dir.display())
    }

    // `name` = the content file's name: `.pak` = the repacked th08.pak, else the original archive.
    pub fn from_bytes(name: &str, raw: Vec<u8>) -> Result<Self> {
        use th08_core::content::{Content, LogicView};
        let content = if name.to_ascii_lowercase().ends_with(".pak") {
            Content::Pack(th08_formats::pack::Pack::from_bytes(raw)?)
        } else {
            Content::Archive(th08_formats::archive::Archive::from_bytes(raw)?)
        };
        let ar = LogicView::new(content);
        let msg = MSG_FILES
            .iter()
            .map(|n| {
                ar.read(&format!("{n}.dat"))
                    .ok()
                    .and_then(|b| th08_formats::msg::parse(b).ok())
            })
            .collect();
        let end = END_FILES.iter().map(|n| ar.read(n).ok()).collect();
        let music = ar
            .read("musiccmt.txt")
            .map(|b| musiccmt::parse_musiccmt(&b))
            .unwrap_or_default();
        Ok(Self { msg, end, music })
    }

    // Every keyed vanilla text slot with its decoded text: msg text ops, ending text lines, music titles and
    // comments, builtins. Spell texts live in ECL (07 §11.1) and are keyed by card id only (see `spell_keys`).
    pub fn keys(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for (fi, f) in self.msg.iter().enumerate() {
            let Some(f) = f else { continue };
            for e in unique_entries(f) {
                for (nth, s) in text_ops(f, f.entries[e]) {
                    let k = TextRef::Msg {
                        file: fi as u8,
                        entry: e as u16,
                        nth,
                    };
                    out.extend(k.key().map(|k| (k, s)));
                }
            }
        }
        for (fi, d) in self.end.iter().enumerate() {
            let Some(d) = d else { continue };
            for (line, s) in end_lines(d).into_iter().enumerate() {
                if is_end_text(&s) {
                    let k = TextRef::Ending {
                        file: fi as u8,
                        line: line as u16,
                    };
                    out.extend(k.key().map(|k| (k, sjis::decode(&s))));
                }
            }
        }
        for (t, e) in self.music.iter().enumerate() {
            let k = TextRef::MusicTitle { track: t as u8 };
            out.extend(k.key().map(|k| (k, e.title.decode())));
            for (line, c) in e.comments.iter().enumerate() {
                let k = TextRef::MusicCmt {
                    track: t as u8,
                    line: line as u8,
                };
                out.extend(k.key().map(|k| (k, c.decode())));
            }
        }
        for b in Builtin::all() {
            out.insert(format!("builtin/{}", b.key()), b.text().to_string());
        }
        out
    }
}

// Entries in index order, each offset once (the lowest index keys a shared offset, 10 §2, I18N_STRINGS §3.1).
fn unique_entries(f: &MsgFile) -> Vec<usize> {
    (0..f.entries.len())
        .filter(|&i| f.entries.iter().position(|&o| o == f.entries[i]) == Some(i))
        .collect()
}

// The instructions of an entry from its offset up to op 0 (10 §2), bounded against looping chains.
fn entry_ins(f: &MsgFile, start: MsgOff) -> Vec<th08_formats::msg::Ins> {
    let mut out = Vec::new();
    let mut p = start;
    while let Some(i) = f.ins(p) {
        if i.op == 0 || out.len() > 100_000 {
            break;
        }
        out.push(i);
        p = i.next();
    }
    out
}

fn is_text_op(op: u8) -> bool {
    matches!(op, 3 | 16 | 19 | 20)
}

// (nth, decoded text) of every text op of the entry at `start` (the `nth` of `MsgFile::text_ops_before`).
fn text_ops(f: &MsgFile, start: MsgOff) -> Vec<(u16, String)> {
    let mut n = 0u16;
    let mut out = Vec::new();
    for i in entry_ins(f, start) {
        if is_text_op(i.op) {
            let from = if i.op == 3 { 4 } else { 0 };
            out.push((n, f.string(&i, from).decode()));
            n += 1;
        }
    }
    out
}

// A thcrap text box: its jdiff key and the text-op numbers (`nth`) it fills, in order.
#[derive(Debug, PartialEq, Eq)]
pub struct MsgBox {
    pub key: String,
    pub slots: Vec<u16>,
}

// thcrap `msg08` box keys of one entry (I18N_STRINGS §3.1): ops 4/15 close the box; ops 16/19/20 open one when none is
// open or the time differs, else append; op 3 opens one on line 0, else appends to the current box; key =
// `<start time>_<k>` with k = boxes already started at that time. Op 8 keys (`_h1_`) are returned separately: msg08
// has no op 8 and its string is ignored (10 §5.1), so they take no slot.
pub fn msg_boxes(f: &MsgFile, start: MsgOff) -> (Vec<MsgBox>, Vec<String>) {
    let mut boxes: Vec<(u16, MsgBox)> = Vec::new();
    let mut h1 = Vec::new();
    let mut per_time: HashMap<u16, u32> = HashMap::new();
    let mut h1_time: HashMap<u16, u32> = HashMap::new();
    let (mut open, mut cur): (Option<usize>, Option<usize>) = (None, None);
    let mut nth = 0u16;
    let mut new_box = |boxes: &mut Vec<(u16, MsgBox)>, t: u16| {
        let k = per_time.entry(t).or_insert(0);
        boxes.push((
            t,
            MsgBox {
                key: format!("{t}_{k}"),
                slots: Vec::new(),
            },
        ));
        *k += 1;
        boxes.len() - 1
    };
    for i in entry_ins(f, start) {
        match i.op {
            4 | 15 => open = None,
            16 | 19 | 20 => {
                let b = match open {
                    Some(b) if boxes[b].0 == i.time => b,
                    _ => new_box(&mut boxes, i.time),
                };
                boxes[b].1.slots.push(nth);
                (open, cur) = (Some(b), Some(b));
            }
            3 => {
                let line = f.i16(&i, 1);
                let b = match cur {
                    Some(b) if line != 0 => b,
                    _ => new_box(&mut boxes, i.time),
                };
                boxes[b].1.slots.push(nth);
                (open, cur) = (Some(b), Some(b));
            }
            8 => {
                let k = h1_time.entry(i.time).or_insert(0);
                h1.push(format!("{}_h1_{k}", i.time));
                *k += 1;
            }
            _ => {}
        }
        if is_text_op(i.op) {
            nth += 1;
        }
    }
    (boxes.into_iter().map(|b| b.1).collect(), h1)
}

// LF-split lines of an .end script without CR and trailing NULs (the physical lines of `Script::line_no`).
fn end_lines(d: &[u8]) -> Vec<Vec<u8>> {
    d.split(|&b| b == b'\n')
        .map(|l| {
            let mut l = l.to_vec();
            while matches!(l.last(), Some(0 | b'\r')) {
                l.pop();
            }
            l
        })
        .collect()
}

// A text line of an .end script: not a command, not empty (16 §1.3; I18N_STRINGS §3.2).
fn is_end_text(l: &[u8]) -> bool {
    !l.is_empty() && l[0] != b'@' && l[0] != 0
}

// Blocks of consecutive text lines: (first line, all its lines) — the thcrap end jdiff key is the first line.
pub fn end_blocks(d: &[u8]) -> Vec<(usize, Vec<usize>)> {
    let lines = end_lines(d);
    let mut out: Vec<(usize, Vec<usize>)> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if !is_end_text(l) {
            continue;
        }
        match out.last_mut() {
            Some((_, b)) if b.last() == Some(&(i - 1)) && i > 0 => b.push(i),
            _ => out.push((i, vec![i])),
        }
    }
    out
}

// thcrap layout markup flattened to plain text (I18N §5: flatten at import): `<tag$a$b…>` → `a`; an empty first field
// (a continuation tab stop, `<l$$…>`) → the width of the last label in spaces; tabs → one space.
pub fn flatten(s: &str, last_label: &mut usize) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(p) = rest.find('<') {
        out.push_str(&rest[..p]);
        let tail = &rest[p..];
        let tag_end = tail[1..]
            .find(|c: char| !c.is_ascii_alphabetic())
            .map(|n| n + 1);
        let close = tail.find('>');
        match (tag_end, close) {
            (Some(t), Some(c)) if t > 1 && tail[t..].starts_with('$') && t < c => {
                let first = tail[t + 1..c].split('$').next().unwrap_or("");
                if first.is_empty() {
                    out.extend(std::iter::repeat_n(' ', *last_label));
                } else {
                    out.push_str(first);
                    out.push(' ');
                    *last_label = first.chars().count() + 1;
                }
                rest = tail[c + 1..].trim_start_matches(' ');
            }
            _ => {
                out.push('<');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out.replace('\t', " ").trim_end().to_string()
}

// A thcrap string shown as plain text: the original draws every text through `vsprintf` (10 §6), so thcrap writes a
// literal `%` as `%%`; the pack text is never formatted again, so it is unescaped here.
fn plain(s: &str, last_label: &mut usize) -> String {
    flatten(s, last_label).replace("%%", "%")
}

// printf conversions of a format (flags, width, precision, conversion), `%%` excluded; Err on a conversion the port's
// `sprintf` does not implement (textref.rs: `%s`, `%d`).
pub fn printf_specs(s: &str) -> Result<Vec<String>, String> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'%' {
            i += 1;
            continue;
        }
        let st = i;
        i += 1;
        while i < b.len() && (b"-0+ #".contains(&b[i]) || b[i].is_ascii_digit() || b[i] == b'.') {
            i += 1;
        }
        match b.get(i) {
            Some(b'%') if i == st + 1 => {}
            Some(b'd' | b's') => out.push(s[st..=i].to_string()),
            _ => return Err(s[st..(i + 1).min(s.len())].to_string()),
        }
        i += 1;
    }
    Ok(out)
}

// Same conversions in the same order (width and flags may differ: they only move pixels).
pub fn printf_compatible(vanilla: &str, tr: &str) -> Result<(), String> {
    let conv =
        |v: Vec<String>| -> Vec<char> { v.iter().filter_map(|s| s.chars().last()).collect() };
    let a = printf_specs(vanilla).map_err(|e| format!("vanilla bad spec {e}"))?;
    let b = printf_specs(tr).map_err(|e| format!("unsupported spec {e:?}"))?;
    if conv(a.clone()) == conv(b.clone()) {
        Ok(())
    } else {
        Err(format!("specifiers {b:?} ≠ vanilla {a:?}"))
    }
}

// The JSON files of one `lang_xx` folder (`<lang>/stringdefs.js`, `<lang>/themes.js`, `<lang>/th08/…`).
pub struct Lang {
    pub dir: PathBuf,
    pub layers: Vec<PathBuf>,
    pub stringdefs: serde_json::Map<String, Value>,
    pub themes: serde_json::Map<String, Value>,
    pub spells: serde_json::Map<String, Value>,
    pub comments: serde_json::Map<String, Value>,
    pub musiccmt: serde_json::Map<String, Value>,
    // `msgNx` → jdiff object; `end0Nx.end` → jdiff object.
    pub msg: BTreeMap<String, serde_json::Map<String, Value>>,
    pub end: BTreeMap<String, serde_json::Map<String, Value>>,
}

fn read_json(p: &Path) -> Result<Option<serde_json::Map<String, Value>>> {
    let Some(len) = crate::vfs::len(p) else {
        return Ok(None);
    };
    anyhow::ensure!(len <= MAX_JSON, "{}: too large", p.display());
    let b = crate::vfs::read(p)?;
    // thcrap JSON may carry a UTF-8 BOM.
    let b = b.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&b);
    match serde_json::from_slice::<Value>(b) {
        Ok(Value::Object(m)) => Ok(Some(m)),
        Ok(_) => anyhow::bail!("{}: not a JSON object", p.display()),
        Err(e) => anyhow::bail!("{}: {e}", p.display()),
    }
}

// thcrap's patch stacking: a later layer's object keys replace the earlier ones', nested objects merge.
fn merge(base: &mut serde_json::Map<String, Value>, over: serde_json::Map<String, Value>) {
    for (k, v) in over {
        match (base.get_mut(&k), v) {
            (Some(Value::Object(b)), Value::Object(o)) => merge(b, o),
            (_, v) => {
                base.insert(k, v);
            }
        }
    }
}

// One JSON file of every layer, merged; None when no layer has it.
fn read_layered(layers: &[PathBuf], rel: &str) -> Result<Option<serde_json::Map<String, Value>>> {
    let mut out: Option<serde_json::Map<String, Value>> = None;
    for l in layers {
        if let Some(m) = read_json(&l.join(rel))? {
            match &mut out {
                Some(b) => merge(b, m),
                None => out = Some(m),
            }
        }
    }
    Ok(out)
}

impl Lang {
    pub fn read(dir: &Path) -> Result<Self> {
        Self::read_layers(&[dir.to_path_buf()])
    }

    // A language over the languages it builds on (`layers`: base first, the language last).
    pub fn read_layers(layers: &[PathBuf]) -> Result<Self> {
        // No th08 folder = nothing translated (every key falls back to the original).
        let obj = |rel: &str| read_layered(layers, rel).map(Option::unwrap_or_default);
        let mut msg = BTreeMap::new();
        for n in MSG_FILES {
            if let Some(m) = read_layered(layers, &format!("th08/{n}.dat.jdiff"))? {
                msg.insert(n.to_string(), m);
            }
        }
        let mut end = BTreeMap::new();
        for n in END_FILES {
            if let Some(m) = read_layered(layers, &format!("th08/{n}.jdiff"))? {
                end.insert(n.to_string(), m);
            }
        }
        Ok(Self {
            dir: layers.last().cloned().unwrap_or_default(),
            layers: layers.to_vec(),
            stringdefs: obj("stringdefs.js")?,
            themes: obj("themes.js")?,
            spells: obj("th08/spells.js")?,
            comments: obj("th08/spellcomments.js")?,
            musiccmt: obj("th08/musiccmt.js")?,
            msg,
            end,
        })
    }

    // Texture replacements (I18N §1.2, §2.4): `th08/data/**.png|jpg` → `images/<path under data>`, `th08/*.jpg|png`
    // → `files/<name>`. Each must decode as an image; names are restricted to [A-Za-z0-9_.-/].
    // A later layer's image replaces an earlier one of the same name.
    pub fn images(&self, report: &mut Report) -> Vec<(String, Vec<u8>)> {
        let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for l in &self.layers {
            out.extend(images(l, report));
        }
        out.into_iter().collect()
    }
}

// Texture replacements of the thcrap folder `dir` (see `Lang::images`).
pub fn images(dir: &Path, report: &mut Report) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let g = dir.join("th08");
    let mut stack = vec![g.clone()];
    while let Some(d) = stack.pop() {
        let Ok(mut ents) = crate::vfs::list(&d) else {
            continue;
        };
        ents.sort();
        for p in ents {
            match crate::vfs::kind(&p) {
                Some(crate::vfs::Kind::Dir) => {
                    stack.push(p);
                    continue;
                }
                Some(crate::vfs::Kind::File) => {}
                _ => continue,
            }
            let rel = p
                .strip_prefix(&g)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            let lower = rel.to_ascii_lowercase();
            if !(lower.ends_with(".png") || lower.ends_with(".jpg")) {
                continue;
            }
            if !rel
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_.-/".contains(c))
            {
                report.add("image", format!("{rel}: odd name, skipped"));
                continue;
            }
            let dst = match rel.strip_prefix("data/") {
                Some(r) => format!("images/{}", strip_version(r)),
                None if !rel.contains('/') => format!("files/{rel}"),
                None => {
                    report.add("image", format!("{rel}: outside data/, skipped"));
                    continue;
                }
            };
            let ok = crate::vfs::len(&p).is_some_and(|n| n <= MAX_IMAGE);
            match crate::vfs::read(&p) {
                Ok(b) if ok && image::load_from_memory(&b).is_ok() => out.push((dst, b)),
                _ => report.add("image", format!("{rel}: not a decodable image, skipped")),
            }
        }
    }
    out
}

// `title02.v1.00d.png` → `title02.png` (thcrap per-version files, I18N §2.4).
fn strip_version(p: &str) -> String {
    match p.find(".v1.") {
        Some(i) => {
            let ext = p.rsplit('.').next().unwrap_or("png");
            format!("{}.{ext}", &p[..i])
        }
        None => p.to_string(),
    }
}

// Import findings by category, printed and saved next to the pack.
#[derive(Default, Debug)]
pub struct Report {
    pub items: BTreeMap<&'static str, Vec<String>>,
}

impl Report {
    pub fn add(&mut self, cat: &'static str, s: String) {
        self.items.entry(cat).or_default().push(s);
    }

    pub fn count(&self, cat: &str) -> usize {
        self.items.get(cat).map_or(0, Vec::len)
    }

    // Counts per category, then up to `max` items of each (`usize::MAX` = all).
    pub fn render(&self, max: usize) -> String {
        let mut s = String::new();
        for (c, v) in &self.items {
            s.push_str(&format!("{c}: {}\n", v.len()));
            for i in v.iter().take(max) {
                s.push_str(&format!("  {i}\n"));
            }
            if v.len() > max {
                s.push_str(&format!("  … {} more\n", v.len() - max));
            }
        }
        s
    }
}

// Translated pack tables: file name under `text/` → key → text.
pub type Tables = BTreeMap<&'static str, BTreeMap<String, String>>;

fn str_lines(v: &Value) -> Option<Vec<String>> {
    match v {
        Value::String(s) => Some(vec![s.clone()]),
        Value::Array(a) => a.iter().map(|x| x.as_str().map(str::to_string)).collect(),
        _ => None,
    }
}

// Fill `slots` (keys of fixed line slots) with `lines` (I18N §2.1: a translation never adds or drops ops): extra lines
// are joined into the last slot with a space, missing ones become " " so no Japanese is left beside the translation.
fn fit_lines(
    table: &mut BTreeMap<String, String>,
    slots: &[String],
    lines: &[String],
    what: &str,
    report: &mut Report,
) {
    if slots.is_empty() {
        return;
    }
    let n = slots.len();
    if lines.len() > n {
        report.add(
            "joined",
            format!("{what}: {} lines into {n} slots", lines.len()),
        );
    } else if lines.len() < n {
        report.add(
            "padded",
            format!("{what}: {} lines for {n} slots", lines.len()),
        );
    }
    for (i, k) in slots.iter().enumerate() {
        let t = if i + 1 == n && lines.len() > n {
            let rest = lines[i + 1..].iter().map(|l| l.trim_start());
            std::iter::once(lines[i].as_str())
                .chain(rest)
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            lines.get(i).cloned().unwrap_or_default()
        };
        let t = if t.trim().is_empty() {
            " ".to_string()
        } else {
            t
        };
        table.insert(k.clone(), t);
    }
}

// The whole mapping: thcrap lang files + vanilla th08.dat → pack tables (I18N §2.3, I18N_STRINGS §3).
pub fn import(lang: &Lang, v: &Vanilla, report: &mut Report) -> Tables {
    let mut t = Tables::new();
    t.insert("msg.json", import_msg(lang, v, report));
    t.insert("end.json", import_end(lang, v, report));
    t.insert("spell.json", import_spells(lang, report));
    t.insert("music.json", import_music(lang, v, report));
    t.insert("builtin.json", import_builtins(lang, report));
    t
}

fn import_msg(lang: &Lang, v: &Vanilla, report: &mut Report) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (name, jd) in &lang.msg {
        let Some(fi) = MSG_FILES.iter().position(|n| n == name) else {
            continue;
        };
        let Some(f) = v.msg.get(fi).and_then(Option::as_ref) else {
            report.add("unmapped", format!("{name}: not in th08.dat"));
            continue;
        };
        for (e, boxes) in jd {
            let Some(e_idx) = e.parse::<usize>().ok().filter(|&i| i < f.entries.len()) else {
                report.add("unmapped", format!("{name} entry {e}: no such entry"));
                continue;
            };
            let off = f.entries[e_idx];
            let lowest = f.entries.iter().position(|&o| o == off).unwrap_or(e_idx);
            if lowest != e_idx {
                report.add(
                    "shared-entry",
                    format!("{name} entry {e} → keyed as entry {lowest}"),
                );
            }
            let (vboxes, h1) = msg_boxes(f, off);
            let Some(boxes) = boxes.as_object() else {
                report.add("unmapped", format!("{name}/{e}: not an object"));
                continue;
            };
            for (bk, bv) in boxes {
                if bk.contains("_h1_") {
                    let known = if h1.contains(bk) {
                        ""
                    } else {
                        " (no op 8 there)"
                    };
                    report.add("h1-dropped", format!("{name}/{e}/{bk}{known}"));
                    continue;
                }
                let Some(vb) = vboxes.iter().find(|b| &b.key == bk) else {
                    report.add("unmapped", format!("{name}/{e}/{bk}: no such box"));
                    continue;
                };
                let Some(lines) = bv.get("lines").and_then(str_lines) else {
                    report.add("unmapped", format!("{name}/{e}/{bk}: no lines"));
                    continue;
                };
                let mut lab = 0;
                let lines: Vec<String> = lines.iter().map(|l| plain(l, &mut lab)).collect();
                let slots: Vec<String> = vb
                    .slots
                    .iter()
                    .filter_map(|&nth| {
                        TextRef::Msg {
                            file: fi as u8,
                            entry: lowest as u16,
                            nth,
                        }
                        .key()
                    })
                    .collect();
                fit_lines(
                    &mut out,
                    &slots,
                    &lines,
                    &format!("{name}/{e}/{bk}"),
                    report,
                );
            }
        }
    }
    out
}

fn import_end(lang: &Lang, v: &Vanilla, report: &mut Report) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (name, jd) in &lang.end {
        let Some(fi) = END_FILES.iter().position(|n| n == name) else {
            continue;
        };
        let Some(d) = v.end.get(fi).and_then(Option::as_ref) else {
            report.add("unmapped", format!("{name}: not in th08.dat"));
            continue;
        };
        let blocks = end_blocks(d);
        for (k, bv) in jd {
            let block = k
                .parse::<usize>()
                .ok()
                .and_then(|l| blocks.iter().find(|b| b.0 == l));
            let Some((_, lines_at)) = block else {
                report.add(
                    "unmapped",
                    format!("{name}/{k}: not the first line of a text block"),
                );
                continue;
            };
            let Some(lines) = bv.get("lines").and_then(str_lines) else {
                report.add("unmapped", format!("{name}/{k}: no lines"));
                continue;
            };
            let mut lab = 0;
            let lines: Vec<String> = lines.iter().map(|l| plain(l, &mut lab)).collect();
            let slots: Vec<String> = lines_at
                .iter()
                .filter_map(|&l| {
                    TextRef::Ending {
                        file: fi as u8,
                        line: l as u16,
                    }
                    .key()
                })
                .collect();
            fit_lines(&mut out, &slots, &lines, &format!("{name}/{k}"), report);
        }
    }
    out
}

fn spell_key(card: u16, field: SpellField) -> String {
    TextRef::Spell { card, field }.key().unwrap_or_default()
}

fn import_spells(lang: &Lang, report: &mut Report) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let card = |k: &str| k.parse::<u16>().ok().filter(|&c| c < CARDS);
    for (k, val) in &lang.spells {
        match (card(k), val.as_str()) {
            (Some(c), Some(s)) => {
                out.insert(spell_key(c, SpellField::Name), plain(s, &mut 0));
            }
            _ => report.add("unmapped", format!("spells.js {k}")),
        }
    }
    for (k, val) in &lang.comments {
        let Some(c) = card(k) else {
            report.add("unmapped", format!("spellcomments.js {k}"));
            continue;
        };
        if let Some(o) = val.get("owner").and_then(Value::as_str) {
            out.insert(spell_key(c, SpellField::Owner), plain(o, &mut 0));
        }
        if let Some(lines) = val.get("comment_1").and_then(str_lines) {
            let lines: Vec<String> = lines.iter().map(|l| plain(l, &mut 0)).collect();
            let slots = [0, 1].map(|i| spell_key(c, SpellField::Comment(i)));
            fit_lines(
                &mut out,
                &slots,
                &lines,
                &format!("spellcomments {k}"),
                report,
            );
        }
    }
    out
}

fn import_music(lang: &Lang, v: &Vanilla, report: &mut Report) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let n = v.music.len();
    // themes.js `th08_NN`: NN = music-room entry + 1 (I18N_STRINGS §3.4).
    let mut titles: HashMap<usize, String> = HashMap::new();
    for (k, val) in &lang.themes {
        let Some(nn) = k.strip_prefix("th08_") else {
            continue;
        };
        // `th08_08/en`-style keys are alternative titles of another patch variant.
        if nn.contains('/') {
            report.add("theme-variant", format!("themes.js {k}: skipped"));
            continue;
        }
        match (
            nn.parse::<usize>().ok().filter(|&i| (1..=n).contains(&i)),
            val.as_str(),
        ) {
            (Some(i), Some(s)) => {
                let s = plain(s, &mut 0);
                if let Some(k) = (TextRef::MusicTitle {
                    track: (i - 1) as u8,
                })
                .key()
                {
                    out.insert(k, s.clone());
                }
                titles.insert(i - 1, s);
            }
            _ => report.add("unmapped", format!("themes.js {k}")),
        }
    }
    // musiccmt.js `"N"`: entry N − 1; `lines[k]` = comment k; `"@"` = the numbered title line, rebuilt from themes
    // as thcrap's `Music Room Numbered Title` does.
    for (k, val) in &lang.musiccmt {
        let Some(t) = k
            .parse::<usize>()
            .ok()
            .filter(|&i| (1..=n).contains(&i))
            .map(|i| i - 1)
        else {
            report.add("unmapped", format!("musiccmt.js {k}"));
            continue;
        };
        let Some(lines) = str_lines(val) else {
            report.add("unmapped", format!("musiccmt.js {k}: no lines"));
            continue;
        };
        let lines: Vec<String> = lines
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                if l == "@" {
                    if i != 0 {
                        return None;
                    }
                    return Some(titles.get(&t).map_or_else(
                        || {
                            v.music[t]
                                .comments
                                .first()
                                .map(sjis::SjisString::decode)
                                .unwrap_or_default()
                        },
                        |s| format!("No.{:>2}  {s}", t + 1),
                    ));
                }
                Some(plain(l, &mut 0))
            })
            .collect();
        let slots: Vec<String> = (0..v.music[t].comments.len())
            .filter_map(|line| {
                TextRef::MusicCmt {
                    track: t as u8,
                    line: line as u8,
                }
                .key()
            })
            .collect();
        fit_lines(&mut out, &slots, &lines, &format!("musiccmt {k}"), report);
    }
    out
}

// stringlocs id of a builtin: the `Def::thcrap` column, plus the 1-based `th08 Spell Condition Line L N…` ids of the
// Last Word hints (card number = card id + 1, I18N_STRINGS §3.5).
fn builtin_id(b: Builtin, defs: &serde_json::Map<String, Value>) -> Option<&str> {
    if let Builtin::SpHint { card, line } = b {
        let prefix = format!("th08 Spell Condition Line {} ", line + 1);
        let num = (u16::from(card) + 1).to_string();
        return defs
            .keys()
            .find(|k| {
                k.strip_prefix(&prefix)
                    .is_some_and(|r| r.split(' ').any(|n| n == num))
            })
            .map(String::as_str);
    }
    let id = b.def().thcrap;
    (!id.is_empty() && defs.contains_key(id)).then_some(id)
}

// Equivalent for our values: a vanilla literal whose ASCII numbers the translation writes as `%d` conversions (Last
// Word hints: `No.138` vs thcrap's `No.%.3d`, I18N_STRINGS §3.5). Each conversion must print its number's digits
// exactly; the translation is then filled in at import.
fn fill_numbers(vanilla: &str, tr: &str) -> Option<String> {
    use th08_core::textref::{FmtArg, sprintf};
    if !printf_specs(vanilla).ok()?.is_empty() {
        return None;
    }
    let specs = printf_specs(tr).ok()?;
    let nums: Vec<&str> = vanilla
        .split(|c: char| !c.is_ascii_digit())
        .filter(|n| !n.is_empty())
        .collect();
    if specs.is_empty() || specs.len() != nums.len() {
        return None;
    }
    let mut args = Vec::new();
    for (sp, n) in specs.iter().zip(&nums) {
        let v: i64 = n.parse().ok()?;
        let one = sprintf(sp.as_bytes(), &[FmtArg::Int(v)], |_| Vec::new());
        if !sp.ends_with('d') || one.trim_ascii() != n.as_bytes() {
            return None;
        }
        args.push(FmtArg::Int(v));
    }
    String::from_utf8(sprintf(tr.as_bytes(), &args, |_| Vec::new())).ok()
}

// Play-data row labels (16 §2.4): vanilla prints `label` as the `%s` of `Builtin::StatsRow`; thcrap writes the whole
// row. A translation ending in the row's own tail gives the label.
fn row_label(b: Builtin, tr: &str) -> Option<String> {
    if !matches!(
        b,
        Builtin::StatsClear | Builtin::StatsContinue | Builtin::StatsPractice
    ) {
        return None;
    }
    let tail = Builtin::StatsRow.text().strip_prefix("%s")?;
    let label = tr.strip_suffix(tail)?;
    printf_specs(label)
        .ok()?
        .is_empty()
        .then(|| label.replace("%%", "%"))
}

fn import_builtins(lang: &Lang, report: &mut Report) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for b in Builtin::all() {
        let Some(id) = builtin_id(b, &lang.stringdefs) else {
            report.add(
                "builtin-missing",
                format!("{} ({:?})", b.key(), b.def().thcrap),
            );
            continue;
        };
        let Some(raw) = lang.stringdefs[id].as_str() else {
            continue;
        };
        let literal = printf_specs(b.text()).is_ok_and(|v| v.is_empty());
        // A literal without conversions is shown as is (`Builtin::txt`), a format goes through `sprintf`.
        let s = if literal {
            plain(raw, &mut 0)
        } else {
            flatten(raw, &mut 0)
        };
        let s = match printf_compatible(b.text(), &s) {
            Ok(()) => Ok(s),
            Err(e) if literal => {
                let f = flatten(raw, &mut 0);
                fill_numbers(b.text(), &f)
                    .or_else(|| row_label(b, &f))
                    .ok_or(e)
            }
            Err(e) => Err(e),
        };
        match s {
            Ok(s) => {
                out.insert(format!("builtin/{}", b.key()), s);
            }
            Err(e) => report.add("printf", format!("{} ({id}): {e}; kept vanilla", b.key())),
        }
    }
    out
}

// Translated share of the vanilla slots per key kind (`msg`, `end`, `spell`, `music`, `musiccmt`, `builtin`): (kind →
// (translated, total)). A slot without a translation keeps the original text at run time (`TextDb` miss).
pub fn coverage(keys: impl Fn(&str) -> bool, v: &Vanilla) -> BTreeMap<String, (usize, usize)> {
    let mut all: Vec<String> = v.keys().into_keys().collect();
    all.extend(spell_keys());
    let mut per: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for k in all {
        let e = per
            .entry(k.split('/').next().unwrap_or("").to_string())
            .or_default();
        e.1 += 1;
        if keys(&k) {
            e.0 += 1;
        }
    }
    per
}

// `kind have/total pct` lines plus the overall percentage.
pub fn coverage_text(per: &BTreeMap<String, (usize, usize)>) -> String {
    let pct = |a: usize, b: usize| {
        if b == 0 {
            0.0
        } else {
            100.0 * a as f64 / b as f64
        }
    };
    let (mut a, mut b) = (0, 0);
    let mut s = String::new();
    for (k, (have, n)) in per {
        a += have;
        b += n;
        s.push_str(&format!(
            "  {k:<9} {have:>5} / {n:<5} {:>5.1}%\n",
            pct(*have, *n)
        ));
    }
    s.push_str(&format!(
        "  {:<9} {a:>5} / {b:<5} {:>5.1}%\n",
        "all",
        pct(a, b)
    ));
    s
}

// Where a key is drawn (`TextCmd` geometry, 10 §6 caller table): site, placement and the sprite's right edge
// (`TextCmd::edge`): text.anm dialogue / spell sprites are 320 wide, the ending, list, music and menu strips 512.
pub fn site_of(key: &str) -> Option<(TextSite, Align, f32)> {
    let mut p = key.split('/');
    let kind = p.next()?;
    let rest = p.next_back().unwrap_or("");
    Some(match kind {
        "msg" => (TextSite::Dialogue, Align::Left, 320.0),
        "spell" if rest == "name" => (TextSite::Spell, Align::Right(320.0), 320.0),
        "spell" => (TextSite::MenuList, Align::Left, 512.0),
        "end" => (TextSite::Ending, Align::Left, 512.0),
        "music" | "musiccmt" => (TextSite::MusicRoom, Align::Left, 512.0),
        "builtin" => {
            let b = key.strip_prefix("builtin/")?;
            if b.starts_with("menu.") {
                (TextSite::MenuDesc, Align::Centre(256.0), 512.0)
            } else if b == "spell.resurrection" {
                (TextSite::Spell, Align::Right(320.0), 320.0)
            } else if b.starts_with("bomb.") {
                (TextSite::Spell, Align::Left, 320.0)
            } else if b.starts_with("music.") || b == "unknown8" {
                (TextSite::MusicRoom, Align::Left, 512.0)
            } else {
                (TextSite::MenuList, Align::Left, 512.0)
            }
        }
        _ => return None,
    })
}

// Every key form a pack may carry (TextRef::key), checked structurally: known file / card / builtin names.
pub fn key_known(key: &str) -> bool {
    let parts: Vec<&str> = key.split('/').collect();
    let num = |s: &str| s.parse::<u32>().is_ok();
    match parts.as_slice() {
        ["msg", f, e, n] => {
            f.strip_suffix(".dat")
                .is_some_and(|f| MSG_FILES.contains(&f))
                && num(e)
                && num(n)
        }
        ["spell", c, field] => {
            c.parse::<u16>().is_ok_and(|c| c < CARDS)
                && matches!(*field, "name" | "owner" | "comment0" | "comment1")
        }
        ["end", f, l] => END_FILES.contains(f) && num(l),
        ["music", t, "title"] => num(t),
        ["musiccmt", t, l] => num(t) && num(l),
        ["builtin", b] => Builtin::all().iter().any(|x| x.key() == *b),
        _ => false,
    }
}

// Spell keys of all 222 cards (their vanilla text is in ECL, not checked here).
pub fn spell_keys() -> Vec<String> {
    (0..CARDS)
        .flat_map(|c| {
            [
                SpellField::Name,
                SpellField::Owner,
                SpellField::Comment(0),
                SpellField::Comment(1),
            ]
            .map(|f| spell_key(c, f))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // A msg file with one entry from (time, op, args) triples; strings are XOR 0x77 NUL-terminated (10 §2).
    fn msg(ins: &[(u16, u8, Vec<u8>)]) -> MsgFile {
        let mut d = vec![1, 0, 0, 0, 8, 0, 0, 0];
        for (t, op, a) in ins {
            d.extend(t.to_le_bytes());
            d.push(*op);
            d.push(a.len() as u8);
            d.extend(a);
        }
        d.extend([0, 0, 0, 0]);
        th08_formats::msg::parse(d).unwrap()
    }

    fn s(t: &str) -> Vec<u8> {
        t.bytes().chain([0]).map(|b| b ^ 0x77).collect()
    }

    fn op3(line: i16, t: &str) -> Vec<u8> {
        let mut a = 0i16.to_le_bytes().to_vec();
        a.extend(line.to_le_bytes());
        a.extend(s(t));
        a
    }

    #[test]
    fn msg_box_keys() {
        let f = msg(&[
            (60, 16, s("a")),
            (60, 16, s("b")),
            (60, 4, vec![0; 4]),
            (60, 16, s("c")),
            (61, 16, s("d")),
            (63, 3, op3(0, "e")),
            (63, 8, vec![]),
            (124, 3, op3(1, "f")),
            (124, 19, s("g")),
        ]);
        let (b, h1) = msg_boxes(&f, f.entries[0]);
        let got: Vec<(&str, &[u16])> = b
            .iter()
            .map(|b| (b.key.as_str(), b.slots.as_slice()))
            .collect();
        assert_eq!(
            got,
            [
                ("60_0", &[0, 1][..]),
                ("60_1", &[2][..]),
                ("61_0", &[3][..]),
                ("63_0", &[4, 5][..]),
                ("124_0", &[6][..]),
            ]
        );
        assert_eq!(h1, ["63_h1_0"]);
        assert_eq!(text_ops(&f, f.entries[0]).len(), 7);
    }

    #[test]
    fn end_block_lines() {
        let d = b"@s\0\nab\ncd\n\n@w\nef\r\n@z\n";
        assert_eq!(end_blocks(d), [(1, vec![1, 2]), (5, vec![5])]);
    }

    #[test]
    fn markup_flattening() {
        let mut lab = 0;
        assert_eq!(flatten("<l$Yuka:$12345678> \"Hi,", &mut lab), "Yuka: \"Hi,");
        assert_eq!(
            flatten("<l$$12345678\"> there.\"", &mut lab),
            "      there.\""
        );
        assert_eq!(
            flatten("<tl$Play Count>  Easy\tNorm", &mut 0),
            "Play Count Easy Norm"
        );
        assert_eq!(flatten("a < b > c", &mut 0), "a < b > c");
        assert_eq!(plain("40%% done", &mut 0), "40% done");
    }

    #[test]
    fn printf_check() {
        assert!(printf_compatible("%3d/%3d（x）", "%3d/%3d (y)").is_ok());
        assert!(printf_compatible("使用者  %s　　%s %s", "User %s, %s %s").is_ok());
        assert!(printf_compatible("クリア回数", "Clear %6d").is_err());
        assert!(printf_compatible("%s %6d", "%6d %s").is_err());
        assert!(printf_compatible("%d", "%x").is_err());
        assert!(printf_compatible("100%%", "100%% done").is_ok());
    }

    #[test]
    fn builtin_mapping() {
        let mut defs = serde_json::Map::new();
        defs.insert("th07 Menu Start".into(), "Begin".into());
        defs.insert(
            "th08 Spell Result Character Select".into(),
            "%3d of %3d".into(),
        );
        defs.insert("th08 Stats Clear Count".into(), "Clears %6d".into());
        defs.insert(
            "th08 Stats Continue".into(),
            "Continues  %6d %6d %6d %6d %6d %6d".into(),
        );
        defs.insert(
            "th08 Spell Condition Line 1 212".into(),
            "Needs No.%.3d,%.3d".into(),
        );
        defs.insert(
            "th08 Spell Condition Line 1 210".into(),
            "Needs No.%3d".into(),
        );
        defs.insert(
            "th08 Spell Condition Line 2 206 207 209".into(),
            "Any difficulty.".into(),
        );
        let lang = Lang {
            dir: PathBuf::new(),
            layers: Vec::new(),
            stringdefs: defs,
            themes: serde_json::Map::new(),
            spells: serde_json::Map::new(),
            comments: serde_json::Map::new(),
            musiccmt: serde_json::Map::new(),
            msg: BTreeMap::new(),
            end: BTreeMap::new(),
        };
        let mut r = Report::default();
        let t = import_builtins(&lang, &mut r);
        assert_eq!(t["builtin/menu.top.0"], "Begin");
        assert_eq!(t["builtin/result.cards"], "%3d of %3d");
        // Card id 206 = card number 207 (1-based ids).
        assert_eq!(t["builtin/spell.hint.206.1"], "Any difficulty.");
        assert!(!t.contains_key("builtin/stats.clear"));
        assert_eq!(t["builtin/stats.continue"], "Continues ");
        // Vanilla `No.146,196` against `%.3d`: filled in; `%3d` of 138 prints the same digits.
        assert_eq!(t["builtin/spell.hint.211.0"], "Needs No.146,196");
        assert_eq!(t["builtin/spell.hint.209.0"], "Needs No.138");
        assert_eq!(r.count("printf"), 1);
    }

    // A variant language over its base: its own keys win, nested jdiff objects merge, base-only files stay.
    #[test]
    fn layered_language() {
        let root = std::env::temp_dir().join(format!("th08-layers-{}", std::process::id()));
        let (base, var) = (root.join("lang_en"), root.join("lang_en-gb"));
        for (d, defs, msg) in [
            (
                &base,
                r#"{"th07 Menu Start": "Start", "th07 Menu Replay": "Replay"}"#,
                r#"{"0": {"60_0": {"lines": ["color"]}, "61_0": {"lines": ["base"]}}}"#,
            ),
            (
                &var,
                r#"{"th07 Menu Start": "Begin"}"#,
                r#"{"0": {"60_0": {"lines": ["colour"]}}}"#,
            ),
        ] {
            std::fs::create_dir_all(d.join("th08")).unwrap();
            std::fs::write(d.join("stringdefs.js"), defs).unwrap();
            std::fs::write(d.join("th08/msg1a.dat.jdiff"), msg).unwrap();
        }
        std::fs::write(base.join("th08/spells.js"), r#"{"0": "Sign"}"#).unwrap();
        let l = Lang::read_layers(&[base.clone(), var.clone()]).unwrap();
        assert_eq!(l.stringdefs["th07 Menu Start"], "Begin");
        assert_eq!(l.stringdefs["th07 Menu Replay"], "Replay");
        assert_eq!(l.msg["msg1a"]["0"]["60_0"]["lines"][0], "colour");
        assert_eq!(l.msg["msg1a"]["0"]["61_0"]["lines"][0], "base");
        assert_eq!(l.spells["0"], "Sign");
        assert_eq!(l.dir, var);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn line_fitting() {
        let mut t = BTreeMap::new();
        let mut r = Report::default();
        let slots = ["a".to_string(), "b".to_string()];
        fit_lines(
            &mut t,
            &slots,
            &["1".into(), "2".into(), "\u{3000}3".into()],
            "x",
            &mut r,
        );
        assert_eq!((t["a"].as_str(), t["b"].as_str()), ("1", "2 3"));
        fit_lines(&mut t, &slots, &["1".into()], "y", &mut r);
        assert_eq!(t["b"], " ");
        assert_eq!((r.count("joined"), r.count("padded")), (1, 1));
    }

    #[test]
    fn keys_and_sites() {
        assert!(key_known("msg/msg1a.dat/0/3"));
        assert!(key_known("spell/221/comment1"));
        assert!(!key_known("spell/222/name"));
        assert!(key_known("builtin/menu.top.0"));
        assert!(!key_known("builtin/nope"));
        assert!(!key_known("end/end09a.end/3"));
        assert_eq!(site_of("spell/3/name").map(|s| s.0), Some(TextSite::Spell));
        assert_eq!(
            site_of("builtin/menu.key.2").map(|s| s.0),
            Some(TextSite::MenuDesc)
        );
        assert_eq!(
            strip_version("title/title02.v1.00d.png"),
            "title/title02.png"
        );
    }
}
