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
//! #6636 — the family matching lives in `byroredux_bsa::lod_family` (shared with the
//! `lod_family_census` test), and now also names the Fallout `.dtl` / `treetypes.lst`
//! distant-tree files and the FO3/FNV `.dlodsettings` descriptors.
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
    use byroredux_bsa::lod_family::{classify, is_high_variant, normalize, LodFamily};
    use std::collections::BTreeMap;
    let mut total = 0usize;
    for path in std::env::args().skip(1) {
        let archive = match AnyArchive::open(&path) {
            Ok(archive) => archive,
            Err(e) => {
                eprintln!("skip {path} ({e})");
                continue;
            }
        };
        let mut per_family: BTreeMap<LodFamily, usize> = BTreeMap::new();
        let mut per_world: BTreeMap<(LodFamily, String), BTreeMap<String, usize>> = BTreeMap::new();
        let mut high_variant = 0usize;
        let mut matched = 0usize;
        for f in archive.list_files() {
            let l = normalize(f);
            if is_high_variant(&l) {
                high_variant += 1;
            }
            let hits = classify(&l);
            // One hit per file for the total, however many families claim it.
            matched += usize::from(!hits.is_empty());
            for hit in hits {
                *per_family.entry(hit.family).or_default() += 1;
                *per_world
                    .entry((hit.family, hit.world.unwrap_or_else(|| "-".to_owned())))
                    .or_default()
                    .entry(hit.level.unwrap_or_else(|| "-".to_owned()))
                    .or_default() += 1;
            }
        }
        let summary: Vec<String> = per_family
            .iter()
            .map(|(family, n)| format!("{}={n}", family.label()))
            .collect();
        println!("{path}\n  {}  .high.={high_variant}", summary.join("  "));
        for ((family, world), levels) in &per_world {
            let n: usize = levels.values().sum();
            println!("    {:<14} {world}: {n}  {levels:?}", family.label());
        }
        total += matched;
    }
    println!("TOTAL LOD-family entries across archives: {total}");
}
