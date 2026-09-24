//! Census a game's distant-LOD corpus in one or more BSA/BA2 archives
//! (#3321).
//!
//! `exal.md` and `placement_lod.rs` both carried the #2086 claim that
//! "FO3/FNV ship neither LOD scheme for distant objects", reached from a
//! `distantlod\\` / `_far.nif` probe alone. That is the wrong question: those
//! two names cover Oblivion's scheme, and Fallout's lives under
//! `landscape\\lod\\<world>\\blocks\\`. This probe counts all three families
//! side by side so the next such claim is made against a full inventory.
//!
//! Both archive families are opened (EXT-D6-2026-09-19-03 / #4502): the
//! leading magic picks `BsaArchive` (`BSA\0`, Oblivion → Skyrim SE) or
//! `Ba2Archive` (`BTDX`, FO4/FO76/Starfield), so a BA2 game produces real
//! counts instead of a bare `skip` that reads as "no LOD content".
//!
//! Splits `landscape\\lod` entries into the *terrain* quadtree and its
//! *blocks* (object-LOD) sibling, per worldspace, per level.
//!
//! #4737 — the fourth family, the Creation baked-quad scheme used by
//! Skyrim, FO4 and FO76 (`meshes\terrain\<ws>\<ws>.<L>.<x>.<y>.btr` and
//! the `objects\*.bto` sibling), is counted too, per worldspace, per
//! level; BA2 names use `/`, normalised here. #4502 opened the BA2s but
//! matched none of the families those games actually ship, so the census
//! printed a false zero for exactly the games it was run on.
//!
//! Usage:
//!   cargo run -p byroredux-bsa --example probe_lod_corpus -- <ARCHIVE> [ARCHIVE ...]

use std::fs::File;
use std::io::{self, Read};

/// Either archive family, selected by the leading four bytes: `BsaArchive`
/// and `Ba2Archive` each hard-reject the other's file, so opening blind
/// cannot work.
enum AnyArchive {
    Bsa(byroredux_bsa::BsaArchive),
    Ba2(byroredux_bsa::Ba2Archive),
}

impl AnyArchive {
    fn open(path: &str) -> io::Result<Self> {
        let mut magic = [0u8; 4];
        File::open(path)?.read_exact(&mut magic)?;
        match magic {
            [b'B', b'S', b'A', 0] => Ok(Self::Bsa(byroredux_bsa::BsaArchive::open(path)?)),
            [b'B', b'T', b'D', b'X'] => Ok(Self::Ba2(byroredux_bsa::Ba2Archive::open(path)?)),
            other => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported magic {:?}", String::from_utf8_lossy(&other)),
            )),
        }
    }

    fn list_files(&self) -> Vec<&str> {
        match self {
            Self::Bsa(a) => a.list_files(),
            Self::Ba2(a) => a.list_files(),
        }
    }
}

fn main() {
    let mut total = 0usize;
    for path in std::env::args().skip(1) {
        let archive = match AnyArchive::open(&path) {
            Ok(archive) => archive,
            Err(e) => {
                eprintln!("skip {path} ({e})");
                continue;
            }
        };
        let mut lod = 0usize;
        let mut blocks: std::collections::BTreeMap<
            String,
            std::collections::BTreeMap<String, usize>,
        > = Default::default();
        let mut terrain: std::collections::BTreeMap<
            String,
            std::collections::BTreeMap<String, usize>,
        > = Default::default();
        let mut far_nif = 0usize;
        let mut distantlod = 0usize;
        let mut high_variant = 0usize;
        let mut creation = 0usize;
        let mut creation_terrain: std::collections::BTreeMap<
            String,
            std::collections::BTreeMap<String, usize>,
        > = Default::default();
        let mut creation_objects: std::collections::BTreeMap<
            String,
            std::collections::BTreeMap<String, usize>,
        > = Default::default();
        for f in archive.list_files() {
            // #4737 — BA2 names use `/`; the Creation matcher below keys on
            // the BSA-style backslash form.
            let l = f.to_ascii_lowercase().replace('/', "\\");
            if l.ends_with("_far.nif") {
                far_nif += 1;
            }
            if l.contains("distantlod\\") {
                distantlod += 1;
            }
            // #4468 — the Fallout-legacy DLC archives bake a second,
            // higher-detail object-quad variant beside the plain form
            // (FO3 `Anchorage - Main.bsa` 60, FNV `LonesomeRoad -
            // Main.bsa` 19); count it so the variant stays visible in
            // the census.
            if l.contains(".high.") {
                high_variant += 1;
            }
            // #4737 — the Creation family: `meshes\terrain\<ws>\` holds the
            // per-quad baked terrain (`.btr`) beside its `objects\` folder
            // of object-LOD (`.bto`). Level-first stem: `<ws>.<L>.<x>.<y>`.
            if let Some(rest) = l.strip_prefix("meshes\\terrain\\") {
                let (world, file) = match rest.split_once('\\') {
                    Some(pair) => pair,
                    None => continue,
                };
                let (stem, ext) = match file.rsplit_once('.') {
                    Some(pair) => pair,
                    None => continue,
                };
                if !matches!(ext, "btr" | "bto") {
                    continue;
                }
                // <ws>.<L>.<x>.<y> — the level is the second dot field;
                // anything else is not a quadtree quad name.
                let level = match stem.split('.').nth(1) {
                    Some(level) if stem.split('.').count() >= 4 => level,
                    _ => continue,
                };
                creation += 1;
                let bucket = if file.starts_with("objects\\") {
                    &mut creation_objects
                } else {
                    &mut creation_terrain
                };
                *bucket
                    .entry(world.to_string())
                    .or_default()
                    .entry(level.to_string())
                    .or_default() += 1;
            }
            if !l.contains("landscape\\lod\\") {
                continue;
            }
            lod += 1;
            let rest = &l[l.find("landscape\\lod\\").unwrap() + "landscape\\lod\\".len()..];
            let world = rest.split('\\').next().unwrap_or("?").to_string();
            let level = rest
                .rsplit('.')
                .find(|s| s.starts_with("level"))
                .unwrap_or_else(|| {
                    rest.split('.')
                        .find(|s| s.starts_with("level"))
                        .unwrap_or("?")
                })
                .to_string();
            let bucket = if rest.contains("\\blocks\\") {
                &mut blocks
            } else {
                &mut terrain
            };
            *bucket.entry(world).or_default().entry(level).or_default() += 1;
        }
        println!(
            "{path}\n  landscape\\lod entries={lod}  _far.nif={far_nif}  distantlod={distantlod}  .high.={high_variant}  meshes\\terrain .btr/.bto={creation}"
        );
        for (label, map) in [
            ("terrain", &terrain),
            ("blocks ", &blocks),
            ("btr    ", &creation_terrain),
            ("bto    ", &creation_objects),
        ] {
            for (world, levels) in map {
                let n: usize = levels.values().sum();
                println!("    {label} {world}: {n}  {levels:?}");
            }
        }
        total += lod;
    }
    println!("TOTAL landscape\\lod entries across archives: {total}");
}
