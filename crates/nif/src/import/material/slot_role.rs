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
//! ## Starfield / FO76 scope
//!
//! Starfield and FO76 `BSGeometry` materials deliberately do not enter this
//! table: their authored texture roles come from the BGSM/BGEM material
//! records (and Starfield's materialsbeta CDB), not a Skyrim-family
//! `BSShaderTextureSet`. A zero Starfield hit here is therefore an explicit
//! format boundary, not an unmeasured routing gap.
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

use std::sync::atomic::{AtomicU64, Ordering};

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
    /// FO4/FO76 greyscale-to-palette gradient lookup.
    GreyscaleLut,
    Environment,
    EnvironmentMask,
    /// MultiLayerParallax inner layer (subsurface).
    InnerLayer,
    /// Standalone specular intensity/colour, on model-space-normal materials.
    Specular,
    /// Skyrim soft/rim-light mask from texture-set slot 2.
    LightingMask,
    /// Skyrim back-light map from texture-set slot 7.
    BackLighting,
    /// FO4 wrinkle/expression-crease normal map, tint-family shader types
    /// only. See #2999.
    Wrinkle,
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
    /// Eye environment map.
    pub const EYE_ENVMAP: u32 = 16;
}

/// Per-game `BSShaderTextureSet` slot vocabulary.
///
/// This is kept beside the table rather than inferred from shader-type values:
/// FO76 reuses several numbers for different meanings, and FO4 changes slot
/// semantics without changing the enum at all.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum TextureSlotLayout {
    #[default]
    Skyrim,
    Fallout4,
    Fallout76,
    Starfield,
}

impl TextureSlotLayout {
    pub const fn from_bsver(bsver: u32) -> Self {
        if bsver >= crate::version::bsver::STARFIELD {
            Self::Starfield
        } else if bsver >= crate::version::bsver::FO76 {
            Self::Fallout76
        } else if bsver >= crate::version::bsver::FALLOUT4 {
            Self::Fallout4
        } else {
            Self::Skyrim
        }
    }
}

/// All non-slot inputs required to interpret a texture-set entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureSlotContext {
    pub layout: TextureSlotLayout,
    /// Canonical Skyrim-numbered shader type, after
    /// [`canonical_shader_type`] has translated the source enum.
    pub shader_type: u32,
    pub glow_map: bool,
    pub model_space_normals: bool,
    pub soft_lighting: bool,
    pub rim_lighting: bool,
    pub back_lighting: bool,
}

/// Translate a source shader-type integer into the canonical numbering used by
/// `Material.material_kind` and the slot table.
///
/// FO76's `BSShaderType155` is not the Skyrim/FO4 enum: 3/4/5/12 mean
/// FaceTint/SkinTint/HairTint/EyeEnvmap rather than
/// Parallax/FaceTint/SkinTint/TreeAnim. FO76 Terrain (17) has no canonical
/// Skyrim material kind and no renderer branch, so it deliberately degrades to
/// Default instead of masquerading as Skyrim Cloud.
pub const fn canonical_shader_type(layout: TextureSlotLayout, raw: u32) -> u32 {
    if matches!(layout, TextureSlotLayout::Fallout76) {
        match raw {
            3 => bs_lighting::FACE_TINT,
            4 => bs_lighting::SKIN_TINT,
            5 => bs_lighting::HAIR_TINT,
            12 => bs_lighting::EYE_ENVMAP,
            17 => 0,
            _ => raw,
        }
    } else {
        raw
    }
}

static UNROUTED_AUTHORED_SLOTS: [AtomicU64; 32] = [const { AtomicU64::new(0) }; 32];

const fn layout_index(layout: TextureSlotLayout) -> usize {
    match layout {
        TextureSlotLayout::Skyrim => 0,
        TextureSlotLayout::Fallout4 => 1,
        TextureSlotLayout::Fallout76 => 2,
        TextureSlotLayout::Starfield => 3,
    }
}

