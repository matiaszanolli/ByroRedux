//! Throwaway (#3809 spike, 2026-08-31): extract BhkSystemBinary raw blobs
//! from real FO4 `_physics.nif` precombine samples and dump structural
//! bytes for corpus-derived Havok tagfile/packfile analysis. No leaked
//! Havok SDK source consulted — pure byte inspection against real data.
//!
//! Usage: cargo run -p byroredux-nif --example _tmp_a0831_havok_blob -- <ba2_path> [count]

use byroredux_bsa::Ba2Archive;
use byroredux_nif::blocks::collision::{parse_havok_packfile, BhkSystemBinary};
use byroredux_nif::parse_nif;

fn hex(data: &[u8], n: usize) -> String {
    data.iter()
        .take(n)
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Scan for printable-ASCII runs of at least `min_len` bytes, reporting
/// (offset, string). Cheap corpus-analysis helper — Havok packfiles
/// embed named section tags and class names as plain ASCII.
fn find_strings(data: &[u8], min_len: usize) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, &b) in data.iter().enumerate() {
        let printable = (0x20..0x7f).contains(&b);
        if printable {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take() {
            if i - s >= min_len {
                out.push((s, String::from_utf8_lossy(&data[s..i]).into_owned()));
            }
        }
    }
    if let Some(s) = start {
        if data.len() - s >= min_len {
            out.push((s, String::from_utf8_lossy(&data[s..]).into_owned()));
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let archive = Ba2Archive::open(&args[1]).expect("open ba2");
    let count: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(40);
    let dump_strings = args.iter().any(|a| a == "--strings");
    let dump_blob_to: Option<&String> = args
        .iter()
        .position(|a| a == "--dump-blob")
        .map(|i| &args[i + 1]);

    let mut files: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|f| f.to_ascii_lowercase().ends_with("_physics.nif"))
        .map(|f| f.to_string())
        .collect();
    files.sort();
    // Spread the sample across the corpus instead of just the first N
    // (alphabetical order clusters by cell-formid prefix, not size).
    let stride = (files.len() / count.max(1)).max(1);
    let sample: Vec<&String> = files.iter().step_by(stride).take(count).collect();

    println!(
        "{} total _physics.nif files; sampling {} (stride {})",
        files.len(),
        sample.len(),
        stride
    );

    let mut blob_count = 0usize;
    for f in sample {
        let data = match archive.extract(f) {
            Ok(d) => d,
            Err(e) => {
                println!("{}: extract failed: {}", f, e);
                continue;
            }
        };
        let scene = match parse_nif(&data) {
            Ok(s) => s,
            Err(e) => {
                println!("{}: parse failed: {}", f, e);
                continue;
            }
        };
        println!(
            "--- {} (nif {} bytes, {} blocks, truncated={}) ---",
            f,
            data.len(),
            scene.blocks.len(),
            scene.truncated
        );
        for block in &scene.blocks {
            if let Some(bin) = block.as_any().downcast_ref::<BhkSystemBinary>() {
                blob_count += 1;
                let d = &bin.data;
                println!(
                    "  {} data_size={} first64={} last32={}",
                    bin.type_name,
                    d.len(),
                    hex(d, 64),
                    if d.len() >= 32 {
                        hex(&d[d.len() - 32..], 32)
                    } else {
                        hex(d, d.len())
                    }
                );
                if args.iter().any(|a| a == "--sections") {
                    println!("    bytes[0x30..0x110]:");
                    for chunk_start in (0x30..0x110.min(d.len())).step_by(16) {
                        let end = (chunk_start + 16).min(d.len());
                        println!(
                            "      {:#05x}: {}",
                            chunk_start,
                            hex(&d[chunk_start..end], 16)
                        );
                    }
                }
                if dump_strings {
                    for (off, s) in find_strings(d, 4) {
                        println!("    str@{:#x} ({}): {:?}", off, off, s);
                    }
                }
                if let Some(path) = dump_blob_to {
                    std::fs::write(path, d).expect("write blob");
                    println!("    wrote blob to {}", path);
                }
                if args.iter().any(|a| a == "--pf") {
                    match parse_havok_packfile(d) {
                        Ok(pf) => {
                            println!(
                                "    PF: version={} contents={:?} sections={:?}",
                                pf.header.file_version,
                                pf.header.contents_version,
                                pf.sections
                                    .iter()
                                    .map(|s| (&s.name, s.absolute_data_start, s.absolute_end()))
                                    .collect::<Vec<_>>()
                            );
                            println!("    classes={:?}", pf.class_names);
                        }
                        Err(e) => println!("    PF parse failed: {}", e),
                    }
                }
            }
        }
    }
    println!("total BhkSystemBinary blobs found: {}", blob_count);
}
