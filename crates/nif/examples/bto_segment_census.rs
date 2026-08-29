//! #3307 — what granularity does a baked `.bto` object-LOD quad expose?
//!
//! Establishes, from shipped data rather than from the format docs alone,
//! whether a VWD full-model cull has anything finer than the whole quad to
//! suppress. Prints, per LOD level: how many sub-meshes a quad spawns, how
//! their names read, and the `BSSubIndexTriShape` segment-count histogram —
//! plus one worked example showing how a quad's segments partition its
//! triangles.
//!
//! Finding (Skyrim SE `Skyrim - Meshes1.bsa`, 1078 quads, 2026-08-29):
//!   * sub-mesh names are **material/type groups** (`Obj`, `obj-LargeRef`,
//!     `objsnowHD-LargeRef`, ...) — never a source object, never a FormID.
//!   * level 4 (`4x4 = 16` cells per quad): `num_segments` caps at exactly
//!     **16** and the non-zero segments exactly partition the triangles —
//!     i.e. one segment **per cell**, matching nif.xml's "segmented in a grid
//!     (for LOD)" for `BSGeometrySegmentData`.
//!   * levels 8 and 16: exactly **1** segment on every sub-mesh — no
//!     sub-quad granularity at all.
//!
//! Usage: `cargo run --release -p byroredux-nif --example bto_segment_census -- <archive>`
use byroredux_bsa::BsaArchive;
use std::collections::BTreeMap;

fn main() {
    let bsa = std::env::args().nth(1).expect("usage: <archive>");
    let archive = BsaArchive::open(&bsa).unwrap();
    // level -> segment-count -> occurrences
    let mut by_level: BTreeMap<u32, BTreeMap<u32, usize>> = BTreeMap::new();
    let mut submesh_names: BTreeMap<String, usize> = BTreeMap::new();
    let mut files = 0usize;
    for f in archive.list_files() {
        let l = f.to_ascii_lowercase();
        if !l.ends_with(".bto") {
            continue;
        }
        // meshes\terrain\<world>\objects\<world>.<level>.<x>.<y>.bto
        let stem = l.rsplit('\\').next().unwrap_or(&l);
        let parts: Vec<&str> = stem.split('.').collect();
        let Some(level) = parts.get(1).and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(bytes) = archive.extract(f) else {
            continue;
        };
        let Ok(scene) = byroredux_nif::parse_nif(&bytes) else {
            continue;
        };
        files += 1;
        let mut pool = byroredux_core::string::StringPool::new();
        for m in byroredux_nif::import::import_nif(&scene, &mut pool) {
            if let Some(n) = m.name.as_ref() {
                *submesh_names.entry(n.to_string()).or_insert(0) += 1;
            }
            if let Some(d) = m.bs_sub_index.as_ref() {
                *by_level
                    .entry(level)
                    .or_default()
                    .entry(d.num_segments)
                    .or_insert(0) += 1;
            }
        }
    }
    println!("# {files} .bto files");
    for (level, counts) in &by_level {
        let max = counts.keys().max().copied().unwrap_or(0);
        let total: usize = counts.values().sum();
        println!("level {level}: {total} sub-meshes, max num_segments={max}");
        let mut rows: Vec<_> = counts.iter().collect();
        rows.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
        let top: Vec<String> = rows
            .iter()
            .take(6)
            .map(|(k, c)| format!("{k}:{c}"))
            .collect();
        println!("   segment-count histogram (top): {}", top.join("  "));
    }
    // One worked example: how a level-4 quad's segments partition its triangles.
    if let Some(f) = archive.list_files().iter().find(|f| {
        f.to_ascii_lowercase().contains(".4.") && f.to_ascii_lowercase().ends_with(".bto")
    }) {
        if let Ok(bytes) = archive.extract(f) {
            if let Ok(scene) = byroredux_nif::parse_nif(&bytes) {
                let mut pool = byroredux_core::string::StringPool::new();
                println!("--- worked example: {f} ---");
                for (i, m) in byroredux_nif::import::import_nif(&scene, &mut pool)
                    .iter()
                    .enumerate()
                {
                    let Some(d) = m.bs_sub_index.as_ref() else {
                        continue;
                    };
                    let used: Vec<String> = d
                        .segments
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| s.num_primitives > 0)
                        .map(|(j, s)| format!("cell#{j}:{}tri", s.num_primitives))
                        .collect();
                    let sum: u32 = d.segments.iter().map(|s| s.num_primitives).sum();
                    println!(
                        "  [{i}] name={:<24} segments={:<3} tris={:<6} non-empty=[{}] sum={sum}",
                        m.name
                            .as_ref()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "-".into()),
                        d.num_segments,
                        m.indices.len() / 3,
                        used.join(" ")
                    );
                }
            }
        }
    }
    println!("--- distinct sub-mesh names ---");
    let mut rows: Vec<_> = submesh_names.into_iter().collect();
    rows.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    for (n, c) in rows.iter().take(12) {
        println!("  {c:6}  {n}")
    }
}
