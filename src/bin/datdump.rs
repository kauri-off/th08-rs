//! Extracts every th08.dat entry: `datdump <th08.dat> <outdir>` (decoded with archive.rs, 01 §5–§6).

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let ar = th08::archive::Archive::from_bytes(std::fs::read(&args[1])?)?;
    let out = std::path::Path::new(&args[2]);
    std::fs::create_dir_all(out)?;
    for n in ar.names() {
        std::fs::write(out.join(n), ar.read(n)?)?;
    }
    println!("{} files", ar.names().len());
    Ok(())
}
