//! #5222 — the Fallout-legacy family's authored-LOD-quad index.
//!
//! FO3/FNV author their distant object and terrain LOD as per-worldspace
//! quad files named by level and SW-corner cell
//! (`meshes\landscape\lod\<ws>\blocks\<ws>.level<L>.x<X>.y<Y>.nif` for
//! objects, `textures\landscape\lod\<ws>\diffuse\<ws>.n.level<L>.x<X>.y<Y>.dds`
//! for terrain). #2586 fixed the *combined*-LOD games (Skyrim/FO4) by
//! deriving each worldspace's quad-lattice origin from WRLD NAM0 — but the
//! Fallout legacy family's lattices are not derivable that way: the audit
//! census (issue #5222) measured 26 of 29 worldspace/level sets off the
//! `(0,0)` grid, no single origin explains them (across the corpus `(0,0)`
//! covers 272 of 697 object quads, NAM0 128, the XCLC minimum 104), and
//! `washmontop`'s level-8 quads alone sit on **three** different y residues.
//! Every `(0,0)`-anchored probe therefore missed, cached an empty sentinel,
//! and no distant geometry or authored terrain colour drew.
//!
//! The fix inverts the question: instead of enumerating a guessed lattice
//! and asking the archive "is this quad there?", scan the archive name
//! tables **once** per streaming state and enumerate the quads that are
//! actually authored. Selection then works from footprints
//! ([`super::lod_bands::select_authored_lod_quads`]), so residues — one or
//! many per worldspace/level — stop mattering.
//!
//! Oblivion's FormID-keyed 32-cell quad family is deliberately NOT indexed
//! here: its quad grid is absolute and its lookup already resolves
//! (`env_translate::translate_terrain_lod_textures`'s `OblivionLegacy` arm).

use std::collections::{BTreeSet, HashMap};

use crate::asset_provider::TextureProvider;

/// One family's authored quads for one worldspace, keyed `(level, qx, qy)`.
pub(crate) type LegacyQuadSet = BTreeSet<(i32, i32, i32)>;

/// Which authored family a scanned quad name belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyLodFamily {
    /// `meshes\landscape\lod\<ws>\blocks\` object quads (`.nif`).
    Objects,
    /// `textures\landscape\lod\<ws>\diffuse\` terrain colour quads (`.dds`).
    TerrainDiffuse,
}

/// Parse one archive file name into its legacy quad identity, or `None`
/// when the name is not a plain legacy quad file.
///
/// Accepts either separator and any case (archive tables are mixed-case;
/// BA2s use `/`, BSAs `\`). The `.high.` detail-variant siblings (#4468:
/// FO3 Anchorage 60, FNV Lonesome Road 19, each with a plain sibling —
/// set-difference 0) are deliberately NOT parsed: the plain form is the
/// quad's availability, and `object_lod_archive_path` builds only the
/// plain form. The terrain normals\ folder is skipped for the same reason —
/// diffuse presence is the family's availability signal, exactly as
/// `translate_terrain_lod_textures`'s FalloutLegacy arm probes it.
pub(crate) fn parse_legacy_lod_quad_name(path: &str) -> Option<(LegacyLodFamily, String, i32, i32, i32)> {
    let p = path.to_ascii_lowercase().replace('/', "\\");
    if let Some(rest) = p.strip_prefix("meshes\\landscape\\lod\\") {
        let (world, tail) = rest.split_once("\\blocks\\")?;
        let file = tail.rsplit('\\').next()?;
        let stem = file.strip_suffix(".nif")?;
        return parse_level_x_y_stem(stem, world, LegacyLodFamily::Objects);
    }
    if let Some(rest) = p.strip_prefix("textures\\landscape\\lod\\") {
        let (world, tail) = rest.split_once("\\diffuse\\")?;
        let file = tail.rsplit('\\').next()?;
        let stem = file.strip_suffix(".dds")?;
        return parse_level_x_y_stem(stem, world, LegacyLodFamily::TerrainDiffuse);
    }
    None
}

/// Shared tail of both name shapes: the stem must repeat the worldspace
/// key, then (for terrain) an `.n` marker, then `level<L>.x<X>.y<Y>` with
/// nothing after — a `.high.` variant or an unrelated file fails the
/// trailing-fields check and yields `None`.
fn parse_level_x_y_stem(
    stem: &str,
    world: &str,
    family: LegacyLodFamily,
) -> Option<(LegacyLodFamily, String, i32, i32, i32)> {
    let after_world = stem.strip_prefix(world)?;
    let after = after_world
        .strip_prefix(".n.level")
        .or_else(|| after_world.strip_prefix(".level"))?;
    let mut fields = after.split('.');
    let level = fields.next()?.parse::<i32>().ok()?;
    let qx = fields.next()?.strip_prefix('x')?.parse::<i32>().ok()?;
    let qy = fields.next()?.strip_prefix('y')?.parse::<i32>().ok()?;
    if fields.next().is_some() {
        return None;
    }
    if !matches!(level, 4 | 8 | 16 | 32) {
        return None;
    }
    Some((family, world.to_string(), level, qx, qy))
}

