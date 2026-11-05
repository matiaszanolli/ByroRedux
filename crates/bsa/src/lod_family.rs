//! Distant-LOD file families, recognised by archive path (#6636, DLOD-GATE-007).
//!
//! One classifier for the census example (`examples/probe_lod_corpus.rs`) and
//! the checked-in `lod_family_census` test (`crates/plugin/tests/lod_family_census.rs`), so
//! the two cannot disagree about which file belongs to which family. Names are
//! the engine's own archive spellings, lowercased and with `/` read as `\`
//! (BA2 tables use `/`).
//!
//! The families are the PRD `distant-lod-completion` §3.1 inventory (F1–F11),
//! split by what the *path* can tell: a path never says whether a `.lod`
//! group holds STAT or TREE placements (that needs the ESM), so the Oblivion
//! family here is the `.lod` file, not F6 / F7.

/// One LOD file family. See the module doc for the path rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LodFamily {
    /// `landscape\lod\<ws>\…` terrain quad (F2): FO3/FNV `.dds` / `.nif` per level.
    LegacyTerrain,
    /// `landscape\lod\<ws>\blocks\…` combined object quads (F5).
    LegacyBlocks,
    /// `meshes\terrain\<ws>\<ws>.<L>.<x>.<y>.btr` (F1).
    CreationTerrainBtr,
    /// `meshes\terrain\<ws>\objects\….bto` (F4).
    CreationObjectsBto,
    /// `meshes\terrain\<ws>\trees\….btt` (F8).
    CreationTreesBtt,
    /// `<name>_far.nif` (the Oblivion per-base far mesh, F6).
    FarNif,
    /// `distantlod\<ws>_<x>_<y>.lod` placement groups (F6 + F7).
    OblivionLod,
    /// `meshes\lod\generated\….nif` (Starfield).
    GeneratedLod,
    /// `lodsettings\<ws>.lod` (F10).
    LodSettingsLod,
    /// `lodsettings\<ws>.dlodsettings` (F11).
    DlodSettings,
    /// `meshes\terrain\<ws>\trees\<ws>.lst` Creation tree species list: Skyrim beside its `.btt`; FO4 ships two
    /// (`diamondcity`, `sanctuaryhillsworld`) with no `.btt`.
    CreationTreeList,
    /// `textures\terrain\<ws>\trees\…treelod….dds` Skyrim billboard atlas.
    TreeLodAtlas,
    /// `meshes\landscape\lod\<ws>\trees\<ws>.level<L>.x<X>.y<Y>.dtl` (F9).
    FalloutTreeDtl,
    /// `meshes\landscape\lod\<ws>\trees\treetypes.lst` (F9).
    FalloutTreeTypes,
}

impl LodFamily {
    /// A short stable label for census output.
    pub fn label(self) -> &'static str {
        match self {
            Self::LegacyTerrain => "legacy-terrain",
            Self::LegacyBlocks => "legacy-blocks",
            Self::CreationTerrainBtr => "btr",
            Self::CreationObjectsBto => "bto",
            Self::CreationTreesBtt => "btt",
            Self::FarNif => "far-nif",
            Self::OblivionLod => "oblivion-lod",
            Self::GeneratedLod => "generated",
            Self::LodSettingsLod => "lodsettings-lod",
            Self::DlodSettings => "dlodsettings",
            Self::CreationTreeList => "tree-lst",
            Self::TreeLodAtlas => "treelod-atlas",
            Self::FalloutTreeDtl => "dtl",
            Self::FalloutTreeTypes => "treetypes-lst",
        }
    }
}

/// One classified file: its family, worldspace (when the path names one), quad
/// level (when the file name carries one) and quad SW cell (the legacy
/// `….level<L>.x<X>.y<Y>.…` names and the Creation `<ws>.<L>.<X>.<Y>.…` names).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LodHit {
    pub family: LodFamily,
    pub world: Option<String>,
    pub level: Option<String>,
    pub cell: Option<(i32, i32)>,
}

/// The `x<X>.y<Y>` pair of a legacy quad name.
fn legacy_cell(rest: &str) -> Option<(i32, i32)> {
    let x = rest.split('.').find_map(|f| f.strip_prefix('x')?.parse().ok())?;
    let y = rest.split('.').find_map(|f| f.strip_prefix('y')?.parse().ok())?;
    Some((x, y))
}

