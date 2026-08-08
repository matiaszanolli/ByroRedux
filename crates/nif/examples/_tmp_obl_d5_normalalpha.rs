//! Throwaway (Oblivion audit dim5): `_n.dds` alpha-format census.
use byroredux_bsa::BsaArchive;
use std::collections::BTreeMap;

fn main() {
    let path = std::env::args().nth(1).expect("usage: <textures bsa>");
    let archive = BsaArchive::open(&path).expect("open");
    let files: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|n| n.to_ascii_lowercase().ends_with("_n.dds"))
        .map(|s| s.to_string())
        .collect();
    let mut fourcc: BTreeMap<String, u64> = Default::default();
    let mut n = 0u64;
    for f in &files {
        let Ok(b) = archive.extract(f) else { continue };
        if b.len() < 92 { continue }
        let flags = u32::from_le_bytes([b[80], b[81], b[82], b[83]]);
        let cc = String::from_utf8_lossy(&b[84..88]).to_string();
        let key = if flags & 0x4 != 0 { cc } else { format!("RGB(flags={flags:#x})") };
        *fourcc.entry(key).or_insert(0) += 1;
        n += 1;
    }
    println!("_n.dds files={n} (listed {})", files.len());
    println!("{fourcc:?}");
}
