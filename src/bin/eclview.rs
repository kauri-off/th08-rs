// Disassembles an .ecl file: `eclview <th08.dat> <x.ecl> [sub]`.
use th08::ecl;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let ar = th08::archive::Archive::from_bytes(std::fs::read(&a[1])?)?;
    let e = ecl::parse(ar.read(&a[2])?)?;
    let only: Option<usize> = a.get(3).and_then(|s| s.parse().ok());
    for (t, &start) in e.timelines.iter().enumerate().filter(|_| only.is_none()) {
        println!("timeline {t}");
        let mut off = start;
        while let Some(i) = e.tl_ins(off).filter(|i| i.time >= 0) {
            let args: Vec<String> = (0..(i.size - 8) / 4).map(|k| fmt(e.tl_arg(&i, k), false)).collect();
            println!("  {off:#07x} t{:<6} op{:<3} r{:02x} {}", i.time, i.op, i.rank, args.join(", "));
            off += i.size;
        }
    }
    for (s, &start) in e.subs.iter().enumerate().filter(|(s, _)| only.is_none_or(|o| o == *s)) {
        println!("sub {s}");
        let mut off = start;
        while let Some(i) = e.ins(off) {
            let sig = ecl::signature(i.op).unwrap_or("?").as_bytes();
            let n = (i.size - ecl::SUB_HEADER) / 4;
            let args: Vec<String> = (0..n.min(9))
                .map(|k| {
                    let v = if i.mask & (1 << k) != 0 { "$" } else { "" };
                    format!("{v}{}", fmt(e.arg(&i, k), matches!(sig.get(k), Some(b'f' | b'W'))))
                })
                .collect();
            println!("  {off:#07x} t{:<5} op{:<3} r{:02x} {}", i.time, i.op, i.rank, args.join(", "));
            if i.op == -1 {
                break;
            }
            off += i.size;
        }
    }
    Ok(())
}

fn fmt(v: u32, float: bool) -> String {
    if float { format!("{}", f32::from_bits(v)) } else { format!("{}", v as i32) }
}
