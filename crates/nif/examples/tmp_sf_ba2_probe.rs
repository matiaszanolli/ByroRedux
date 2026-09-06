//! TEMPORARY audit scratch — Starfield BA2 v2/v3 + DX10 extract check (delete after use).
//!
//! For every archive in the Starfield Data dir: report BA2 version + (v3)
//! compression method from the raw header bytes, then extract a bounded
//! stride-sample of entries and verify they come back non-empty (and, for
//! `.dds`, that the reconstructed header carries the DDS magic).

use byroredux_bsa::Ba2Archive;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;

fn header_info(path: &std::path::Path) -> Option<(u32, [u8; 4], Option<u32>)> {
    let mut f = File::open(path).ok()?;
    let mut buf = [0u8; 40];
    f.read_exact(&mut buf).ok()?;
    if &buf[0..4] != b"BTDX" {
        return None;
    }
    let version = u32::from_le_bytes(buf[4..8].try_into().ok()?);
    let kind: [u8; 4] = buf[8..12].try_into().ok()?;
    // base header is 24 B; v2 adds 8; v3 adds 8 + 4 (compression_method).
    let method = if version == 3 {
        Some(u32::from_le_bytes(buf[32..36].try_into().ok()?))
    } else {
        None
    };
    Some((version, kind, method))
}

fn main() {
    let base = std::env::var("BYROREDUX_STARFIELD_DATA").unwrap_or_else(|_| {
        "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data".to_string()
    });
    let sample: usize = std::env::var("SF_SAMPLE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);

    let mut versions: BTreeMap<String, usize> = BTreeMap::new();
    let mut grand_ok = 0usize;
    let mut grand_fail = 0usize;
    let mut grand_dds_ok = 0usize;
    let mut grand_dds_bad = 0usize;
    let mut failures: Vec<String> = Vec::new();

    let mut entries: Vec<_> = std::fs::read_dir(&base)
        .expect("data dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .map(|x| x.eq_ignore_ascii_case("ba2"))
                .unwrap_or(false)
        })
        .collect();
    entries.sort();

    for path in &entries {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let Some((ver, kind, method)) = header_info(path) else {
            println!("[{name}] NOT A BTDX ARCHIVE");
            continue;
        };
        let kind_s = String::from_utf8_lossy(&kind).to_string();
        let key = format!(
            "v{ver} {kind_s} method={}",
            method.map(|m| m.to_string()).unwrap_or("-".into())
        );
        *versions.entry(key.clone()).or_insert(0) += 1;

        let archive = match Ba2Archive::open(path) {
            Ok(a) => a,
            Err(e) => {
                println!("[{name}] {key} OPEN FAILED: {e}");
                failures.push(format!("{name}: open: {e}"));
                continue;
            }
        };
        let files: Vec<String> = archive.list_files().into_iter().map(|s| s.into()).collect();
        if files.is_empty() {
            println!("[{name}] {key} entries=0");
            continue;
        }
        let stride = (files.len() / sample).max(1);
        let mut ok = 0usize;
        let mut fail = 0usize;
        let mut dds_ok = 0usize;
        let mut dds_bad = 0usize;
        for f in files.iter().step_by(stride) {
            match archive.extract(f) {
                Ok(d) => {
                    ok += 1;
                    if f.to_lowercase().ends_with(".dds") {
                        if d.len() >= 4 && &d[0..4] == b"DDS " {
                            dds_ok += 1;
                        } else {
                            dds_bad += 1;
                            if failures.len() < 30 {
                                failures.push(format!("{name}: {f}: bad DDS magic len={}", d.len()));
                            }
                        }
                    }
                }
                Err(e) => {
                    fail += 1;
                    if failures.len() < 30 {
                        failures.push(format!("{name}: {f}: {e}"));
                    }
                }
            }
        }
        println!(
            "[{name}] {key} entries={} sampled={} ok={ok} fail={fail} dds_ok={dds_ok} dds_bad={dds_bad}",
            files.len(),
            ok + fail
        );
        grand_ok += ok;
        grand_fail += fail;
        grand_dds_ok += dds_ok;
        grand_dds_bad += dds_bad;
    }

    println!("---");
    println!("TOTAL sampled_ok={grand_ok} sampled_fail={grand_fail} dds_ok={grand_dds_ok} dds_bad={grand_dds_bad}");
    for (k, c) in &versions {
        println!("VER {c}\t{k}");
    }
    for f in &failures {
        println!("FAIL {f}");
    }
}
