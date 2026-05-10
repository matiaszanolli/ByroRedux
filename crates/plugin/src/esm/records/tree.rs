//! TREE — Pre-Skyrim tree base record.
//!
//! Oblivion / Fallout 3 / Fallout New Vegas use TREE records to attach
//! a SpeedTree binary (`.spt`) to a placeable form. Every TREE REFR in a
//! cell points back to a TREE base whose MODL field is the `.spt` path.
//! Skyrim and later dropped `.spt` and bake tree geometry into NIFs
//! rooted at `BSTreeNode`; on Skyrim+, TREE records still exist but their
//! MODL points at a regular NIF (no SpeedTree binary involved).
//!
//! **Sub-record layout** (cross-referenced against OpenMW's
//! `components/esm4/loadtree.cpp` and the FO3 / FNV / Oblivion .esm
//! corpora — fields OpenMW skips are captured here as raw byte arrays
//! so downstream consumers can inspect them without re-walking the
//! sub-record list):
//!
//! - `EDID` — editor ID (z-string).
//! - `OBND` — object bounds (six i16, [−x, −y, −z, +x, +y, +z]).
//! - `MODL` — model path (z-string). Pre-Skyrim points at a `.spt`;
//!   Skyrim+ at a `.nif`.
//! - `MODT` / `MODC` / `MODS` / `MODF` — model-data hashes (skipped).
//! - `ICON` — leaf billboard texture path (z-string).
//! - `MODB` — bound radius (f32).
//! - `SNAM` — array of u32 leaf indices flagging which leaves animate
//!   under wind. Empty / absent on Skyrim+ TREE records.
//! - `CNAM` — canopy shadow / wind parameters as a contiguous f32 array.
//!   Field count varies per game (5 floats Oblivion, 8 floats FO3/FNV);
//!   semantics not pinned down here — we surface the raw values for the
//!   future SpeedTree runtime to interpret.
//! - `BNAM` — billboard width/height (two f32) on FO3/FNV; absent on
//!   Skyrim+. Captured as `(width, height)` when shaped like that, else
//!   left as `None`.
//! - `FULL` — display name (lstring on localised plugins, else z-string).
//! - `PFIG` — harvest base form (u32 form ID, ALCH or INGR usually).
//! - `PFPC` — per-season harvest probability (skipped — gameplay only).
//!
//! Pre-#TREE wiring this record was collapsed into the generic MODL-only
//! path at [`super::mod`]'s `parse_modl_group` arm alongside STAT / MSTT
//! / FURN / etc., so every field beyond MODL was discarded silently.
//! That's a problem for the FNV / FO3 / Oblivion compatibility path
//! because `.spt` files need ICON for the leaf texture and CNAM/BNAM for
//! wind tuning.

use super::common::{
    find_sub, read_f32_at, read_f32_sub, read_i16_at, read_lstring_or_zstring, read_string_sub,
    read_u32_sub,
};
use crate::esm::reader::SubRecord;

/// Object bounds from an OBND sub-record (6 × i16, [−x, −y, −z, +x, +y, +z]).
/// Captured raw — the cell loader converts to engine units when needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectBounds {
    pub min: [i16; 3],
    pub max: [i16; 3],
}

