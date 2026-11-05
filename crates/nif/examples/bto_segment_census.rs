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
//! #6636 (V6, V7) — the census reads BA2 as well as BSA (the FO4 `.bto` quads), and `--blocks` runs it over
//! the Fallout 3 / New Vegas `landscape\lod\<ws>\blocks\*.nif` quads, where there is no segment data to read:
//! it prints the NIF block-type histogram instead, so "do those quads partition one per cell" is answered
//! by what the files contain.
//!
//! Usage: `cargo run --release -p byroredux-nif --example bto_segment_census -- [--blocks | --textures] <archive>`
use byroredux_bsa::{Ba2Archive, BsaArchive};
use std::collections::BTreeMap;

/// Either archive family, picked by the leading magic.
enum Archive {
    Bsa(BsaArchive),
    Ba2(Ba2Archive),
}

impl Archive {
    fn open(path: &str) -> Self {
        let mut magic = [0u8; 4];
        std::io::Read::read_exact(&mut std::fs::File::open(path).unwrap(), &mut magic).unwrap();
        if &magic == b"BTDX" {
            Self::Ba2(Ba2Archive::open(path).unwrap())
        } else {
            Self::Bsa(BsaArchive::open(path).unwrap())
        }
    }

    fn list_files(&self) -> Vec<&str> {
        match self {
            Self::Bsa(a) => a.list_files(),
            Self::Ba2(a) => a.list_files(),
        }
    }

    fn extract(&self, path: &str) -> std::io::Result<Vec<u8>> {
        match self {
            Self::Bsa(a) => a.extract(path),
            Self::Ba2(a) => a.extract(path),
        }
    }
}

/// V7 — the block-type histogram of every `landscape\lod\<ws>\blocks\*.nif` quad in `archive`.
fn blocks_census(archive: &Archive) {
    let mut types: BTreeMap<&'static str, usize> = BTreeMap::new();
    let (mut files, mut segmented) = (0usize, 0usize);
    for f in archive.list_files() {
        let l = f.to_ascii_lowercase().replace('/', "\\");
        if !(l.contains("landscape\\lod\\") && l.contains("\\blocks\\") && l.ends_with(".nif")) {
            continue;
        }
        let Ok(bytes) = archive.extract(f) else { continue };
        let Ok(scene) = byroredux_nif::parse_nif(&bytes) else { continue };
        files += 1;
        let mut has_segments = false;
        for block in &scene.blocks {
            let name = block.block_type_name();
            *types.entry(name).or_default() += 1;
            has_segments |= name.contains("Segment") || name.contains("SubIndex");
        }
        segmented += usize::from(has_segments);
    }
    println!("# {files} blocks\\*.nif quads, {segmented} with any segmented / sub-index block");
    let mut rows: Vec<_> = types.into_iter().collect();
    rows.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    for (t, c) in rows {
        println!("  {c:7}  {t}");
    }
}

/// V13 — the diffuse texture and `.bgsm` material paths every `.bto` sub-mesh names, grouped by their first four
/// path components, with the number naming a tree asset (`tree` anywhere in the path).
fn textures_census(archive: &Archive) {
    let mut prefixes: BTreeMap<String, usize> = BTreeMap::new();
    let (mut meshes, mut tree_named) = (0usize, 0usize);
    for f in archive.list_files() {
        if !f.to_ascii_lowercase().ends_with(".bto") {
            continue;
        }
        let Ok(bytes) = archive.extract(f) else { continue };
        let Ok(scene) = byroredux_nif::parse_nif(&bytes) else { continue };
        let mut pool = byroredux_core::string::StringPool::new();
        for m in byroredux_nif::import::import_nif(&scene, &mut pool) {
            meshes += 1;
            for path in [m.material.textures.base_color, m.material.material_path].into_iter().flatten() {
                let path = pool.resolve(path).unwrap_or("").to_ascii_lowercase().replace('/', "\\");
                tree_named += usize::from(path.contains("tree"));
                let prefix: Vec<&str> = path.split('\\').take(4).collect();
                *prefixes.entry(prefix.join("\\")).or_default() += 1;
            }
        }
    }
    println!("# {meshes} .bto sub-meshes; {tree_named} texture / material paths mention `tree`");
    let mut rows: Vec<_> = prefixes.into_iter().collect();
    rows.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    for (p, c) in rows.iter().take(12) {
        println!("  {c:6}  {p}");
    }
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let blocks = args.first().is_some_and(|a| a == "--blocks");
    let textures = args.first().is_some_and(|a| a == "--textures");
    if blocks || textures {
        args.remove(0);
    }
    let bsa = args.first().expect("usage: [--blocks | --textures] <archive>").clone();
    let archive = Archive::open(&bsa);
    if blocks {
        blocks_census(&archive);
        return;
    }
    if textures {
        textures_census(&archive);
        return;
    }
    // level -> segment-count -> occurrences
    let mut by_level: BTreeMap<u32, BTreeMap<u32, usize>> = BTreeMap::new();
    let mut submesh_names: BTreeMap<String, usize> = BTreeMap::new();
    let mut files = 0usize;
    // Level-4 sub-meshes whose segments' primitive counts sum to the mesh's triangle count (a partition).
    let (mut partitioned, mut level4) = (0usize, 0usize);
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
                if level == 4 {
                    level4 += 1;
                    let sum: u32 = d.segments.iter().map(|s| s.num_primitives).sum();
                    partitioned += usize::from(sum as usize == m.indices.len() / 3);
                }
                *by_level
                    .entry(level)
                    .or_default()
                    .entry(d.num_segments)
                    .or_insert(0) += 1;
            }
        }
    }
    println!("# {files} .bto files; level-4 sub-meshes whose segments partition their triangles: {partitioned} of {level4}");
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