/// Process-lifetime count, per game layout and raw slot, of non-empty NIF
/// texture bindings which reached no canonical role. This makes future table
/// gaps observable to corpus probes instead of silently disappearing like the
/// pre-#3085 FO76 slot-6 bindings.
pub fn unrouted_texture_slot_bindings(layout: TextureSlotLayout, slot: u32) -> u64 {
    if slot >= 8 {
        return 0;
    }
    UNROUTED_AUTHORED_SLOTS[layout_index(layout) * 8 + slot as usize].load(Ordering::Relaxed)
}

pub(crate) fn record_unrouted_texture_slot(context: TextureSlotContext, slot: u32) {
    if slot >= 8 {
        return;
    }
    let count = UNROUTED_AUTHORED_SLOTS[layout_index(context.layout) * 8 + slot as usize]
        .fetch_add(1, Ordering::Relaxed)
        + 1;
    if count.is_power_of_two() {
        log::debug!(
            "unrouted authored BSShaderTextureSet bindings for {:?} slot {}: {count} (latest type {})",
            context.layout,
            slot,
            context.shader_type,
        );
    }
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
/// `model_space_normals` gates slot 7 on Skyrim only. FO4 authors specular in
/// slot 7 regardless of that flag (#2998), while the shipped FO76 corpus puts
/// specular in slot 6: 1,616 of 1,664 populated bindings use an `_s.dds`
/// suffix, across all five FO76 shader types observed (#3085).
pub fn slot_to_role(context: TextureSlotContext, slot: u32) -> Option<TextureRole> {
    let shader_type = context.shader_type;
    let tint_family = matches!(
        shader_type,
        bs_lighting::FACE_TINT | bs_lighting::SKIN_TINT | bs_lighting::HAIR_TINT
    );

    match (context.layout, slot) {
        (_, 0) => Some(TextureRole::BaseColor),
        (_, 1) => Some(TextureRole::Normal),

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
        (TextureSlotLayout::Skyrim | TextureSlotLayout::Starfield, 2) => {
            if tint_family {
                Some(TextureRole::Tint)
            } else if context.glow_map {
                Some(TextureRole::Emissive)
            } else if context.soft_lighting || context.rim_lighting {
                Some(TextureRole::LightingMask)
            } else {
                // Skyrim multiplexes slot 2 between Glow_Map, Soft_Lighting,
                // and Rim_Lighting. The latter two have no canonical texture
                // role yet and must not become self-illumination (#3068).
                None
            }
        }
        (TextureSlotLayout::Fallout4, 2) => {
            if tint_family {
                Some(TextureRole::Tint)
            } else {
                Some(TextureRole::Emissive)
            }
        }
        (TextureSlotLayout::Fallout76, 2) => {
            if tint_family {
                Some(TextureRole::Tint)
            } else if context.glow_map {
                Some(TextureRole::Emissive)
            } else {
                None
            }
        }

        // #2694 — nif.xml calls slot 3 "Height/Parallax" generically, but every
        // vanilla FaceTint puts a *detail* map there
        // (`MaleHeadDetail_Rough01.dds`, 3149/3158). Feeding that to the height
        // role made `triangle.frag` ray-march POM over a face complexion map —
        // its POM branch gates only on `parallaxMapIndex != 0u`, with no
        // material-kind check.
        (TextureSlotLayout::Skyrim | TextureSlotLayout::Starfield, 3) => match shader_type {
            bs_lighting::FACE_TINT => Some(TextureRole::Detail),
            _ => Some(TextureRole::Height),
        },
        // FO4/FO76 slot 3 is the greyscale-to-palette gradient, not a POM
        // height field. FO4 ships it on 31,303 properties (#2997).
        (TextureSlotLayout::Fallout4 | TextureSlotLayout::Fallout76, 3) => {
            Some(TextureRole::GreyscaleLut)
        }

        // #1350 — types 4/5/6 declare no TS slot 4/5 on Skyrim; skip
        // explicitly so a stray authored string cannot bind an env cube.
        // FaceTint's slots 4/5 are absent on 100% of vanilla Skyrim
        // properties. **This occupancy claim is Skyrim-only — see the FO4
        // arm below, #2999.**
        (
            TextureSlotLayout::Skyrim | TextureSlotLayout::Starfield | TextureSlotLayout::Fallout76,
            4,
        ) => (!tint_family).then_some(TextureRole::Environment),
        (
            TextureSlotLayout::Skyrim | TextureSlotLayout::Starfield | TextureSlotLayout::Fallout76,
            5,
        ) => (!tint_family).then_some(TextureRole::EnvironmentMask),

        // #2999 — FO4 slots 4/5 ARE routinely authored on FaceTint/SkinTint/
        // HairTint heads, unlike Skyrim. Measured on `Fallout4 - Meshes.ba2`
        // type-4 FaceTint properties (n=1,229): slot 4 50.7% non-empty, all
        // genuine environment cubemaps
        // (`Shared/Cubemaps/mipblur_DefaultOutside1_dielectric.dds`); slot 5
        // 79.8% non-empty, all `_n` wrinkle/crease normals
        // (`BaseFemaleHeadWrinkles_n.DDS`, `HeadWrinkles_n.dds`,
        // `Gen2SkinHeadCrease_n.dds`, `SupermutantHeadCrease_n.dds`).
        // `MeshesExtra` reproduces both. Slot 4 is unconditionally
        // Environment regardless of shader type — non-tint FO4 types carry
        // it too, same as Skyrim's non-tint case. Slot 5 is Wrinkle only on
        // the tint family; non-tint FO4 types (e.g. type 1) measured `_m`
        // mask entries there, matching the ordinary EnvironmentMask role.
        (TextureSlotLayout::Fallout4, 4) => Some(TextureRole::Environment),
        (TextureSlotLayout::Fallout4, 5) => {
            if tint_family {
                Some(TextureRole::Wrinkle)
            } else {
                Some(TextureRole::EnvironmentMask)
            }
        }

        // #2693 — the inner layer is slot **6**, not 7. nif.xml contradicts
        // itself (its enum prose says "Layer(TS7)", its field table says slot 6
        // = "Subsurface for Multilayer Parallax"); the field table wins on
        // shipped data — slot 6 is non-empty on 607/607 type-11 properties in
        // `Skyrim - Meshes0.bsa` while slot 7 holds tint maps on 370.
        //
        // FaceTint slot 6 is the baked FaceGen tint — see this fn's doc for why
        // it is deliberately unrouted.
        (
            TextureSlotLayout::Skyrim | TextureSlotLayout::Fallout4 | TextureSlotLayout::Starfield,
            6,
        ) => match shader_type {
            bs_lighting::MULTI_LAYER_PARALLAX => Some(TextureRole::InnerLayer),
            _ => None,
        },
        // Measured over 95,041 FO76 NIFs from the five installed mesh BA2s:
        // 1,664 bindings, 1,616 `_s.dds`; slot 6 is specular, not Skyrim's
        // multilayer inner texture (#3085).
        (TextureSlotLayout::Fallout76, 6) => Some(TextureRole::Specular),

        // Slot 7 is the alternate specular on model-space-normal materials,
        // independent of shader type (#2742) — except on type 11, where it is a
        // back-lighting map with no canonical role.
        (TextureSlotLayout::Skyrim | TextureSlotLayout::Starfield, 7) => {
            if context.back_lighting {
                Some(TextureRole::BackLighting)
            } else {
                match (shader_type, context.model_space_normals) {
                    (bs_lighting::MULTI_LAYER_PARALLAX, _) => None,
                    (_, true) => Some(TextureRole::Specular),
                    (_, false) => None,
                }
            }
        }
        // FO4 slot 7 is authored specular whether or not the almost-never-set
        // Model_Space_Normals flag is present (#2998).
        (TextureSlotLayout::Fallout4, 7) => Some(TextureRole::Specular),
        (TextureSlotLayout::Fallout76, 7) => None,

        (_, _) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::bs_lighting;
    use super::*;

    fn skyrim(shader_type: u32, glow_map: bool, model_space_normals: bool) -> TextureSlotContext {
        TextureSlotContext {
            layout: TextureSlotLayout::Skyrim,
            shader_type,
            glow_map,
            model_space_normals,
            soft_lighting: false,
            rim_lighting: false,
            back_lighting: false,
        }
    }

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
                slot_to_role(skyrim(ty, false, false), 2),
                Some(TextureRole::Tint),
                "shader_type {ty} slot 2 must be the skin/hair tint mask"
            );
        }
        // slot 3 on FaceTint → Detail, not Height (POM over a face otherwise).
        assert_eq!(
            slot_to_role(skyrim(bs_lighting::FACE_TINT, false, false), 3),
            Some(TextureRole::Detail)
        );
        // slots 4/5 on the tint family → nothing, not env/env-mask.
        for ty in [
            bs_lighting::FACE_TINT,
            bs_lighting::SKIN_TINT,
            bs_lighting::HAIR_TINT,
        ] {
            assert_eq!(slot_to_role(skyrim(ty, false, false), 4), None);
            assert_eq!(slot_to_role(skyrim(ty, false, false), 5), None);
        }
        // slot 7 on MultiLayerParallax → nothing (back lighting), even with MSN.
        assert_eq!(
            slot_to_role(skyrim(bs_lighting::MULTI_LAYER_PARALLAX, false, true), 7,),
            None
        );
        assert_eq!(
            slot_to_role(skyrim(bs_lighting::MULTI_LAYER_PARALLAX, false, false), 7,),
            None
        );
    }

    /// The ordinary path is unchanged — this is what the overwhelming majority
    /// of content takes, and the fix must not disturb it.
    #[test]
    fn default_shader_type_keeps_the_conventional_mapping() {
        let ty = 0; // Default
        let context = skyrim(ty, true, false);
        assert_eq!(slot_to_role(context, 0), Some(TextureRole::BaseColor));
        assert_eq!(slot_to_role(context, 1), Some(TextureRole::Normal));
        assert_eq!(slot_to_role(context, 2), Some(TextureRole::Emissive));
        assert_eq!(slot_to_role(context, 3), Some(TextureRole::Height));
        assert_eq!(slot_to_role(context, 4), Some(TextureRole::Environment));
        assert_eq!(slot_to_role(context, 5), Some(TextureRole::EnvironmentMask));
        assert_eq!(slot_to_role(context, 6), None);
    }

    #[test]
    fn skyrim_slot_two_requires_the_glow_map_flag() {
        assert_eq!(
            slot_to_role(skyrim(0, false, false), 2),
            None,
            "a Glow_Map-clear soft/rim mask must not land in emissive (#3068)"
        );
        assert_eq!(
            slot_to_role(skyrim(0, true, false), 2),
            Some(TextureRole::Emissive)
        );
    }

    /// #2693 — inner layer is slot 6 on type 11, and slot 6 is inert elsewhere.
    #[test]
    fn multi_layer_parallax_inner_layer_is_slot_six() {
        assert_eq!(
            slot_to_role(skyrim(bs_lighting::MULTI_LAYER_PARALLAX, false, false), 6,),
            Some(TextureRole::InnerLayer)
        );
        assert_eq!(slot_to_role(skyrim(0, false, false), 6), None);
        // Type 11 still takes env/env-mask at 4/5.
        assert_eq!(
            slot_to_role(skyrim(bs_lighting::MULTI_LAYER_PARALLAX, false, false), 4,),
            Some(TextureRole::Environment)
        );
        assert_eq!(
            slot_to_role(skyrim(bs_lighting::MULTI_LAYER_PARALLAX, false, false), 5,),
            Some(TextureRole::EnvironmentMask)
        );
    }

    /// #2742 — slot 7 is specular only under model-space normals, and that is
    /// shader-type independent (SkinTint included, which #1350 had dropped).
    #[test]
    fn slot_seven_is_specular_only_under_model_space_normals() {
        assert_eq!(
            slot_to_role(skyrim(bs_lighting::SKIN_TINT, false, true), 7),
            Some(TextureRole::Specular)
        );
        assert_eq!(
            slot_to_role(skyrim(bs_lighting::SKIN_TINT, false, false), 7),
            None
        );
        assert_eq!(
            slot_to_role(skyrim(0, false, true), 7),
            Some(TextureRole::Specular)
        );
        assert_eq!(slot_to_role(skyrim(0, false, false), 7), None);
    }

    /// #2999 — FO4 FaceTint/SkinTint/HairTint slots 4/5 must land in
    /// Environment / Wrinkle, not skip (the Skyrim-measured #1350
    /// occupancy claim does not hold on FO4). Non-tint FO4 shader types
    /// keep the ordinary Environment/EnvironmentMask reading at 4/5,
    /// matching the measured type-1 `_m` mask entries at slot 5.
    #[test]
    fn fo4_tint_family_routes_slots_four_and_five_to_cubemap_and_wrinkle() {
        for ty in [
            bs_lighting::FACE_TINT,
            bs_lighting::SKIN_TINT,
            bs_lighting::HAIR_TINT,
        ] {
            let context = TextureSlotContext {
                layout: TextureSlotLayout::Fallout4,
                shader_type: ty,
                glow_map: false,
                model_space_normals: false,
                soft_lighting: false,
                rim_lighting: false,
                back_lighting: false,
            };
            assert_eq!(
                slot_to_role(context, 4),
                Some(TextureRole::Environment),
                "FO4 shader_type {ty} slot 4 must be the environment cubemap (#2999)"
            );
            assert_eq!(
                slot_to_role(context, 5),
                Some(TextureRole::Wrinkle),
                "FO4 shader_type {ty} slot 5 must be the wrinkle/crease normal (#2999)"
            );
        }
        // Non-tint FO4 shader type keeps the ordinary reading.
        let non_tint = TextureSlotContext {
            layout: TextureSlotLayout::Fallout4,
            shader_type: 0,
            glow_map: false,
            model_space_normals: false,
            soft_lighting: false,
            rim_lighting: false,
            back_lighting: false,
        };
        assert_eq!(slot_to_role(non_tint, 4), Some(TextureRole::Environment));
        assert_eq!(
            slot_to_role(non_tint, 5),
            Some(TextureRole::EnvironmentMask),
            "non-tint FO4 slot 5 stays EnvironmentMask, matching the measured \
             type-1 `_m` entries — Wrinkle is tint-family only"
        );
        // Skyrim/Starfield/FO76 keep the pre-#2999 skip on the tint family —
        // this occupancy claim IS accurate there (#1350).
        for layout in [
            TextureSlotLayout::Skyrim,
            TextureSlotLayout::Starfield,
            TextureSlotLayout::Fallout76,
        ] {
            let context = TextureSlotContext {
                layout,
                shader_type: bs_lighting::FACE_TINT,
                glow_map: false,
                model_space_normals: false,
                soft_lighting: false,
                rim_lighting: false,
                back_lighting: false,
            };
            assert_eq!(
                slot_to_role(context, 4),
                None,
                "{layout:?} FaceTint slot 4 must still skip — the #1350 claim holds here"
            );
            assert_eq!(
                slot_to_role(context, 5),
                None,
                "{layout:?} FaceTint slot 5 must still skip — the #1350 claim holds here"
            );
        }
    }

    #[test]
    fn fo4_routes_palette_and_specular_without_skyrim_gates() {
        let context = TextureSlotContext {
            layout: TextureSlotLayout::Fallout4,
            shader_type: 0,
            glow_map: false,
            model_space_normals: false,
            soft_lighting: false,
            rim_lighting: false,
            back_lighting: false,
        };
        assert_eq!(
            slot_to_role(context, 3),
            Some(TextureRole::GreyscaleLut),
            "FO4 slot 3 is a palette gradient, not POM height (#2997)"
        );
        assert_eq!(
            slot_to_role(context, 7),
            Some(TextureRole::Specular),
            "FO4 slot 7 must not depend on Model_Space_Normals (#2998)"
        );
    }

    #[test]
    fn fo76_slot_six_is_specular_and_enum_is_canonicalized() {
        let context = TextureSlotContext {
            layout: TextureSlotLayout::Fallout76,
            shader_type: canonical_shader_type(TextureSlotLayout::Fallout76, 5),
            glow_map: false,
            model_space_normals: false,
            soft_lighting: false,
            rim_lighting: false,
            back_lighting: false,
        };
        assert_eq!(context.shader_type, bs_lighting::HAIR_TINT);
        assert_eq!(
            slot_to_role(context, 6),
            Some(TextureRole::Specular),
            "the measured FO76 `_s.dds` slot must reach a canonical role (#3085)"
        );
        assert_eq!(
            canonical_shader_type(TextureSlotLayout::Fallout76, 3),
            bs_lighting::FACE_TINT
        );
        assert_eq!(
            canonical_shader_type(TextureSlotLayout::Fallout76, 4),
            bs_lighting::SKIN_TINT
        );
        assert_eq!(
            canonical_shader_type(TextureSlotLayout::Fallout76, 12),
            bs_lighting::EYE_ENVMAP
        );
        assert_eq!(canonical_shader_type(TextureSlotLayout::Fallout76, 17), 0);
    }

    /// Out-of-range slots resolve to `None` rather than panicking — the REFR
    /// overlay feeds this a raw `XTXR` slot index straight from the ESM.
    #[test]
    fn out_of_range_slots_are_none() {
        for slot in [8u32, 9, 64, u32::MAX] {
            assert_eq!(slot_to_role(skyrim(0, false, true), slot), None);
        }
    }

    #[test]
    fn unrouted_authored_bindings_are_counted_per_layout_and_slot() {
        let context = TextureSlotContext {
            layout: TextureSlotLayout::Fallout76,
            shader_type: 0,
            glow_map: false,
            model_space_normals: false,
            soft_lighting: false,
            rim_lighting: false,
            back_lighting: false,
        };
        let before = unrouted_texture_slot_bindings(context.layout, 7);
        record_unrouted_texture_slot(context, 7);
        assert!(unrouted_texture_slot_bindings(context.layout, 7) >= before + 1);
        assert_eq!(unrouted_texture_slot_bindings(context.layout, 8), 0);
    }

    /// No slot may resolve to the same role as another for one shader type —
    /// two slots writing one role means the later silently clobbers the
    /// earlier, which is the class of bug this table exists to prevent.
    #[test]
    fn roles_are_unique_per_shader_type() {
        for layout in [
            TextureSlotLayout::Skyrim,
            TextureSlotLayout::Fallout4,
            TextureSlotLayout::Fallout76,
            TextureSlotLayout::Starfield,
        ] {
            for ty in [
                0u32,
                bs_lighting::FACE_TINT,
                bs_lighting::SKIN_TINT,
                bs_lighting::HAIR_TINT,
                bs_lighting::MULTI_LAYER_PARALLAX,
                bs_lighting::EYE_ENVMAP,
            ] {
                for msn in [false, true] {
                    let context = TextureSlotContext {
                        layout,
                        shader_type: ty,
                        glow_map: true,
                        model_space_normals: msn,
                        soft_lighting: false,
                        rim_lighting: false,
                        back_lighting: false,
                    };
                    let mut seen = Vec::new();
                    for slot in 0..8u32 {
                        if let Some(role) = slot_to_role(context, slot) {
                            assert!(
                                !seen.contains(&role),
                                "{layout:?} shader_type {ty} (msn={msn}) maps two slots to {role:?}"
                            );
                            seen.push(role);
                        }
                    }
                }
            }
        }
    }
}
