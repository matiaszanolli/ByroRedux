//! `BSShaderTextureSet` slot → canonical texture role, in one place.
//!
//! #2695 (NIFAL-D8-04) — this table used to exist twice. The NIF importer
//! resolved slots per `BSLightingShaderType`, while the REFR texture overlay
//! (`byroredux::cell_loader::refr`) resolved the *same* NIF slot indices
//! through a fixed shader-type-agnostic table. They already disagreed on four
//! slots, so an `XTXR` swap on a FaceTint / SkinTint / MultiLayerParallax
//! placement landed in a different canonical role than the identical slot read
//! from the mesh's own texture set — an override changed shading *semantics*,
//! not just the texture. Worse, a fix to one table silently failed to reach the
//! other.
//!
//! Both sites now call [`slot_to_role`]. The overlay recovers `shader_type`
//! from the cached import ([`crate::import::ImportedMaterial::shader_type`]),
//! which is why that field exists.
//!
//! ## The disagreements this resolves
//!
//! | slot | importer (was) | overlay (was) | now |
//! |---|---|---|---|
//! | 2, types 4/5/6 | tint | emissive | **tint** |
//! | 3, type 4 | detail | height | **detail** |
//! | 4/5, types 5/6 | skipped | env / env-mask | **skipped** |
//! | 7, type 11 | parked | specular | **parked** |
//!
//! The importer's reading wins in every case: each arm is backed by measured
//! occupancy across vanilla `Skyrim - Meshes0.bsa` (see the per-arm notes
//! below and #2693 / #2694 / #2742), whereas the overlay's flat table was
//! never evidence-driven — it mirrored `TextureSet`'s field order.

/// Canonical destination for a `BSShaderTextureSet` slot.
///
/// Deliberately a subset of `MaterialTextureSet`'s roles: only the ones a NIF
/// texture-set slot can actually name. Roles fed from BGSM/BGEM or synthesized
/// elsewhere (`dark`, `flow`, `decals`, …) are not reachable from here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureRole {
    BaseColor,
    Normal,
    /// Glow / emissive map.
    Emissive,
    /// Skin- or hair-tint mask (`*_sk.dds`).
    Tint,
    /// Complexion / surface detail map. NOT a parallax height field.
    Detail,
    /// Parallax / POM height field.
    Height,
    Environment,
    EnvironmentMask,
    /// MultiLayerParallax inner layer (subsurface).
    InnerLayer,
    /// Standalone specular intensity/colour, on model-space-normal materials.
    Specular,
}

/// `BSLightingShaderType` values this table branches on. Named so the arms
/// read as intent rather than as magic numbers.
pub mod bs_lighting {
    /// Face Tint — vanilla Skyrim head meshes.
    pub const FACE_TINT: u32 = 4;
    /// Skin Tint.
    pub const SKIN_TINT: u32 = 5;
    /// Hair Tint.
    pub const HAIR_TINT: u32 = 6;
    /// Multi-Layer Parallax.
    pub const MULTI_LAYER_PARALLAX: u32 = 11;
}

