use byroredux_bsa::Ba2Archive;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = Ba2Archive::open(&a[1]).expect("open");
    let want = a.get(2).cloned();
    let mut short = 0usize;
    let mut total = 0usize;
    let mut shown = 0;
    for (name, packed, unpacked) in ar.iter_general_sizes() {
        if !name.to_ascii_lowercase().ends_with(".mesh") {
            continue;
        }
        if let Some(w) = &want {
            if name != w {
                continue;
            }
        }
        total += 1;
        let got = ar.extract(name).map(|v| v.len()).unwrap_or(0);
        if got as u32 != unpacked && unpacked != 0 {
            short += 1;
            if shown < 10 {
                println!(
                    "SHORT {} packed={} unpacked={} got={}",
                    name, packed, unpacked, got
                );
                shown += 1;
            }
        } else if want.is_some() {
            println!(
                "OK {} packed={} unpacked={} got={}",
                name, packed, unpacked, got
            );
        }
        if want.is_none() && total >= 3000 {
            break;
        }
    }
    println!("checked={} short={}", total, short);
}
