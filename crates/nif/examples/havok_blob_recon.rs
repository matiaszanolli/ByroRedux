//! `havok_blob_recon` — corpus survey of the `BhkSystemBinary` payload
//! (#3809, EX-14/15 item C4).
//!
//! `bhkPhysicsSystem` / `bhkRagdollSystem` decode to a raw byte blob that
//! nothing downstream reads. This walks FO4+ archives, pulls every blob out,
//! and reports what the *bytes* say about their own structure — magic,
//! section layout, size distribution — before any layout is assumed.
//!
//! Clean-room posture, same as the SpeedTree crate's: observations are
//! corpus-derived. No SDK header is copied and no SDK documentation is
//! paraphrased; what is recorded is what the files contain.
//!
//! Usage:
//!   havok_blob_recon <archive.ba2> [<substr>]        corpus survey
//!   havok_blob_recon <archive.ba2> --dump <inner>    hex one file's blobs

use byroredux_bsa::Ba2Archive;
use byroredux_nif::blocks::collision::BhkSystemBinary;
use std::collections::BTreeMap;

fn blobs_of(bytes: &[u8]) -> Vec<(&'static str, Vec<u8>)> {
    let Ok(scene) = byroredux_nif::parse_nif(bytes) else {
        return Vec::new();
    };
    scene
        .blocks
        .iter()
        .filter_map(|b| {
            b.as_any()
                .downcast_ref::<BhkSystemBinary>()
                .map(|s| (s.type_name, s.data.clone()))
        })
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: havok_blob_recon <archive.ba2> [<substr> | --dump <inner>]");
        std::process::exit(2);
    }
    let archive = Ba2Archive::open(&args[0]).expect("open archive");

    if args.get(1).map(|a| a == "--dump").unwrap_or(false) {
        let inner = args.get(2).expect("--dump needs an inner path");
        let bytes = archive.extract(inner).expect("extract");
        for (kind, blob) in blobs_of(&bytes) {
            println!("## {} — {} bytes\n", kind, blob.len());
            println!("```");
            for (row, chunk) in blob.chunks(16).take(24).enumerate() {
                let hex: Vec<String> = chunk.iter().map(|b| format!("{:02x}", b)).collect();
                let ascii: String = chunk
                    .iter()
                    .map(|&b| {
                        if (0x20..0x7f).contains(&b) {
                            b as char
                        } else {
                            '.'
                        }
                    })
                    .collect();
                println!("{:04}: {:<47}  {}", row * 16, hex.join(" "), ascii);
            }
            println!("```\n");
        }
        return;
    }

    if args.get(1).map(|a| a == "--fixups").unwrap_or(false) {
        let inner = args.get(2).expect("--fixups needs an inner path");
        let bytes = archive.extract(inner).expect("extract");
        for (_, blob) in blobs_of(&bytes) {
            fixups(&blob);
        }
        return;
    }
    if args.get(1).map(|a| a == "--census").unwrap_or(false) {
        census(&archive);
        return;
    }

    let needle = args
        .get(1)
        .cloned()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let names: Vec<String> = archive
        .list_files()
        .iter()
        .filter(|f| {
            let l = f.to_ascii_lowercase();
            l.ends_with(".nif") && l.contains(&needle)
        })
        .map(|f| f.to_string())
        .collect();

    let mut files_with_blob = 0u32;
    let mut blob_count = 0u32;
    let mut by_kind: BTreeMap<&'static str, u32> = BTreeMap::new();
    // First 4 bytes, as hex — the magic question.
    let mut magic4: BTreeMap<String, u32> = BTreeMap::new();
    // First 16 bytes of the very first blob seen, for the writeup.
    let mut sizes: Vec<usize> = Vec::new();
    // Printable 4-byte runs near the head — section tags in a tagfile.
    let mut head_tags: BTreeMap<String, u32> = BTreeMap::new();

    for name in &names {
        let Ok(bytes) = archive.extract(name) else {
            continue;
        };
        let blobs = blobs_of(&bytes);
        if blobs.is_empty() {
            continue;
        }
        files_with_blob += 1;
        for (kind, blob) in blobs {
            blob_count += 1;
            *by_kind.entry(kind).or_insert(0) += 1;
            sizes.push(blob.len());
            if blob.len() >= 4 {
                *magic4
                    .entry(blob[..4].iter().map(|b| format!("{:02x}", b)).collect())
                    .or_insert(0) += 1;
            }
            // Any 4-byte all-uppercase-ASCII run in the first 256 bytes is a
            // section-tag candidate; count them without assuming a layout.
            let head = &blob[..blob.len().min(256)];
            for w in head.windows(4) {
                if w.iter()
                    .all(|&b| b.is_ascii_uppercase() || b.is_ascii_digit())
                {
                    *head_tags
                        .entry(String::from_utf8_lossy(w).to_string())
                        .or_insert(0) += 1;
                }
            }
        }
    }

    println!("# `BhkSystemBinary` payload survey\n");
    println!("| metric | value |\n|---|---:|");
    println!("| `.nif` files matching filter | {} |", names.len());
    println!("| …carrying ≥1 blob | {} |", files_with_blob);
    println!("| total blobs | {} |", blob_count);
    for (kind, n) in &by_kind {
        println!("| …of which `{}` | {} |", kind, n);
    }
    if !sizes.is_empty() {
        sizes.sort_unstable();
        println!("| smallest blob (bytes) | {} |", sizes[0]);
        println!("| median blob (bytes) | {} |", sizes[sizes.len() / 2]);
        println!("| largest blob (bytes) | {} |", sizes[sizes.len() - 1]);
        println!("| total blob bytes | {} |", sizes.iter().sum::<usize>());
    }

    println!("\n## Leading 4 bytes\n");
    println!("| hex | blobs | share |\n|---|---:|---:|");
    let mut m: Vec<(&String, &u32)> = magic4.iter().collect();
    m.sort_by(|a, b| b.1.cmp(a.1));
    for (hex, n) in m.iter().take(10) {
        println!(
            "| `{}` | {} | {:.1}% |",
            hex,
            n,
            **n as f32 / blob_count.max(1) as f32 * 100.0
        );
    }

    println!("\n## Uppercase-ASCII 4-byte runs in the first 256 bytes\n");
    println!("Section-tag candidates, counted without assuming any layout.\n");
    println!("| run | occurrences | blobs it could cover |\n|---|---:|---:|");
    let mut t: Vec<(&String, &u32)> = head_tags.iter().collect();
    t.sort_by(|a, b| b.1.cmp(a.1));
    for (tag, n) in t.iter().take(20) {
        println!(
            "| `{}` | {} | {:.1}% |",
            tag,
            n,
            **n as f32 / blob_count.max(1) as f32 * 100.0
        );
    }
}

