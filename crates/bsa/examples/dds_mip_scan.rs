// Triage tool for #789 (suspect 2: DDS mip count).
//
// Walks a BSA, finds .dds entries matching a path substring, parses the
// 32-byte DDS header at offset 0..32, and reports `mip_count` vs the
// theoretical maximum `floor(log2(max(w, h))) + 1`. Truncated chains
// are flagged.
//
// Floor mode (M39 Phase 0, docs/prd/texture-streaming.md, Q3): with
// `--floor-edge N` first, it takes any number of BSAs, prints no per-file
// lines, and summarises the textures against an N-pixel floor — how many
// are single-mip or cubemaps (**whole**), how many have a whole chain that
// fits inside the floor (**whole**), how many stream, and what share of
// the streamable bytes the floor levels hold.
//
// Usage:
//   cargo run --release -p byroredux-bsa --example dds_mip_scan -- \
//       "Fallout - Textures.bsa" "clutter\\chem"
//   cargo run --release -p byroredux-bsa --example dds_mip_scan -- \
//       --floor-edge 256 "" "Fallout - Textures.bsa" "Fallout - Textures2.bsa"
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
    if args.get(1).map(String::as_str) == Some("--floor-edge") {
        let edge: u32 = args.get(2).and_then(|e| e.parse().ok()).unwrap_or_else(|| {
            eprintln!("usage: dds_mip_scan --floor-edge <px> <substring> <bsa>...");
            std::process::exit(2);
        });
        let needle = args
            .get(3)
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        floor_summary(edge, &needle, &args[4..]);
        return;
    }
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
        let Ok(data) = archive.extract(f) else {
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

/// Is the payload block-compressed? FourCC DXT1–5 / ATI1–2 / BC4–5, or a
/// DX10 header naming a BC format (DXGI 70–84, 94–99).
fn is_block_compressed(data: &[u8]) -> bool {
    match &data[84..88] {
        b"DXT1" | b"DXT2" | b"DXT3" | b"DXT4" | b"DXT5" | b"ATI1" | b"ATI2" | b"BC4U" | b"BC4S"
        | b"BC5U" | b"BC5S" => true,
        b"DX10" if data.len() >= 132 => {
            let dxgi = read_u32(data, 128);
            (70..=84).contains(&dxgi) || (94..=99).contains(&dxgi)
        }
        _ => false,
    }
}

/// Storage units of one level: 4×4 blocks for BC formats, pixels otherwise.
/// Only ratios between levels are used, so the per-unit byte size cancels.
fn level_units(width: u32, height: u32, level: u32, bc: bool) -> u64 {
    let w = (width >> level).max(1);
    let h = (height >> level).max(1);
    if bc {
        u64::from(w.div_ceil(4)) * u64::from(h.div_ceil(4))
    } else {
        u64::from(w) * u64::from(h)
    }
}

#[derive(Default)]
struct FloorTally {
    files: usize,
    unreadable: usize,
    cubemaps: (usize, u64),
    single_mip: (usize, u64),
    fits_floor: (usize, u64),
    streamable: (usize, u64),
    streamable_floor_bytes: u64,
    truncated: usize,
}

fn floor_summary(edge: u32, needle: &str, bsas: &[String]) {
    let mut t = FloorTally::default();
    for bsa in bsas {
        let archive = match BsaArchive::open(bsa) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("! open failed: {bsa}: {e}");
                continue;
            }
        };
        let before = t.files;
        for f in archive.list_files() {
            let lc = f.to_ascii_lowercase();
            if !lc.ends_with(".dds") || !lc.contains(needle) {
                continue;
            }
            t.files += 1;
            let Ok(data) = archive.extract(f) else {
                t.unreadable += 1;
                continue;
            };
            if data.len() < 128 || &data[..4] != b"DDS " {
                t.unreadable += 1;
                continue;
            }
            let height = read_u32(&data, 12);
            let width = read_u32(&data, 16);
            let mips = read_u32(&data, 28).max(1);
            let bytes = data.len() as u64;
            if mips < declared_mip_max(width, height) {
                t.truncated += 1;
            }
            let slot = if read_u32(&data, 112) & 0x200 != 0 {
                &mut t.cubemaps
            } else if mips <= 1 {
                &mut t.single_mip
            } else if width.max(height) <= edge {
                &mut t.fits_floor
            } else {
                let bc = is_block_compressed(&data);
                let units: Vec<u64> = (0..mips)
                    .map(|l| level_units(width, height, l, bc))
                    .collect();
                let total: u64 = units.iter().sum();
                let floor: u64 = (0..mips)
                    .filter(|l| (width >> l).max(height >> l).max(1) <= edge)
                    .map(|l| units[l as usize])
                    .sum();
                let header = if &data[84..88] == b"DX10" { 148 } else { 128 };
                let payload = bytes.saturating_sub(header);
                t.streamable_floor_bytes += payload * floor / total.max(1);
                &mut t.streamable
            };
            slot.0 += 1;
            slot.1 += bytes;
        }
        eprintln!("+ {} DDS: {bsa}", t.files - before);
    }
    let mib = |b: u64| b as f64 / (1024.0 * 1024.0);
    println!("## BSA DDS floor census (floor edge {edge} px)\n");
    println!("| Measure | Files | Bytes |");
    println!("|---|---|---|");
    println!("| Archives | {} | |", bsas.len());
    println!("| DDS files | {} | |", t.files);
    println!("| Unreadable / not DDS | {} | |", t.unreadable);
    println!("| Truncated mip chains | {} | |", t.truncated);
    println!(
        "| Cubemaps (**whole**) | {} | {:.1} MiB |",
        t.cubemaps.0,
        mib(t.cubemaps.1)
    );
    println!(
        "| Single-mip 2D (**whole**) | {} | {:.1} MiB |",
        t.single_mip.0,
        mib(t.single_mip.1)
    );
    println!(
        "| Chain fits inside the floor (**whole**) | {} | {:.1} MiB |",
        t.fits_floor.0,
        mib(t.fits_floor.1)
    );
    println!(
        "| Streamable | {} | {:.1} MiB |",
        t.streamable.0,
        mib(t.streamable.1)
    );
    println!(
        "| … of which floor levels | | {:.1} MiB ({:.1} %) |",
        mib(t.streamable_floor_bytes),
        100.0 * t.streamable_floor_bytes as f64 / t.streamable.1.max(1) as f64
    );
}