/// Normalise an archive path to the lowercase backslash form [`classify`] reads.
pub fn normalize(path: &str) -> String {
    path.to_ascii_lowercase().replace('/', "\\")
}

/// `true` when the (already normalised) name is a `.high.` object-quad variant,
/// the second, higher-detail bake the Fallout DLC archives ship (#4468).
pub fn is_high_variant(normalized: &str) -> bool {
    normalized.contains(".high.")
}

/// The `level<L>` field of a legacy quad name (`wasteland.level8.x-24.y16.dtl`,
/// `….level4.x0.y0.dds`), searched from the right as the old probe did.
fn legacy_level(rest: &str) -> Option<&str> {
    rest.rsplit('.')
        .find(|s| s.starts_with("level"))
        .or_else(|| rest.split('.').find(|s| s.starts_with("level")))
}

/// Classify one archive path. A path can belong to more than one family only
/// where the families overlap by construction (none do today), so the result
/// is a list for forward compatibility; it is empty for a non-LOD file.
pub fn classify(path: &str) -> Vec<LodHit> {
    let l = normalize(path);
    let mut hits = Vec::new();
    let mut push = |family, world: Option<&str>, level: Option<&str>, cell: Option<(i32, i32)>| {
        hits.push(LodHit {
            family,
            world: world.map(str::to_owned),
            level: level.map(str::to_owned),
            cell,
        });
    };

    if l.ends_with("_far.nif") {
        push(LodFamily::FarNif, None, None, None);
    }
    if let Some(rest) = l.strip_prefix("distantlod\\") {
        if rest.ends_with(".lod") {
            // `<ws>_<x>_<y>.lod`: the worldspace is everything before the last two `_` fields.
            let stem = rest.trim_end_matches(".lod");
            let world = stem.rsplitn(3, '_').nth(2).unwrap_or(stem);
            push(LodFamily::OblivionLod, Some(world), None, None);
        }
    }
    if l.starts_with("meshes\\lod\\generated\\") && l.ends_with(".nif") {
        push(LodFamily::GeneratedLod, None, None, None);
    }
    if let Some(rest) = l.strip_prefix("lodsettings\\") {
        if let Some(world) = rest.strip_suffix(".dlodsettings") {
            push(LodFamily::DlodSettings, Some(world), None, None);
        } else if let Some(world) = rest.strip_suffix(".lod") {
            push(LodFamily::LodSettingsLod, Some(world), None, None);
        }
    }
    if l.starts_with("textures\\terrain\\") && l.contains("\\trees\\") && l.contains("treelod") {
        let world = l["textures\\terrain\\".len()..].split('\\').next();
        push(LodFamily::TreeLodAtlas, world, None, None);
    }
    if let Some(rest) = l.strip_prefix("meshes\\terrain\\") {
        let (world, file) = rest.split_once('\\').unwrap_or((rest, ""));
        if file.ends_with(".lst") {
            push(LodFamily::CreationTreeList, Some(world), None, None);
        } else if let Some((stem, ext)) = file.rsplit_once('.') {
            // `<ws>.<L>.<x>.<y>`: the level is the second dot field; anything with fewer
            // fields is not a quadtree quad name.
            let name = stem.rsplit('\\').next().unwrap_or(stem);
            if name.split('.').count() >= 4 {
                let level = name.split('.').nth(1);
                let cell = name
                    .split('.')
                    .nth(2)
                    .and_then(|x| x.parse().ok())
                    .zip(name.split('.').nth(3).and_then(|y| y.parse().ok()));
                match ext {
                    "btr" => push(LodFamily::CreationTerrainBtr, Some(world), level, cell),
                    "bto" => push(LodFamily::CreationObjectsBto, Some(world), level, cell),
                    "btt" => push(LodFamily::CreationTreesBtt, Some(world), level, cell),
                    _ => {}
                }
            }
        }
    }
    if let Some(idx) = l.find("landscape\\lod\\") {
        let rest = &l[idx + "landscape\\lod\\".len()..];
        let world = rest.split('\\').next();
        let level = legacy_level(rest);
        let cell = legacy_cell(rest);
        if l.starts_with("meshes\\") && rest.contains("\\trees\\") {
            if rest.ends_with("\\treetypes.lst") {
                push(LodFamily::FalloutTreeTypes, world, None, None);
            } else if rest.ends_with(".dtl") {
                push(LodFamily::FalloutTreeDtl, world, level, cell);
            }
        } else if rest.contains("\\blocks\\") {
            push(LodFamily::LegacyBlocks, world, level, cell);
        } else {
            push(LodFamily::LegacyTerrain, world, level, cell);
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(path: &str) -> LodHit {
        let hits = classify(path);
        assert_eq!(hits.len(), 1, "{path}: {hits:?}");
        hits.into_iter().next().unwrap()
    }

    #[test]
    fn every_family_is_recognised_by_its_path() {
        let cases: &[(&str, LodFamily, Option<&str>, Option<&str>)] = &[
            ("meshes\\terrain\\tamriel\\tamriel.4.0.0.btr", LodFamily::CreationTerrainBtr, Some("tamriel"), Some("4")),
            ("Meshes/Terrain/Commonwealth/Objects/Commonwealth.8.-8.0.bto", LodFamily::CreationObjectsBto, Some("commonwealth"), Some("8")),
            ("meshes\\terrain\\tamriel\\trees\\tamriel.4.8.8.btt", LodFamily::CreationTreesBtt, Some("tamriel"), Some("4")),
            ("meshes\\terrain\\tamriel\\trees\\tamriel.lst", LodFamily::CreationTreeList, Some("tamriel"), None),
            ("textures\\terrain\\tamriel\\trees\\tamrieltreelod.dds", LodFamily::TreeLodAtlas, Some("tamriel"), None),
            ("meshes\\landscape\\lod\\wasteland\\blocks\\wasteland.level8.x0.y0.nif", LodFamily::LegacyBlocks, Some("wasteland"), Some("level8")),
            ("textures\\landscape\\lod\\wasteland\\wasteland.level4.x0.y0.dds", LodFamily::LegacyTerrain, Some("wasteland"), Some("level4")),
            ("meshes\\landscape\\lod\\wasteland\\trees\\wasteland.level8.x-24.y16.dtl", LodFamily::FalloutTreeDtl, Some("wasteland"), Some("level8")),
            ("meshes\\landscape\\lod\\wasteland\\trees\\treetypes.lst", LodFamily::FalloutTreeTypes, Some("wasteland"), None),
            ("lodsettings\\wasteland.dlodsettings", LodFamily::DlodSettings, Some("wasteland"), None),
            ("lodsettings\\tamriel.lod", LodFamily::LodSettingsLod, Some("tamriel"), None),
            ("distantlod\\tamriel_28_-27.lod", LodFamily::OblivionLod, Some("tamriel"), None),
            ("distantlod\\cheydinhalworld_1_2.lod", LodFamily::OblivionLod, Some("cheydinhalworld"), None),
            ("meshes\\architecture\\anvil\\anvilhouse01_far.nif", LodFamily::FarNif, None, None),
            ("meshes\\lod\\generated\\a.nif", LodFamily::GeneratedLod, None, None),
        ];
        for (path, family, world, level) in cases {
            let hit = one(path);
            assert_eq!(hit.family, *family, "{path}");
            assert_eq!(hit.world.as_deref(), *world, "{path}");
            assert_eq!(hit.level.as_deref(), *level, "{path}");
        }
    }

    #[test]
    fn quad_names_carry_their_sw_cell() {
        assert_eq!(
            one("meshes\\landscape\\lod\\wasteland\\blocks\\wasteland.level8.x-24.y16.nif").cell,
            Some((-24, 16))
        );
        assert_eq!(one("meshes\\terrain\\tamriel\\tamriel.4.-8.12.btr").cell, Some((-8, 12)));
        assert_eq!(one("distantlod\\tamriel_28_-27.lod").cell, None);
    }

    #[test]
    fn ordinary_files_are_not_lod() {
        for path in [
            "meshes\\clutter\\bottle01.nif",
            "meshes\\terrain\\tamriel\\readme.txt",
            "meshes\\terrain\\tamriel\\notaquad.btr",
            "textures\\landscape\\rock01.dds",
        ] {
            assert!(classify(path).is_empty(), "{path}");
        }
    }

    #[test]
    fn the_dtl_and_treetypes_files_are_trees_not_terrain_quads() {
        // `landscape\lod\…\trees\` must not fall through to the terrain bucket.
        assert!(classify("meshes\\landscape\\lod\\wasteland\\trees\\x.dtl")
            .iter()
            .all(|h| h.family == LodFamily::FalloutTreeDtl));
    }
}
