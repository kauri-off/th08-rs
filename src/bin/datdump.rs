#[path = "../archive.rs"]
mod archive;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let ar = archive::Archive::open(std::path::Path::new(&args[1]))?;
    let out = std::path::Path::new(&args[2]);
    std::fs::create_dir_all(out)?;
    for n in ar.names() {
        std::fs::write(out.join(n), ar.read(n)?)?;
    }
    println!("{} files", ar.names().len());
    Ok(())
}
