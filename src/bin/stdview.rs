// Prints an .std file: header, objects with quads, instances and the script.
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let ar = th08::archive::Archive::from_bytes(std::fs::read(&a[1])?)?;
    let s = th08::stdfile::parse(&ar.read(&a[2])?)?;
    println!("quads {} bgm {:?}", s.quad_count, s.bgm);
    for o in &s.objects {
        println!(
            "object {} layer {} pos {:?} size {:?}",
            o.id, o.layer, o.pos, o.size
        );
        for q in &o.quads {
            println!("  script {} vm {} {:?}", q.script, q.vm, q.kind);
        }
    }
    for i in &s.instances {
        println!("instance obj {} at {:?}", i.object, i.pos);
    }
    for (k, r) in s.script.iter().enumerate() {
        println!(
            "{k:4} t{:6} op {:2} i{:?} f[{:.3}, {:.3}, {:.3}] {:08x}",
            r.time,
            r.op,
            [r.i(0), r.i(1), r.i(2)],
            r.f(0),
            r.f(1),
            r.f(2),
            r.args[0]
        );
    }
    Ok(())
}
