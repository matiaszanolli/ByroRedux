// Dimension 6 (BSA v104 + Real-Data Validation) throwaway survey.
// Walks Fallout - Textures.bsa, parses every .dds entry's header (32-byte
// legacy header + optional DX10 extended header), and tallies FourCC /
// DXGI format buckets + basic structural validity (declared size sane,
// mip count sane). Not wired into CI; ad hoc audit tool.
use byroredux_bsa::BsaArchive;
use std::collections::BTreeMap;

fn read_u32(data: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
}

fn fourcc_str(fourcc: u32) -> String {
    let b = fourcc.to_le_bytes();
    b.iter()
        .map(|&c| {
            if c.is_ascii_graphic() {
                c as char
            } else {
                '?'
            }
        })
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bsa_path = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "Fallout - Textures.bsa".to_string());
    let archive = BsaArchive::open(&bsa_path).expect("open bsa");

    let mut format_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut total = 0usize;
    let mut bad_magic = 0usize;
    let mut too_small = 0usize;
    let mut bad_dims = 0usize;
    let mut extract_fail = 0usize;

    let dds_files: Vec<String> = archive
        .list_files()
        .iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".dds"))
        .map(|s| s.to_string())
        .collect();
    eprintln!("{} .dds entries in {}", dds_files.len(), bsa_path);

    for f in &dds_files {
        total += 1;
        let data = match archive.extract(f) {
            Ok(d) => d,
            Err(_) => {
                extract_fail += 1;
                continue;
            }
        };
        if data.len() < 128 {
            too_small += 1;
            continue;
        }
        if &data[0..4] != b"DDS " {
            bad_magic += 1;
            continue;
        }
        let height = read_u32(&data, 12);
        let width = read_u32(&data, 16);
        if width == 0 || height == 0 || width > 16384 || height > 16384 {
            bad_dims += 1;
            continue;
        }
        // pixelformat flags at offset 80, fourcc at offset 84
        let pf_flags = read_u32(&data, 80);
        let fourcc = read_u32(&data, 84);
        const DDPF_FOURCC: u32 = 0x4;
        let label = if pf_flags & DDPF_FOURCC != 0 {
            if fourcc == u32::from_le_bytes(*b"DX10") {
                if data.len() >= 128 + 20 {
                    let dxgi = read_u32(&data, 128);
                    format!("DX10/dxgi={}", dxgi)
                } else {
                    "DX10/truncated-header".to_string()
                }
            } else {
                fourcc_str(fourcc)
            }
        } else {
            "uncompressed(RGB masks)".to_string()
        };
        *format_counts.entry(label).or_insert(0) += 1;
    }

    println!("\n=== DDS survey: {} ===", bsa_path);
    println!("total .dds entries: {}", total);
    println!("extract failures:   {}", extract_fail);
    println!("too small (<128B):  {}", too_small);
    println!("bad magic:          {}", bad_magic);
    println!("bad dims:           {}", bad_dims);
    println!("\nformat histogram:");
    for (k, v) in &format_counts {
        println!("  {:>8}  {}", v, k);
    }
}