/// Corpus-wide validation of the shipped `parse_havok_packfile` decoder.
///
/// Every check is arithmetic or cross-referential, not assertive: offsets
/// must close on the blob length, every virtual fixup must name a class the
/// class-name table actually declares, and every fixup target must land
/// inside the section it claims. A wrong field order would not survive
/// thousands of files of differing sizes.
fn census(archive: &Ba2Archive) {
    use byroredux_nif::blocks::collision::parse_havok_packfile;

    let mut blobs = 0u32;
    let mut parsed = 0u32;
    let mut closed_exactly = 0u32;
    let mut all_vfixups_resolve = 0u32;
    let mut objects_ascending = 0u32;
    let mut global_targets_in_range = 0u32;
    let mut local_targets_in_range = 0u32;
    let mut failures: Vec<String> = Vec::new();

    let mut versions: BTreeMap<String, u32> = BTreeMap::new();
    let mut object_sets: BTreeMap<String, u32> = BTreeMap::new();
    let mut classes: BTreeMap<String, u32> = BTreeMap::new();
    let mut counts: BTreeMap<&'static str, Vec<usize>> = BTreeMap::new();

    let names: Vec<String> = archive
        .list_files()
        .iter()
        .filter(|f| f.to_ascii_lowercase().ends_with("_physics.nif"))
        .map(|f| f.to_string())
        .collect();

    for name in &names {
        let Ok(bytes) = archive.extract(name) else {
            continue;
        };
        for (_, blob) in blobs_of(&bytes) {
            blobs += 1;
            let Ok(pf) = parse_havok_packfile(&blob) else {
                if failures.len() < 5 {
                    failures.push(format!("{name}: parse failed"));
                }
                continue;
            };
            parsed += 1;
            *versions
                .entry(pf.header.contents_version.clone())
                .or_insert(0) += 1;
            for c in &pf.class_names {
                *classes.entry(c.clone()).or_insert(0) += 1;
            }

            if let Some(last) = pf.sections.last() {
                if last.absolute_end() as usize == blob.len() {
                    closed_exactly += 1;
                }
            }

            let data = pf.section("__data__");
            if let Some(d) = data {
                counts
                    .entry("local")
                    .or_default()
                    .push(d.local_fixups.len());
                counts
                    .entry("global")
                    .or_default()
                    .push(d.global_fixups.len());
                counts
                    .entry("virtual")
                    .or_default()
                    .push(d.virtual_fixups.len());

                if d.virtual_fixups
                    .iter()
                    .all(|v| pf.class_name_at(v.class_name_offset).is_some())
                {
                    all_vfixups_resolve += 1;
                }
                if d.global_fixups
                    .iter()
                    .all(|g| (g.dst_section as usize) < pf.sections.len())
                {
                    global_targets_in_range += 1;
                }
                let span = d.end_offset;
                if d.local_fixups
                    .iter()
                    .all(|l| l.src_offset < span && l.dst_offset < span)
                {
                    local_targets_in_range += 1;
                }
            }

            let objs = pf.objects();
            if objs
                .windows(2)
                .all(|w| w[0].section_offset < w[1].section_offset)
            {
                objects_ascending += 1;
            }
            *object_sets
                .entry(
                    objs.iter()
                        .map(|o| o.class_name.as_str())
                        .collect::<Vec<_>>()
                        .join(" + "),
                )
                .or_insert(0) += 1;
        }
    }

    let pct = |n: u32| n as f32 / blobs.max(1) as f32 * 100.0;
    println!("# Havok packfile decoder — corpus validation\n");
    println!("| check | blobs | share |\n|---|---:|---:|");
    println!("| blobs examined | {} | — |", blobs);
    println!(
        "| `parse_havok_packfile` succeeds | {} | {:.1}% |",
        parsed,
        pct(parsed)
    );
    println!(
        "| last section `absolute_end()` == blob length | {} | {:.1}% |",
        closed_exactly,
        pct(closed_exactly)
    );
    println!(
        "| **every virtual fixup resolves to a declared class** | **{}** | **{:.1}%** |",
        all_vfixups_resolve,
        pct(all_vfixups_resolve)
    );
    println!(
        "| objects in strictly ascending offset order | {} | {:.1}% |",
        objects_ascending,
        pct(objects_ascending)
    );
    println!(
        "| every global fixup names a real section | {} | {:.1}% |",
        global_targets_in_range,
        pct(global_targets_in_range)
    );
    println!(
        "| every local fixup lands inside the section | {} | {:.1}% |",
        local_targets_in_range,
        pct(local_targets_in_range)
    );
    if !failures.is_empty() {
        println!("\nFailures:\n");
        for f in &failures {
            println!("- `{}`", f);
        }
    }

    println!("\n## Fixup-table sizes\n");
    println!("| table | min | median | max |\n|---|---:|---:|---:|");
    for (k, v) in counts.iter_mut() {
        v.sort_unstable();
        println!(
            "| {} | {} | {} | {} |",
            k,
            v[0],
            v[v.len() / 2],
            v[v.len() - 1]
        );
    }

    println!("\n## SDK version string\n");
    println!("| value | blobs |\n|---|---:|");
    for (k, n) in &versions {
        println!("| `{}` | {} |", k, n);
    }

    println!("\n## Object sets found in `__data__`\n");
    println!("| objects, in file order | blobs |\n|---|---:|");
    let mut o: Vec<(&String, &u32)> = object_sets.iter().collect();
    o.sort_by(|a, b| b.1.cmp(a.1));
    for (k, n) in o.iter().take(8) {
        println!("| {} | {} |", k, n);
    }

    println!("\n## Classes declared\n");
    println!("| class | blobs |\n|---|---:|");
    let mut c: Vec<(&String, &u32)> = classes.iter().collect();
    c.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (k, n) in c.iter() {
        println!("| `{}` | {} |", k, n);
    }
}

/// Dump the object table and fixups via the shipped decoder.
fn fixups(blob: &[u8]) {
    let pf = byroredux_nif::blocks::collision::parse_havok_packfile(blob).expect("packfile");
    let data = pf.section("__data__").expect("__data__");
    println!(
        "blob {} bytes; __data__ base {}",
        blob.len(),
        data.absolute_data_start
    );
    println!("\n### objects (from virtual fixups)\n");
    for o in pf.objects() {
        println!(
            "  data+{:<8} abs {:<8} {}",
            o.section_offset, o.absolute_offset, o.class_name
        );
    }
    println!("\n### global fixups ({})\n", data.global_fixups.len());
    for g in &data.global_fixups {
        println!(
            "  data+{:<8} -> section {} offset {}",
            g.src_offset, g.dst_section, g.dst_offset
        );
    }
    println!(
        "\n### local fixups ({}, first 12)\n",
        data.local_fixups.len()
    );
    for l in data.local_fixups.iter().take(12) {
        println!("  data+{:<8} -> data+{}", l.src_offset, l.dst_offset);
    }
}