/// The authored-quad index for the whole open archive set, scanned once per
/// `WorldStreamingState` (the opened archives do not change under it).
#[derive(Debug, Default, Clone)]
pub(crate) struct LegacyLodQuadIndex {
    /// Object quads per lowercased worldspace key.
    pub(crate) objects: HashMap<String, LegacyQuadSet>,
    /// Terrain colour quads per lowercased worldspace key.
    pub(crate) terrain: HashMap<String, LegacyQuadSet>,
}

impl LegacyLodQuadIndex {
    /// Scan every mesh- and texture-archive name table. Cold by design —
    /// one pass per worldspace streaming state, not per reconcile.
    pub(crate) fn scan(tex_provider: &TextureProvider) -> Self {
        let mut index = Self::default();
        for name in tex_provider.all_archive_names() {
            if let Some((family, worldspace, level, qx, qy)) =
                parse_legacy_lod_quad_name(name)
            {
                let bucket = match family {
                    LegacyLodFamily::Objects => &mut index.objects,
                    LegacyLodFamily::TerrainDiffuse => &mut index.terrain,
                };
                bucket
                    .entry(worldspace)
                    .or_default()
                    .insert((level, qx, qy));
            }
        }
        index
    }

    /// Authored object quads for `worldspace_key` (already lowercase), empty
    /// when the family authored none.
    pub(crate) fn objects_for(&self, worldspace_key: &str) -> Vec<(i32, i32, i32)> {
        self.objects
            .get(worldspace_key)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Authored terrain colour quads for `worldspace_key`, empty when none.
    pub(crate) fn terrain_for(&self, worldspace_key: &str) -> Vec<(i32, i32, i32)> {
        self.terrain
            .get(worldspace_key)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two vanilla name shapes round-trip into family + identity.
    /// Negative coordinates (Skyrim-style grids appear on FNV `road*`
    /// worldspaces) must parse — `-` is a legal integer prefix.
    #[test]
    fn plain_quad_names_parse() {
        assert_eq!(
            parse_legacy_lod_quad_name(
                "meshes\\landscape\\lod\\dcworld03\\blocks\\dcworld03.level4.x0.y2.nif"
            ),
            Some((
                LegacyLodFamily::Objects,
                "dcworld03".to_string(),
                4,
                0,
                2
            ))
        );
        assert_eq!(
            parse_legacy_lod_quad_name(
                "Textures/Landscape/LOD/washmontop/Diffuse/washmontop.n.level8.x4.y7.dds"
            ),
            Some((
                LegacyLodFamily::TerrainDiffuse,
                "washmontop".to_string(),
                8,
                4,
                7
            ))
        );
        assert_eq!(
            parse_legacy_lod_quad_name(
                "meshes\\landscape\\lod\\nuke\\blocks\\nuke.level8.x-4.y-12.nif"
            ),
            Some((LegacyLodFamily::Objects, "nuke".to_string(), 8, -4, -12))
        );
    }

    /// Everything that is not a plain quad is rejected: the `.high.`
    /// variant (availability rides the plain sibling), the terrain
    /// `normals\` folder (diffuse is the availability signal), non-quad
    /// files in the folders, and non-ladder levels.
    #[test]
    fn non_plain_names_are_rejected() {
        for name in [
            // #4468 variant: extra trailing field.
            "meshes\\landscape\\lod\\dlc01\\blocks\\dlc01.level4.x1.y1.high.nif",
            "meshes\\landscape\\lod\\dlc01\\blocks\\dlc01.level4.x1.y1.high",
            // Wrong folder for the family.
            "meshes\\landscape\\lod\\dcworld03\\other\\dcworld03.level4.x0.y2.nif",
            "textures\\landscape\\lod\\wasteland\\normals\\wasteland.n.level4.x0.y0.dds",
            // Not a quad name at all.
            "meshes\\landscape\\lod\\wasteland\\blocks\\readme.txt",
            "meshes\\clutter\\cup.nif",
            // Level outside the ladder.
            "meshes\\landscape\\lod\\ws\\blocks\\ws.level2.x0.y0.nif",
            // Stem does not repeat the worldspace key.
            "meshes\\landscape\\lod\\ws\\blocks\\other.level4.x0.y0.nif",
        ] {
            assert_eq!(
                parse_legacy_lod_quad_name(name),
                None,
                "{name} must not parse as a legacy quad"
            );
        }
    }
}