/// A parsed TREE base record. Every field defaults to its zero-value /
/// `None` when the corresponding sub-record is absent — the SpeedTree
/// importer falls back to a textured billboard placeholder when the
/// `.spt` data isn't recoverable.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TreeRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// MODL — `.spt` path on Oblivion / FO3 / FNV; a regular `.nif` path
    /// on Skyrim+. Empty when the author shipped a stub TREE without a
    /// model (rare; observed in some mod content but no vanilla data).
    pub model_path: String,
    /// ICON — leaf billboard texture. Always a `.dds` (or `.tga` on
    /// Oblivion). The SpeedTree runtime samples this for every animated
    /// leaf card; the placeholder fallback uses it as the only visible
    /// surface when `.spt` decoding stalls.
    pub leaf_texture: String,
    /// FULL — localised display name. On localised plugins (Skyrim+
    /// with the TES4 `Localized` flag set) this will be a `<lstring
    /// 0x…>` placeholder until the .strings loader lands; on
    /// non-localised plugins (every Oblivion / FO3 / FNV) it's the
    /// inline cstring.
    pub full_name: String,
    /// MODB — bound radius (object-space). Used by the cell loader's
    /// frustum culler for first-pass rejection.
    pub bound_radius: f32,
    /// OBND — object bounds. `None` on records without OBND (most
    /// pre-Oblivion content, plus rare authoring oversights).
    pub bounds: Option<ObjectBounds>,
    /// SNAM — leaf-index list. SpeedTree runtime walks this to know
    /// which leaf cards animate under wind versus stay rigid (vanilla
    /// canopy setups carry both kinds in a single tree).
    pub leaf_indices: Vec<u32>,
    /// CNAM — canopy / wind parameters as raw f32. Field count varies
    /// across games (5 on Oblivion, 8 on FO3/FNV); semantics aren't
    /// pinned down here. Phases 2/4 of the SpeedTree plan consume this
    /// once `WindField` is wired.
    pub canopy_params: Vec<f32>,
    /// BNAM — billboard width / height on FO3/FNV. `None` on Oblivion
    /// (BNAM absent there) and Skyrim+ (TREE records dropped the field).
    pub billboard_size: Option<(f32, f32)>,
    /// PFIG — harvest base form (ALCH on Oblivion, INGR on FO3/FNV).
    /// Empty / `None` on non-harvestable trees and on every Skyrim+
    /// TREE (harvest moved to ACTI activator records).
    pub harvest_form: Option<u32>,
}

impl TreeRecord {
    /// True when the model path ends in `.spt` (case-insensitive). Used
    /// by the cell loader to route TREE REFRs through the
    /// `crates/spt/` parser instead of the NIF loader.
    pub fn has_speedtree_binary(&self) -> bool {
        self.model_path
            .rsplit('.')
            .next()
            .map(|ext| ext.eq_ignore_ascii_case("spt"))
            .unwrap_or(false)
    }
}

