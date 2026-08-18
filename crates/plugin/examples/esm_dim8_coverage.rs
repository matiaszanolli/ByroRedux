//! Dimension-8 real-data coverage probe (throwaway audit scratch).
//!
//! Walks an ESM/ESP end-to-end and emits a TSV census:
//!   FILE <path> <hedr> <variant> <gamekind> <hdr_record_count> <bytes>
//!   TOP  <label> <count>              -- top-level (group_type 0) GRUP labels
//!   REC  <rectype> <count>            -- every record header seen, any depth
//!   SUB  <rectype> <subcode> <occurrences> <records_carrying_it>
//!
//! Usage: cargo run --release -p byroredux-plugin --example esm_dim8_coverage -- <file.esm> [--no-subs]

use byroredux_plugin::esm::reader::{EsmReader, GameKind};
use std::collections::HashMap;

struct Census {
    records: HashMap<[u8; 4], u64>,
    subs: HashMap<([u8; 4], [u8; 4]), (u64, u64)>,
    top: Vec<([u8; 4], u64)>,
    want_subs: bool,
    errors: u64,
    groups: u64,
    cur_top: [u8; 4],
    pairs: HashMap<([u8; 4], [u8; 4]), u64>,
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: esm_dim8_coverage <file.esm>"))?;
    let want_subs = !args.any(|a| a == "--no-subs");

    let bytes = std::fs::read(&path)?;
    let mut reader = EsmReader::new(&bytes);
    let variant = reader.variant();
    let header = reader.read_file_header()?;
    let game = GameKind::from_header(variant, header.hedr_version, header.record_version);
    println!(
        "FILE\t{}\t{}\t{:?}\t{:?}\t{}\t{}\t{}\t{}",
        path,
        header.hedr_version,
        variant,
        game,
        header.record_count,
        bytes.len(),
        header.localized,
        header.master_files.len(),
    );

    let mut census = Census {
        records: HashMap::new(),
        subs: HashMap::new(),
        top: Vec::new(),
        want_subs,
        errors: 0,
        groups: 0,
        cur_top: *b"????",
        pairs: HashMap::new(),
    };

    // Top level: enumerate GRUPs, recording labels of group_type 0.
    let end = bytes.len();
    while reader.position() < end && reader.remaining() > 0 {
        if !reader.is_group() {
            let h = match reader.read_record_header() {
                Ok(h) => h,
                Err(_) => break,
            };
            *census.records.entry(h.record_type).or_insert(0) += 1;
            reader.skip_record(&h);
            continue;
        }
        let group = match reader.read_group_header() {
            Ok(g) => g,
            Err(_) => break,
        };
        let inner_end = reader.group_content_end(&group);
        census.groups += 1;
        if group.group_type == 0 {
            census.cur_top = group.label;
        }
        let before: u64 = census.records.values().sum();
        walk(&mut reader, inner_end, &mut census);
        let after: u64 = census.records.values().sum();
        if group.group_type == 0 {
            census.top.push((group.label, after - before));
        }
        // Re-sync if `walk` bailed out early on a malformed sub-tree.
        if reader.position() < inner_end {
            let delta = inner_end - reader.position();
            reader.skip(delta);
        }
    }

    let mut tops = census.top.clone();
    tops.sort();
    for (label, count) in tops {
        println!("TOP\t{}\t{}", fourcc(&label), count);
    }
    let mut recs: Vec<_> = census.records.iter().collect();
    recs.sort();
    for (ty, count) in recs {
        println!("REC\t{}\t{}", fourcc(ty), count);
    }
    if want_subs {
        let mut subs: Vec<_> = census.subs.iter().collect();
        subs.sort();
        for ((ty, code), (occ, carriers)) in subs {
            println!(
                "SUB\t{}\t{}\t{}\t{}",
                fourcc(ty),
                fourcc(code),
                occ,
                carriers
            );
        }
    }
    let mut pairs: Vec<_> = census.pairs.iter().collect();
    pairs.sort();
    for ((top, rt), n) in pairs {
        println!("PAIR\t{}\t{}\t{}", fourcc(top), fourcc(rt), n);
    }
    println!("GROUPS\t{}", census.groups);
    println!("ERRORS\t{}", census.errors);
    Ok(())
}

fn walk(reader: &mut EsmReader<'_>, end: usize, census: &mut Census) {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let group = match reader.read_group_header() {
                Ok(g) => g,
                Err(_) => {
                    census.errors += 1;
                    return;
                }
            };
            census.groups += 1;
            let inner_end = reader.group_content_end(&group);
            if inner_end < reader.position() || inner_end > end {
                census.errors += 1;
                eprintln!(
                    "ERR grup-bounds label={} type={} pos={} inner_end={} end={} total_size={}",
                    fourcc(&group.label),
                    group.group_type,
                    reader.position(),
                    inner_end,
                    end,
                    group.total_size
                );
                return;
            }
            walk(reader, inner_end, census);
            continue;
        }
        let header = match reader.read_record_header() {
            Ok(h) => h,
            Err(e) => {
                census.errors += 1;
                eprintln!("ERR rechdr pos={} end={}: {e}", reader.position(), end);
                return;
            }
        };
        *census.records.entry(header.record_type).or_insert(0) += 1;
        *census
            .pairs
            .entry((census.cur_top, header.record_type))
            .or_insert(0) += 1;
        if census.want_subs {
            match reader.read_sub_records(&header) {
                Ok(subs) => {
                    let mut seen: Vec<[u8; 4]> = Vec::new();
                    for sub in &subs {
                        let e = census
                            .subs
                            .entry((header.record_type, sub.sub_type))
                            .or_insert((0, 0));
                        e.0 += 1;
                        if !seen.contains(&sub.sub_type) {
                            seen.push(sub.sub_type);
                            e.1 += 1;
                        }
                    }
                }
                Err(e) => {
                    // Position is indeterminate after a failed decompress;
                    // bail out of this sub-tree and let the caller re-sync.
                    census.errors += 1;
                    eprintln!(
                        "ERR subrec type={} form={:08X} size={} flags={:08X}: {e}",
                        fourcc(&header.record_type),
                        header.form_id,
                        header.data_size,
                        header.flags
                    );
                    return;
                }
            }
        } else {
            reader.skip_record(&header);
        }
    }
}

fn fourcc(b: &[u8; 4]) -> String {
    b.iter()
        .map(|c| {
            if c.is_ascii_graphic() {
                *c as char
            } else {
                '.'
            }
        })
        .collect()
}
