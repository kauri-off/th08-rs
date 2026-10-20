//! `.xz` file reading and writing for draw logs and session traces (port tooling, native only; liblzma).

use liblzma::read::XzDecoder;
use liblzma::stream::{Check, MtStreamBuilder};
use liblzma::write::XzEncoder;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

// Decodes every concatenated stream in the file, like `xz -dc`.
pub fn open(path: &Path) -> std::io::Result<XzDecoder<BufReader<File>>> {
    Ok(XzDecoder::new_multi_decoder(BufReader::new(File::open(
        path,
    )?)))
}

pub fn read_to_string(path: &Path) -> std::io::Result<String> {
    let mut s = String::new();
    std::io::Read::read_to_string(&mut open(path)?, &mut s)?;
    Ok(s)
}

// Preset 1 on all cores (the former `xz -1 -T0`); the stream is finished when the writer is dropped or `finish`ed.
pub type Writer = BufWriter<XzEncoder<File>>;

pub fn create(path: &Path) -> std::io::Result<Writer> {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get() as u32);
    let stream = MtStreamBuilder::new()
        .preset(1)
        .threads(threads)
        .check(Check::Crc64)
        .encoder()?;
    Ok(BufWriter::with_capacity(
        1 << 20,
        XzEncoder::new_stream(File::create(path)?, stream),
    ))
}

pub fn finish(w: Writer) -> std::io::Result<()> {
    w.into_inner()
        .map_err(|e| e.into_error())?
        .finish()?
        .flush()
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    #[test]
    fn round_trip_concatenated_streams() {
        let dir = crate::settings::temp_dir().join(format!("th08_xz_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut both = Vec::new();
        for (i, text) in ["first stream\n", "second stream\n"].iter().enumerate() {
            let p = dir.join(format!("{i}.xz"));
            let mut w = super::create(&p).unwrap();
            w.write_all(text.as_bytes()).unwrap();
            super::finish(w).unwrap();
            both.extend(std::fs::read(&p).unwrap());
        }
        let p = dir.join("both.xz");
        std::fs::write(&p, both).unwrap();
        let s = super::read_to_string(&p).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(s, "first stream\nsecond stream\n");
    }
}