/// Parse a TREE record from its sub-record list. Unknown sub-records
/// (MODT / MODC / MODS / MODF model-data hashes, PFPC harvest
/// probabilities) are ignored; any subrecord we don't recognise just
/// passes through silently rather than panicking — same convention as
/// every other parser in this module.
pub fn parse_tree(form_id: u32, subs: &[SubRecord]) -> TreeRecord {
    let editor_id = read_string_sub(subs, b"EDID").unwrap_or_default();
    let model_path = read_string_sub(subs, b"MODL").unwrap_or_default();
    let leaf_texture = read_string_sub(subs, b"ICON").unwrap_or_default();
    let full_name = find_sub(subs, b"FULL")
        .map(read_lstring_or_zstring)
        .unwrap_or_default();
    let bound_radius = read_f32_sub(subs, b"MODB").unwrap_or(0.0);
    let harvest_form = read_u32_sub(subs, b"PFIG");

    let bounds = find_sub(subs, b"OBND").and_then(|data| {
        if data.len() < 12 {
            return None;
        }
        Some(ObjectBounds {
            min: [
                read_i16_at(data, 0)?,
                read_i16_at(data, 2)?,
                read_i16_at(data, 4)?,
            ],
            max: [
                read_i16_at(data, 6)?,
                read_i16_at(data, 8)?,
                read_i16_at(data, 10)?,
            ],
        })
    });

    // SNAM is a packed u32 array. The slice length varies by tree
    // (more leaves → more indices). Read every full u32 we can; ignore
    // any trailing bytes that don't make a complete u32 (defensive
    // against mod-authored TREE records with corrupt SNAM payloads).
    let leaf_indices = find_sub(subs, b"SNAM")
        .map(|data| {
            data.chunks_exact(4)
                .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect()
        })
        .unwrap_or_default();

    // CNAM is 5 × f32 on Oblivion, 8 × f32 on FO3/FNV. Read every full
    // f32 — semantics deferred to the SpeedTree runtime.
    let canopy_params = find_sub(subs, b"CNAM")
        .map(|data| {
            (0..data.len() / 4)
                .map(|i| read_f32_at(data, i * 4).unwrap_or(0.0))
                .collect()
        })
        .unwrap_or_default();

    // BNAM is 2 × f32 (billboard width, billboard height) on FO3/FNV.
    // Oblivion ships TREE records without BNAM. Skyrim+ TREE records
    // also drop BNAM (no SpeedTree binary so no leaf billboards).
    let billboard_size = find_sub(subs, b"BNAM").and_then(|data| {
        if data.len() < 8 {
            return None;
        }
        Some((read_f32_at(data, 0)?, read_f32_at(data, 4)?))
    });

    TreeRecord {
        form_id,
        editor_id,
        model_path,
        leaf_texture,
        full_name,
        bound_radius,
        bounds,
        leaf_indices,
        canopy_params,
        billboard_size,
        harvest_form,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(typ: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *typ,
            data: data.to_vec(),
        }
    }

    fn obnd_bytes(min: [i16; 3], max: [i16; 3]) -> Vec<u8> {
        let mut v = Vec::with_capacity(12);
        for x in min.iter().chain(max.iter()) {
            v.extend_from_slice(&x.to_le_bytes());
        }
        v
    }

    fn snam_bytes(indices: &[u32]) -> Vec<u8> {
        let mut v = Vec::with_capacity(indices.len() * 4);
        for &i in indices {
            v.extend_from_slice(&i.to_le_bytes());
        }
        v
    }

    fn cnam_bytes(params: &[f32]) -> Vec<u8> {
        let mut v = Vec::with_capacity(params.len() * 4);
        for &p in params {
            v.extend_from_slice(&p.to_le_bytes());
        }
        v
    }

    /// Realistic FNV TREE record: every sub-record present, MODL points
    /// at a `.spt`, CNAM carries 8 floats. This is the modal vanilla
    /// shape across `FalloutNV.esm` (verified post-parse to be the
    /// dominant CNAM length).
    #[test]
    fn parse_fnv_full_record_round_trips_every_field() {
        let subs = vec![
            sub(b"EDID", b"TreeJoshua01\0"),
            sub(b"OBND", &obnd_bytes([-128, -128, 0], [128, 128, 512])),
            sub(b"MODL", b"meshes\\trees\\treejoshua01.spt\0"),
            sub(b"MODB", &1.5f32.to_le_bytes()),
            sub(b"ICON", b"textures\\trees\\joshua_leaf.dds\0"),
            sub(b"SNAM", &snam_bytes(&[0, 2, 5, 7])),
            sub(
                b"CNAM",
                &cnam_bytes(&[0.5, 1.0, 0.7, 2.5, 1.2, 0.3, 0.4, 1.0]),
            ),
            sub(b"BNAM", &cnam_bytes(&[64.0, 128.0])),
            sub(b"PFIG", &0x000A1234u32.to_le_bytes()),
            sub(b"FULL", b"Joshua Tree\0"),
        ];
        let tree = parse_tree(0x000DEAD0, &subs);

        assert_eq!(tree.form_id, 0x000DEAD0);
        assert_eq!(tree.editor_id, "TreeJoshua01");
        assert_eq!(tree.model_path, "meshes\\trees\\treejoshua01.spt");
        assert_eq!(tree.leaf_texture, "textures\\trees\\joshua_leaf.dds");
        assert_eq!(tree.full_name, "Joshua Tree");
        assert_eq!(tree.bound_radius, 1.5);
        assert_eq!(
            tree.bounds,
            Some(ObjectBounds {
                min: [-128, -128, 0],
                max: [128, 128, 512],
            })
        );
        assert_eq!(tree.leaf_indices, vec![0, 2, 5, 7]);
        assert_eq!(tree.canopy_params.len(), 8);
        assert_eq!(tree.billboard_size, Some((64.0, 128.0)));
        assert_eq!(tree.harvest_form, Some(0x000A1234));
        assert!(tree.has_speedtree_binary());
    }

    /// Oblivion TREE: CNAM is 5 floats (not 8), no BNAM, no PFIG on
    /// non-harvestable trees, MODL still points at a `.spt`. The parser
    /// must shape-tolerate the shorter CNAM rather than dropping the
    /// whole record.
    #[test]
    fn parse_oblivion_short_cnam_no_bnam_no_pfig() {
        let subs = vec![
            sub(b"EDID", b"TreePine01\0"),
            sub(b"MODL", b"trees\\pine01.spt\0"),
            sub(b"ICON", b"trees\\pine_leaf.tga\0"),
            sub(b"CNAM", &cnam_bytes(&[0.4, 0.9, 0.6, 1.8, 1.0])),
        ];
        let tree = parse_tree(0x00000042, &subs);
        assert_eq!(tree.editor_id, "TreePine01");
        assert!(tree.has_speedtree_binary());
        assert_eq!(tree.canopy_params.len(), 5, "Oblivion CNAM is 5 × f32");
        assert!(
            tree.billboard_size.is_none(),
            "Oblivion TREE records ship no BNAM"
        );
        assert!(tree.harvest_form.is_none());
        assert!(tree.bounds.is_none(), "Oblivion TREE often omits OBND");
    }

    /// Skyrim+ TREE points MODL at a regular `.nif` (BSTreeNode-rooted)
    /// rather than a `.spt`. `has_speedtree_binary` must report false so
    /// the cell loader stays on the NIF path.
    #[test]
    fn skyrim_nif_path_does_not_route_through_spt() {
        let subs = vec![
            sub(b"EDID", b"TreeAspen01\0"),
            sub(b"MODL", b"meshes\\landscape\\trees\\treeaspen01.nif\0"),
            sub(b"ICON", b"textures\\landscape\\trees\\treeaspen.dds\0"),
        ];
        let tree = parse_tree(0x000ABCDE, &subs);
        assert!(!tree.has_speedtree_binary());
        assert_eq!(tree.editor_id, "TreeAspen01");
        // Skyrim TREE drops SNAM/CNAM/BNAM.
        assert!(tree.leaf_indices.is_empty());
        assert!(tree.canopy_params.is_empty());
        assert!(tree.billboard_size.is_none());
    }

    /// Defensive parsing: a stub TREE record with only EDID still
    /// produces a `TreeRecord` rather than panicking. Every field
    /// defaults; downstream code can branch on `model_path.is_empty()`
    /// to skip placement.
    #[test]
    fn parse_minimal_record_yields_defaults() {
        let subs = vec![sub(b"EDID", b"TreeStub\0")];
        let tree = parse_tree(0x00000001, &subs);
        assert_eq!(tree.editor_id, "TreeStub");
        assert!(tree.model_path.is_empty());
        assert!(tree.leaf_texture.is_empty());
        assert_eq!(tree.bound_radius, 0.0);
        assert!(tree.bounds.is_none());
        assert!(tree.leaf_indices.is_empty());
        assert!(tree.canopy_params.is_empty());
        assert!(tree.billboard_size.is_none());
        assert!(tree.harvest_form.is_none());
        assert!(!tree.has_speedtree_binary());
    }

    /// Mod-authored TREE with a corrupt SNAM (5 bytes — one full u32 +
    /// a stray byte). The trailing byte must drop silently rather than
    /// taking the whole parser down. Same shape as the existing
    /// chunks_exact-based `leaf_indices` reader.
    #[test]
    fn corrupt_snam_truncated_chunk_drops_silently() {
        let mut snam = snam_bytes(&[0xCAFEBABE]);
        snam.push(0x42); // stray trailing byte
        let subs = vec![
            sub(b"EDID", b"TreeBroken\0"),
            sub(b"MODL", b"trees\\broken.spt\0"),
            sub(b"SNAM", &snam),
        ];
        let tree = parse_tree(0xDEADBEEF, &subs);
        assert_eq!(
            tree.leaf_indices,
            vec![0xCAFEBABE],
            "trailing partial u32 dropped, full leading u32 preserved"
        );
    }

    /// `has_speedtree_binary` is case-insensitive — Bethesda content
    /// ships paths in mixed casing. Pre-fix a `.SPT` path would have
    /// missed the SpeedTree route.
    #[test]
    fn has_speedtree_binary_is_case_insensitive() {
        let mut subs = vec![sub(b"MODL", b"trees\\foo.SPT\0")];
        let tree = parse_tree(0, &subs);
        assert!(tree.has_speedtree_binary());

        // Sanity: a non-spt extension stays on the NIF route regardless
        // of case.
        subs[0] = sub(b"MODL", b"trees\\foo.NIF\0");
        let nif = parse_tree(0, &subs);
        assert!(!nif.has_speedtree_binary());
    }
}
