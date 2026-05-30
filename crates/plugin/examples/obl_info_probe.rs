//! Adversarial probe for audit finding OBL-D3-2026-05-28-04.
//! Walks Oblivion.esm raw bytes, finds DIAL top-group + Topic Children
//! (group_type==7), and for each INFO dumps: TRDT length + first 16
//! bytes (to inspect EmotionType@0 vs Response_Type@0), QSTI presence,
//! and the parent DIAL's DATA byte. Pure raw walk — does not trust the
//! engine parser's per-field interpretation.

use std::convert::TryInto;
use std::sync::Mutex;

static REC_TYPES: Mutex<std::collections::BTreeMap<String, usize>> =
    Mutex::new(std::collections::BTreeMap::new());

fn rd_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/Oblivion.esm".into()
    });
    let data = std::fs::read(&path).expect("read esm");
    eprintln!("file: {} ({} bytes)", path, data.len());

    let mut dial_count = 0usize;
    let mut info_count = 0usize;
    let mut info_with_trdt = 0usize;
    let mut info_with_qsti = 0usize;
    let mut trdt_len_hist: std::collections::BTreeMap<u32, usize> = Default::default();
    let mut byte0_hist: std::collections::BTreeMap<u8, usize> = Default::default();
    let mut emotype_hist: std::collections::BTreeMap<u32, usize> = Default::default();
    let mut samples = 0usize;

    // Walk top-level: records + GRUPs. We descend any GRUP and parse any
    // INFO / DIAL record we encounter. Oblivion record header = 20 bytes
    // (type[4] dataSize[4] flags[4] formid[4] vc[4]); GRUP header = 24
    // bytes (GRUP[4] groupSize[4] label[4] type[4] stamp[4] unk[4]).
    fn walk(
        b: &[u8],
        start: usize,
        end: usize,
        dial: &mut usize,
        info: &mut usize,
        info_trdt: &mut usize,
        info_qsti: &mut usize,
        trdt_len_hist: &mut std::collections::BTreeMap<u32, usize>,
        byte0_hist: &mut std::collections::BTreeMap<u8, usize>,
        emotype_hist: &mut std::collections::BTreeMap<u32, usize>,
        samples: &mut usize,
    ) {
        let mut p = start;
        let top = start == 0;
        while p + 24 <= end {
            let ty = &b[p..p + 4];
            let _ = top;
            if ty == b"GRUP" {
                let group_size = rd_u32(b, p + 4) as usize;
                if group_size < 24 || p + group_size > end {
                    break;
                }
                walk(
                    b,
                    p + 24,
                    p + group_size,
                    dial,
                    info,
                    info_trdt,
                    info_qsti,
                    trdt_len_hist,
                    byte0_hist,
                    emotype_hist,
                    samples,
                );
                p += group_size;
            } else {
                // Record: 20-byte header (Oblivion TES4)
                if p + 20 > end {
                    break;
                }
                let data_size = rd_u32(b, p + 4) as usize;
                let flags = rd_u32(b, p + 8);
                let body_start = p + 20;
                let body_end = body_start + data_size;
                if body_end > end {
                    break;
                }
                let compressed = flags & 0x0004_0000 != 0;
                {
                    let t = String::from_utf8_lossy(ty).to_string();
                    *REC_TYPES.lock().unwrap().entry(t).or_insert(0) += 1;
                }
                if (ty == b"INFO" || ty == b"DIAL") && !compressed {
                    // Walk sub-records: type[4] size[2] data[size]
                    let mut s = body_start;
                    let mut has_trdt = false;
                    let mut has_qsti = false;
                    while s + 6 <= body_end {
                        let st = &b[s..s + 4];
                        let ssz = u16::from_le_bytes(b[s + 4..s + 6].try_into().unwrap()) as usize;
                        let sd = s + 6;
                        if sd + ssz > body_end {
                            break;
                        }
                        if ty == b"INFO" && st == b"TRDT" {
                            has_trdt = true;
                            *trdt_len_hist.entry(ssz as u32).or_insert(0) += 1;
                            if ssz >= 1 {
                                *byte0_hist.entry(b[sd]).or_insert(0) += 1;
                            }
                            if ssz >= 4 {
                                *emotype_hist.entry(rd_u32(b, sd)).or_insert(0) += 1;
                            }
                            if *samples < 8 {
                                let n = ssz.min(16);
                                let hex: Vec<String> =
                                    b[sd..sd + n].iter().map(|x| format!("{:02x}", x)).collect();
                                eprintln!(
                                    "  INFO TRDT len={} bytes=[{}]",
                                    ssz,
                                    hex.join(" ")
                                );
                                *samples += 1;
                            }
                        }
                        if ty == b"INFO" && st == b"QSTI" {
                            has_qsti = true;
                        }
                        s = sd + ssz;
                    }
                    if ty == b"INFO" {
                        *info += 1;
                        if has_trdt {
                            *info_trdt += 1;
                        }
                        if has_qsti {
                            *info_qsti += 1;
                        }
                    } else {
                        *dial += 1;
                    }
                }
                p = body_end;
            }
        }
    }

    walk(
        &data,
        0,
        data.len(),
        &mut dial_count,
        &mut info_count,
        &mut info_with_trdt,
        &mut info_with_qsti,
        &mut trdt_len_hist,
        &mut byte0_hist,
        &mut emotype_hist,
        &mut samples,
    );

    eprintln!("\n=== Oblivion.esm INFO/DIAL raw probe ===");
    eprintln!("DIAL records         : {}", dial_count);
    eprintln!("INFO records         : {}", info_count);
    eprintln!("INFO with TRDT       : {}", info_with_trdt);
    eprintln!("INFO with QSTI       : {}", info_with_qsti);
    eprintln!("TRDT length histogram: {:?}", trdt_len_hist);
    eprintln!("TRDT byte[0] histogram (what code reads as response_type):");
    for (k, v) in &byte0_hist {
        eprintln!("    byte0 = {} ({:#04x}) -> {} INFOs", k, k, v);
    }
    eprintln!("TRDT EmotionType(u32@0) histogram (top 12):");
    let mut emo: Vec<_> = emotype_hist.iter().collect();
    emo.sort_by(|a, b| b.1.cmp(a.1));
    for (k, v) in emo.iter().take(12) {
        eprintln!("    emoType = {} -> {} INFOs", k, v);
    }
    let _ = samples;

    eprintln!("\n=== record-type histogram (selected) ===");
    let rt = REC_TYPES.lock().unwrap();
    for k in ["TES4", "DIAL", "INFO", "GRUP", "CELL", "WRLD", "NPC_"] {
        if let Some(v) = rt.get(k) {
            eprintln!("    {} -> {}", k, v);
        }
    }
    eprintln!("    (distinct record types seen: {})", rt.len());
    eprintln!("--- full type histogram ---");
    for (k, v) in rt.iter() {
        eprintln!("    {:?} -> {}", k, v);
    }
}
