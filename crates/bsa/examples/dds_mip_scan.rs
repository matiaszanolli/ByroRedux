// Triage tool for #789 (suspect 2: DDS mip count).
//
// Walks a BSA, finds .dds entries matching a path substring, parses the
// 32-byte DDS header at offset 0..32, and reports `mip_count` vs the
// theoretical maximum `floor(log2(max(w, h))) + 1`. Truncated chains
// are flagged.
//
// Usage:
//   cargo run --release -p byroredux-bsa --example dds_mip_scan -- \
//       "Fallout - Textures.bsa" "clutter\\chem"
use byroredux_bsa::BsaArchive;

fn declared_mip_max(w: u32, h: u32) -> u32 {
    let m = w.max(h).max(1);
    32 - m.leading_zeros()
}

fn read_u32(data: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: dds_mip_scan <bsa> <substring>");
        std::process::exit(2);
    }
    let bsa = &args[1];
    let needle = args[2].to_ascii_lowercase();
    let archive = BsaArchive::open(bsa).expect("open bsa");

    let mut hits = 0usize;
    let mut truncated = 0usize;
    for f in archive.list_files() {
        let lc = f.to_ascii_lowercase();
        if !lc.ends_with(".dds") || !lc.contains(&needle) {
            continue;
        }
        let Ok(data) = archive.extract(&f) else {
            eprintln!("  ! extract failed: {}", f);
            continue;
        };
        if data.len() < 32 || &data[..4] != b"DDS " {
            eprintln!("  ! not DDS: {}", f);
            continue;
        }
        let height = read_u32(&data, 12);
        let width = read_u32(&data, 16);
        let mip_count = read_u32(&data, 28).max(1);
        let theoretical = declared_mip_max(width, height);
        let trunc = mip_count < theoretical;
        if trunc {
            truncated += 1;
        }
        hits += 1;
        println!(
            "{tag} {w:>5}x{h:<5} mips={m:>2}/{t:<2} {path}",
            tag = if trunc { "TRUNC" } else { "  ok " },
            w = width,
            h = height,
            m = mip_count,
            t = theoretical,
            path = f
        );
    }
    println!(
        "\n{} files matched, {} with truncated mip chains",
        hits, truncated
    );
}
