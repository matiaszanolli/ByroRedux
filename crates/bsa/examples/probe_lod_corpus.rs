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
//! #4933 — `TOTAL` now counts every file any family matched (once each),
//! not just `landscape\lod`, so a Creation-era run no longer ends on a
//! false `0`. Three more families are counted: Skyrim's tree-LOD `.btt`
//! beside the `.btr`/`.bto` quads, Starfield's `meshes\lod\generated\`
//! NIFs, and the `lodsettings\*.lod` worldspace descriptors.
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

/// #4737 — the Creation family: `meshes\terrain\<ws>\` holds the
/// per-quad baked terrain (`.btr`) beside its `objects\` folder of
/// object-LOD (`.bto`) and (Skyrim, #4933) its `trees\` folder of tree-LOD
/// (`.btt`). Level-first stem: `<ws>.<L>.<x>.<y>`. Returns
/// `(worldspace, extension, level)` for a quad name, `None` otherwise.
fn creation_quad(l: &str) -> Option<(&str, &str, &str)> {
    let rest = l.strip_prefix("meshes\\terrain\\")?;
    let (world, file) = rest.split_once('\\')?;
    let (stem, ext) = file.rsplit_once('.')?;
    if !matches!(ext, "btr" | "bto" | "btt") {
        return None;
    }
    // <ws>.<L>.<x>.<y> — the level is the second dot field; anything else
    // is not a quadtree quad name.
    let name = stem.rsplit('\\').next()?;
    if name.split('.').count() < 4 {
        return None;
    }
    Some((world, ext, name.split('.').nth(1)?))
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
        let mut generated = 0usize;
        let mut lodsettings = 0usize;
        let mut matched = 0usize;
        let mut creation_terrain: std::collections::BTreeMap<
            String,
            std::collections::BTreeMap<String, usize>,
        > = Default::default();
        let mut creation_objects: std::collections::BTreeMap<
            String,
            std::collections::BTreeMap<String, usize>,
        > = Default::default();
        let mut creation_trees: std::collections::BTreeMap<
            String,
            std::collections::BTreeMap<String, usize>,
        > = Default::default();
        for f in archive.list_files() {
            // #4737 — BA2 names use `/`; the Creation matcher below keys on
            // the BSA-style backslash form.
            let l = f.to_ascii_lowercase().replace('/', "\\");
            // #4933 — one hit per file for `TOTAL`, however many of the
            // (overlapping, e.g. `.high.`) family counters it bumps.
            let mut is_lod = false;
            if l.ends_with("_far.nif") {
                far_nif += 1;
                is_lod = true;
            }
            if l.contains("distantlod\\") {
                distantlod += 1;
                is_lod = true;
            }
            // #4933 — Starfield's LOD corpus: generated NIFs under
            // `meshes\lod\generated\` (no `.btr`/`.bto` quads at all).
            if l.starts_with("meshes\\lod\\generated\\") && l.ends_with(".nif") {
                generated += 1;
                is_lod = true;
            }
            // #4933 — the per-worldspace `lodsettings\<ws>.lod` descriptors.
            if l.starts_with("lodsettings\\") && l.ends_with(".lod") {
                lodsettings += 1;
                is_lod = true;
            }
            // #4468 — the Fallout-legacy DLC archives bake a second,
            // higher-detail object-quad variant beside the plain form
            // (FO3 `Anchorage - Main.bsa` 60, FNV `LonesomeRoad -
            // Main.bsa` 19); count it so the variant stays visible in
            // the census.
            if l.contains(".high.") {
                high_variant += 1;
            }
            if let Some((world, ext, level)) = creation_quad(&l) {
                creation += 1;
                is_lod = true;
                let bucket = match ext {
                    "btt" => &mut creation_trees,
                    "bto" => &mut creation_objects,
                    _ => &mut creation_terrain,
                };
                *bucket
                    .entry(world.to_string())
                    .or_default()
                    .entry(level.to_string())
                    .or_default() += 1;
            }
            if !l.contains("landscape\\lod\\") {
                matched += usize::from(is_lod);
                continue;
            }
            lod += 1;
            matched += 1;
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
            "{path}\n  landscape\\lod entries={lod}  _far.nif={far_nif}  distantlod={distantlod}  .high.={high_variant}  meshes\\terrain .btr/.bto/.btt={creation}  meshes\\lod\\generated={generated}  lodsettings={lodsettings}"
        );
        for (label, map) in [
            ("terrain", &terrain),
            ("blocks ", &blocks),
            ("btr    ", &creation_terrain),
            ("bto    ", &creation_objects),
            ("btt    ", &creation_trees),
        ] {
            for (world, levels) in map {
                let n: usize = levels.values().sum();
                println!("    {label} {world}: {n}  {levels:?}");
            }
        }
        total += matched;
    }
    println!("TOTAL LOD-family entries across archives: {total}");
}
