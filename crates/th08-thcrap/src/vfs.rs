//! File access of the importer: the real file system natively, an in-memory tree in the browser (no file system there).

use std::io;
use std::path::{Path, PathBuf};

#[cfg(not(target_arch = "wasm32"))]
pub use std::fs::{create_dir_all, read, read_to_string, remove_dir_all, rename, write};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
    Other,
}

// What `p` is, without following a symlink (a symlink is `Other`); None = nothing there.
#[cfg(not(target_arch = "wasm32"))]
pub fn kind(p: &Path) -> Option<Kind> {
    let t = std::fs::symlink_metadata(p).ok()?.file_type();
    Some(if t.is_dir() {
        Kind::Dir
    } else if t.is_file() {
        Kind::File
    } else {
        Kind::Other
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn len(p: &Path) -> Option<u64> {
    std::fs::metadata(p).ok().map(|m| m.len())
}

// The entries of a folder, unsorted.
#[cfg(not(target_arch = "wasm32"))]
pub fn list(p: &Path) -> io::Result<Vec<PathBuf>> {
    std::fs::read_dir(p)?.map(|e| e.map(|e| e.path())).collect()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn modified(p: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

#[cfg(target_arch = "wasm32")]
mod mem {
    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;

    #[derive(Default)]
    pub struct Tree {
        pub files: BTreeMap<PathBuf, Vec<u8>>,
        pub dirs: BTreeSet<PathBuf>,
    }

    thread_local! {
        pub static TREE: RefCell<Tree> = RefCell::default();
    }
}

#[cfg(target_arch = "wasm32")]
fn not_found(p: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{}: not found", p.display()),
    )
}

#[cfg(target_arch = "wasm32")]
fn add_dirs(t: &mut mem::Tree, p: &Path) {
    for a in p.ancestors().filter(|a| !a.as_os_str().is_empty()) {
        if !t.dirs.insert(a.to_path_buf()) {
            break;
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn kind(p: &Path) -> Option<Kind> {
    mem::TREE.with_borrow(|t| {
        if t.files.contains_key(p) {
            Some(Kind::File)
        } else {
            t.dirs.contains(p).then_some(Kind::Dir)
        }
    })
}

#[cfg(target_arch = "wasm32")]
pub fn len(p: &Path) -> Option<u64> {
    mem::TREE.with_borrow(|t| t.files.get(p).map(|b| b.len() as u64))
}

#[cfg(target_arch = "wasm32")]
pub fn list(p: &Path) -> io::Result<Vec<PathBuf>> {
    mem::TREE.with_borrow(|t| {
        if !t.dirs.contains(p) {
            return Err(not_found(p));
        }
        let child = |q: &&PathBuf| q.parent() == Some(p);
        Ok(t.dirs
            .iter()
            .filter(child)
            .chain(t.files.keys().filter(child))
            .cloned()
            .collect())
    })
}

#[cfg(target_arch = "wasm32")]
pub fn modified(_: &Path) -> Option<std::time::SystemTime> {
    None
}

#[cfg(target_arch = "wasm32")]
pub fn read(p: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    let p = p.as_ref();
    mem::TREE.with_borrow(|t| t.files.get(p).cloned().ok_or_else(|| not_found(p)))
}

#[cfg(target_arch = "wasm32")]
pub fn read_to_string(p: impl AsRef<Path>) -> io::Result<String> {
    String::from_utf8(read(p)?).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(target_arch = "wasm32")]
pub fn write(p: impl AsRef<Path>, b: impl AsRef<[u8]>) -> io::Result<()> {
    let p = p.as_ref();
    mem::TREE.with_borrow_mut(|t| {
        if t.dirs.contains(p) {
            return Err(io::Error::other(format!("{}: is a folder", p.display())));
        }
        if let Some(d) = p.parent() {
            add_dirs(t, d);
        }
        t.files.insert(p.to_path_buf(), b.as_ref().to_vec());
        Ok(())
    })
}

#[cfg(target_arch = "wasm32")]
pub fn create_dir_all(p: impl AsRef<Path>) -> io::Result<()> {
    mem::TREE.with_borrow_mut(|t| add_dirs(t, p.as_ref()));
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub fn remove_dir_all(p: impl AsRef<Path>) -> io::Result<()> {
    let p = p.as_ref();
    mem::TREE.with_borrow_mut(|t| {
        if !t.dirs.contains(p) {
            return Err(not_found(p));
        }
        t.dirs.retain(|d| !d.starts_with(p));
        t.files.retain(|f, _| !f.starts_with(p));
        Ok(())
    })
}

// Moves a file or a folder with everything under it.
#[cfg(target_arch = "wasm32")]
pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let (from, to) = (from.as_ref(), to.as_ref());
    mem::TREE.with_borrow_mut(|t| {
        let moved = |q: &Path| to.join(q.strip_prefix(from).unwrap_or(q));
        let dirs: Vec<PathBuf> = t
            .dirs
            .iter()
            .filter(|d| d.starts_with(from))
            .cloned()
            .collect();
        let files: Vec<PathBuf> = t
            .files
            .keys()
            .filter(|f| f.starts_with(from))
            .cloned()
            .collect();
        if dirs.is_empty() && files.is_empty() {
            return Err(not_found(from));
        }
        for d in dirs {
            t.dirs.remove(&d);
            add_dirs(t, &moved(&d));
        }
        for f in files {
            let b = t.files.remove(&f).unwrap_or_default();
            let m = moved(&f);
            if let Some(d) = m.parent() {
                add_dirs(t, d);
            }
            t.files.insert(m, b);
        }
        Ok(())
    })
}

pub fn exists(p: &Path) -> bool {
    kind(p).is_some()
}

pub fn is_file(p: &Path) -> bool {
    kind(p) == Some(Kind::File)
}

pub fn is_dir(p: &Path) -> bool {
    kind(p) == Some(Kind::Dir)
}

// Every file under `dir` as (path relative to it with '/', bytes), in name order.
pub fn files_under(dir: &Path) -> io::Result<Vec<(String, Vec<u8>)>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for p in list(&d)? {
            match kind(&p) {
                Some(Kind::Dir) => stack.push(p),
                Some(Kind::File) => {
                    let rel = p
                        .strip_prefix(dir)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.push((rel, read(&p)?));
                }
                _ => {}
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_under_walks_folders() {
        let d = std::env::temp_dir().join(format!("th08-vfs-{}", std::process::id()));
        write_tree(&d);
        let f = files_under(&d).unwrap();
        let names: Vec<&str> = f.iter().map(|x| x.0.as_str()).collect();
        assert_eq!(names, ["a.txt", "sub/b.txt"]);
        assert!(is_dir(&d) && is_file(&d.join("a.txt")) && !exists(&d.join("x")));
        assert_eq!(len(&d.join("sub/b.txt")), Some(1));
        remove_dir_all(&d).unwrap();
    }

    fn write_tree(d: &Path) {
        create_dir_all(d.join("sub")).unwrap();
        write(d.join("a.txt"), b"ab").unwrap();
        write(d.join("sub/b.txt"), b"c").unwrap();
    }
}
