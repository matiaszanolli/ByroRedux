//! Throwaway (#3810 spike, 2026-08-31): dump `.uvd` (FO4 previs/occlusion)
//! headers across many real samples for corpus-derived byte-layout
//! analysis. Bytes `0..0x14` and the `0xB0..0x100` debug string are
//! already cracked (see issue #3810); this focuses on `0x14..0xB0`.
//!
//! Usage: cargo run -p byroredux-bsa --example _tmp_a0831_uvd_header -- <ba2_path> [count]

use byroredux_bsa::{parse_uvd_header, Ba2Archive};

fn hex(data: &[u8], n: usize) -> String {
    data.iter()
        .take(n)
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

fn f32le(data: &[u8], off: usize) -> f32 {
    f32::from_le_bytes(data[off..off + 4].try_into().unwrap())
}
fn u32le(data: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(data[off..off + 4].try_into().unwrap())
}
fn i32le(data: &[u8], off: usize) -> i32 {
    i32::from_le_bytes(data[off..off + 4].try_into().unwrap())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let archive = Ba2Archive::open(&args[1]).expect("open ba2");
    let count: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(20);

    let mut files: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|f| f.to_ascii_lowercase().ends_with(".uvd"))
        .map(|f| f.to_string())
        .collect();
    files.sort();
    let stride = (files.len() / count.max(1)).max(1);
    let sample: Vec<&String> = files.iter().step_by(stride).take(count).collect();

    println!("{} total .uvd files; sampling {}", files.len(), sample.len());

    for f in sample {
        let data = match archive.extract(f) {
            Ok(d) => d,
            Err(e) => {
                println!("{}: extract failed: {}", f, e);
                continue;
            }
        };
        // cell form id from filename
        let stem = f.rsplit('\\').next().unwrap_or(f).trim_end_matches(".uvd");
        println!("--- {} formid={} size={} ---", f, stem, data.len());
        println!("  0x00..0x14: {}", hex(&data, 0x14));
        println!(
            "  magic={:#x} field4={:#x} selfsize={} f32@0x0c={}",
            u32le(&data, 0),
            u32le(&data, 4),
            u32le(&data, 8),
            f32le(&data, 0x0c)
        );
        // bytes 0x14..0xB0 as both hex and float/int interpretation
        for off in (0x14..0xb0.min(data.len())).step_by(16) {
            let end = (off + 16).min(data.len());
            let chunk = &data[off..end];
            print!("  {:#05x}: {}", off, hex(chunk, 16));
            if chunk.len() == 16 {
                print!("  | f32x4=[{:.3}, {:.3}, {:.3}, {:.3}]", f32le(&data,off), f32le(&data,off+4), f32le(&data,off+8), f32le(&data,off+12));
                print!(" i32x4=[{}, {}, {}, {}]", i32le(&data,off), i32le(&data,off+4), i32le(&data,off+8), i32le(&data,off+12));
            }
            println!();
        }
        // debug string region
        if data.len() >= 0x100 {
            let s = String::from_utf8_lossy(&data[0xb0..0x100]);
            println!("  debug_str={:?}", s.trim_end_matches('\0'));
        }
        match parse_uvd_header(&data) {
            Ok(h) => println!("  UvdHeader: {:?}", h),
            Err(e) => println!("  UvdHeader parse failed: {}", e),
        }
        // table starting at the universal-constant offset 0x150 (336)
        if args.iter().any(|a| a == "--table") && data.len() > 0x150 {
            println!("  bytes[0x150..0x1d0]:");
            for off in (0x150..0x1d0.min(data.len())).step_by(16) {
                let end = (off + 16).min(data.len());
                println!("    {:#05x}: {}", off, hex(&data[off..end], 16));
            }
        }
    }
}