/// Resolve one `BSShaderTextureSet` slot to its canonical role.
///
/// `None` means the slot has **no** canonical destination for this shader
/// type, and the caller must drop it rather than guess. Three distinct reasons
/// produce `None`, all deliberate:
///
/// * **Not authored.** Types 5/6 declare no slot 4/5 (their tint is a colour
///   field, not a texture). Vanilla leaves them empty, but a mis-exported NIF
///   with a stray slot-4 string would otherwise bind a spurious env cube
///   (#1350).
/// * **No canonical role exists yet.** Slot 7 on type 11 is a back-lighting
///   map; `MaterialTextureSet` has no back-lighting role and no shader consumes
///   one, so inventing a mapping would be fabrication.
/// * **Owned by another subsystem.** Slot 6 on FaceTint is the per-NPC baked
///   FaceGen tint. Routing it to `BaseColor` here would silently pre-empt the
///   FaceGen path (#2095), which overrides diffuse from the *actor's* form id.
///
/// `model_space_normals` gates slot 7 only: with MSN the slot is the alternate
/// specular intensity/colour texture, which is independent of shader type
/// (#2742 — 390/390 slot-7-bearing SkinTint properties in `Skyrim - Meshes0.bsa`
/// are MSN). Without it there is no role.
pub fn slot_to_role(shader_type: u32, slot: u32, model_space_normals: bool) -> Option<TextureRole> {
    match slot {
        0 => Some(TextureRole::BaseColor),
        1 => Some(TextureRole::Normal),

        // Slot 2 is polymorphic. The tint family puts its skin/hair mask here;
        // everything else puts a glow map.
        //
        // #2694 — FaceTint (4) belongs with SkinTint (5): every vanilla
        // FaceTint property carries an `*_sk.dds` here (3158/3158 in
        // `Skyrim - Meshes0.bsa`). It was previously excluded, so every vanilla
        // head bound its skin-tint mask as a GLOW map — latent only because
        // `emissive_color` is black, one authored value away from glowing
        // faces. HairTint (6) is included on the same evidence (`_sk` on all 16
        // of the 10 815 HairTint properties that populate the slot).
        2 => match shader_type {
            bs_lighting::FACE_TINT | bs_lighting::SKIN_TINT | bs_lighting::HAIR_TINT => {
                Some(TextureRole::Tint)
            }
            _ => Some(TextureRole::Emissive),
        },

        // #2694 — nif.xml calls slot 3 "Height/Parallax" generically, but every
        // vanilla FaceTint puts a *detail* map there
        // (`MaleHeadDetail_Rough01.dds`, 3149/3158). Feeding that to the height
        // role made `triangle.frag` ray-march POM over a face complexion map —
        // its POM branch gates only on `parallaxMapIndex != 0u`, with no
        // material-kind check.
        3 => match shader_type {
            bs_lighting::FACE_TINT => Some(TextureRole::Detail),
            _ => Some(TextureRole::Height),
        },

        // #1350 — types 5/6 declare no TS slot 4/5; skip explicitly so a stray
        // authored string cannot bind an env cube. FaceTint's slots 4/5 are
        // absent on 100% of vanilla properties.
        4 => match shader_type {
            bs_lighting::SKIN_TINT | bs_lighting::HAIR_TINT | bs_lighting::FACE_TINT => None,
            _ => Some(TextureRole::Environment),
        },
        5 => match shader_type {
            bs_lighting::SKIN_TINT | bs_lighting::HAIR_TINT | bs_lighting::FACE_TINT => None,
            _ => Some(TextureRole::EnvironmentMask),
        },

        // #2693 — the inner layer is slot **6**, not 7. nif.xml contradicts
        // itself (its enum prose says "Layer(TS7)", its field table says slot 6
        // = "Subsurface for Multilayer Parallax"); the field table wins on
        // shipped data — slot 6 is non-empty on 607/607 type-11 properties in
        // `Skyrim - Meshes0.bsa` while slot 7 holds tint maps on 370.
        //
        // FaceTint slot 6 is the baked FaceGen tint — see this fn's doc for why
        // it is deliberately unrouted.
        6 => match shader_type {
            bs_lighting::MULTI_LAYER_PARALLAX => Some(TextureRole::InnerLayer),
            _ => None,
        },

        // Slot 7 is the alternate specular on model-space-normal materials,
        // independent of shader type (#2742) — except on type 11, where it is a
        // back-lighting map with no canonical role.
        7 => match (shader_type, model_space_normals) {
            (bs_lighting::MULTI_LAYER_PARALLAX, _) => None,
            (_, true) => Some(TextureRole::Specular),
            (_, false) => None,
        },

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::bs_lighting;
    use super::*;

    /// The four slots where the importer and the REFR overlay disagreed
    /// (#2695). Each asserts the *importer's* evidence-backed reading, which
    /// is the one that wins.
    #[test]
    fn resolves_the_four_table_disagreements_the_importer_way() {
        // slot 2 on the tint family → Tint, not Emissive.
        for ty in [
            bs_lighting::FACE_TINT,
            bs_lighting::SKIN_TINT,
            bs_lighting::HAIR_TINT,
        ] {
            assert_eq!(
                slot_to_role(ty, 2, false),
                Some(TextureRole::Tint),
                "shader_type {ty} slot 2 must be the skin/hair tint mask"
            );
        }
        // slot 3 on FaceTint → Detail, not Height (POM over a face otherwise).
        assert_eq!(
            slot_to_role(bs_lighting::FACE_TINT, 3, false),
            Some(TextureRole::Detail)
        );
        // slots 4/5 on the tint family → nothing, not env/env-mask.
        for ty in [
            bs_lighting::FACE_TINT,
            bs_lighting::SKIN_TINT,
            bs_lighting::HAIR_TINT,
        ] {
            assert_eq!(slot_to_role(ty, 4, false), None);
            assert_eq!(slot_to_role(ty, 5, false), None);
        }
        // slot 7 on MultiLayerParallax → nothing (back lighting), even with MSN.
        assert_eq!(
            slot_to_role(bs_lighting::MULTI_LAYER_PARALLAX, 7, true),
            None
        );
        assert_eq!(
            slot_to_role(bs_lighting::MULTI_LAYER_PARALLAX, 7, false),
            None
        );
    }

    /// The ordinary path is unchanged — this is what the overwhelming majority
    /// of content takes, and the fix must not disturb it.
    #[test]
    fn default_shader_type_keeps_the_conventional_mapping() {
        let ty = 0; // Default
        assert_eq!(slot_to_role(ty, 0, false), Some(TextureRole::BaseColor));
        assert_eq!(slot_to_role(ty, 1, false), Some(TextureRole::Normal));
        assert_eq!(slot_to_role(ty, 2, false), Some(TextureRole::Emissive));
        assert_eq!(slot_to_role(ty, 3, false), Some(TextureRole::Height));
        assert_eq!(slot_to_role(ty, 4, false), Some(TextureRole::Environment));
        assert_eq!(
            slot_to_role(ty, 5, false),
            Some(TextureRole::EnvironmentMask)
        );
        assert_eq!(slot_to_role(ty, 6, false), None);
    }

    /// #2693 — inner layer is slot 6 on type 11, and slot 6 is inert elsewhere.
    #[test]
    fn multi_layer_parallax_inner_layer_is_slot_six() {
        assert_eq!(
            slot_to_role(bs_lighting::MULTI_LAYER_PARALLAX, 6, false),
            Some(TextureRole::InnerLayer)
        );
        assert_eq!(slot_to_role(0, 6, false), None);
        // Type 11 still takes env/env-mask at 4/5.
        assert_eq!(
            slot_to_role(bs_lighting::MULTI_LAYER_PARALLAX, 4, false),
            Some(TextureRole::Environment)
        );
        assert_eq!(
            slot_to_role(bs_lighting::MULTI_LAYER_PARALLAX, 5, false),
            Some(TextureRole::EnvironmentMask)
        );
    }

    /// #2742 — slot 7 is specular only under model-space normals, and that is
    /// shader-type independent (SkinTint included, which #1350 had dropped).
    #[test]
    fn slot_seven_is_specular_only_under_model_space_normals() {
        assert_eq!(
            slot_to_role(bs_lighting::SKIN_TINT, 7, true),
            Some(TextureRole::Specular)
        );
        assert_eq!(slot_to_role(bs_lighting::SKIN_TINT, 7, false), None);
        assert_eq!(slot_to_role(0, 7, true), Some(TextureRole::Specular));
        assert_eq!(slot_to_role(0, 7, false), None);
    }

    /// Out-of-range slots resolve to `None` rather than panicking — the REFR
    /// overlay feeds this a raw `XTXR` slot index straight from the ESM.
    #[test]
    fn out_of_range_slots_are_none() {
        for slot in [8u32, 9, 64, u32::MAX] {
            assert_eq!(slot_to_role(0, slot, true), None);
        }
    }

    /// No slot may resolve to the same role as another for one shader type —
    /// two slots writing one role means the later silently clobbers the
    /// earlier, which is the class of bug this table exists to prevent.
    #[test]
    fn roles_are_unique_per_shader_type() {
        for ty in [
            0u32,
            bs_lighting::FACE_TINT,
            bs_lighting::SKIN_TINT,
            bs_lighting::HAIR_TINT,
            bs_lighting::MULTI_LAYER_PARALLAX,
            16,
        ] {
            for msn in [false, true] {
                let mut seen = Vec::new();
                for slot in 0..8u32 {
                    if let Some(role) = slot_to_role(ty, slot, msn) {
                        assert!(
                            !seen.contains(&role),
                            "shader_type {ty} (msn={msn}) maps two slots to {role:?}"
                        );
                        seen.push(role);
                    }
                }
            }
        }
    }
}
