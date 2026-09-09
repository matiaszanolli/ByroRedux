//! Material-path resolution and the external-sidecar merge boundary.
//!
//! #3857 — split from a single 2219-line `material.rs`. The clusters were
//! already separable; what needed care was #2412's invariant, which closed
//! this file's previous review with *"a deliberate single NIFAL boundary
//! [that] should not be split in a way that weakens that invariant"*. It is
//! preserved literally rather than merely respected: `merge_external_material`
//! is still the one entry point and the one place a sidecar touches
//! `&mut ImportedMaterial`.
//!
//! - [`cdb`] — Starfield `materialsbeta.cdb` discovery, probing, fallback
//! - [`provider`] — archive-backed sidecar lookup and its four LRU caches
//! - [`merge`] — the NIFAL boundary itself
//!
//! The BGSM/BGEM *semantics* helpers stay here: they are pure translations
//! (a blend enum, a metalness derivation, a glass-behaviour predicate) shared
//! by the merge arms, and they are what `/audit-nifal` reads first.

mod cdb;
mod merge;
mod provider;

pub(crate) use cdb::{cdb_scan_candidates, discover_starfield_cdbs, unresolved_material_warning};
// Test-only re-exports, at the width these symbols had before the split
// (`pub(super)` — visible to `asset_provider` and its test siblings, not the
// whole crate). `tests/starfield_mat.rs` reaches them through `use super::*`,
// which the unused-import lint cannot see through: without the `cfg(test)`
// gate the production build warns, and without the re-export the test build
// does not compile. Both halves are load-bearing.
#[cfg(test)]
pub(super) use cdb::{
    is_materialsbeta_cdb_path, probe_starfield_cdb, sf_cdb_cache, sf_cdb_cache_insert,
    SF_CDB_CACHE_MAX_ENTRIES,
};
pub(crate) use merge::{merge_external_material, MergeOutcome};
// The CDB memo was `pub(super)` before the split — visible to
// `asset_provider` and its test siblings, not the whole crate. Re-exported at
// that same width so the eviction tests in `tests/starfield_mat.rs` keep
// reaching it without widening the surface.
pub(crate) use provider::{build_material_provider, MaterialProvider};

/// Every source file this module was split across, concatenated (#3857).
///
/// Two tests in `asset_provider/tests/bgsm_merge.rs` read this module as
/// *text* — one pins the `#2642` deferral marker at the merge site, the other
/// pins `push_archive`'s cache-clear, a site no unit test can reach. Before
/// the split both used `include_str!("../material.rs")`. Pointing each at
/// whichever new file it happens to live in today would make them silently
/// vacuous the next time a helper moves between siblings; one concatenation
/// keeps both looking at the same text they always did. Same reasoning, and
/// the same hazard, as `boot::SOURCES` (#3855).
#[cfg(test)]
pub(crate) const SOURCES: &str = concat!(
    include_str!("mod.rs"),
    include_str!("cdb.rs"),
    include_str!("provider.rs"),
    include_str!("merge.rs"),
);

use super::*;

use byroredux_bgsm::{BgemFile, BgsmFile};
use byroredux_nif::import::ImportedMaterial;

/// #1077 / FO4-D6-003 (Phase 1: data propagation) — forwards one BGSM
/// chain step's `translucency` / `model_space_normals` shader-flag bits
/// into `ImportedMaterial`. Same child-first precedence as every texture
/// slot in [`merge_external_material`]'s walk: first authored `true`
/// wins, so a step whose own flag is unset (`false`) doesn't clobber a
/// value an earlier (closer) step in the chain already set. Sets
/// `*touched = true` whenever it flips either flag.
///
/// Extracted out of the merge loop (#2702 / FO4-D2-03) so its three
/// regression tests call the real production logic instead of a
/// hand-copied mirror — the mirror was proven able to diverge silently
/// from `merge_external_material` (the FO4-D2-01 `is_pbr` contract flip
/// landed with a green mirror suite while its own comment kept stating
/// the pre-flip behaviour). #2700 restored the pre-flip contract itself
/// (see [`merge_external_material`]'s BGSM arm) — `is_pbr` is unconditional
/// on any successful BGSM resolve again, not gated on a `bgsm.pbr` bit
/// vanilla content essentially never sets.
pub(crate) fn forward_bgsm_phase1_flags(
    material: &mut ImportedMaterial,
    bgsm: &BgsmFile,
    touched: &mut bool,
) {
    if !material.has_translucency && bgsm.translucency {
        material.has_translucency = true;
        *touched = true;
    }
    if !material.model_space_normals && bgsm.model_space_normals {
        material.model_space_normals = true;
        *touched = true;
    }
}

/// #2607 (FO4-D7-02) — forward BGSM's rim / backlight / subsurface shading
/// scalars onto `ImportedMaterial`'s matching sinks.
///
/// The parser has decoded these since the crate existed, `ImportedMaterial`
/// has had `rimlight_power` / `backlight_power` / `subsurface_rolloff` since
/// #2284 wired the NIF-native path, and `translate_material` already forwards
/// all three onto the canonical `Material` — only this hop was missing, so
/// every BGSM-authored surface fed the Disney subsurface/rimlight lobe
/// hardcoded zeros. #1352 (unconditional `MAT_FLAG_PBR_BSDF` for BGSM
/// content) is what makes that visible rather than inert.
///
/// **Gated on the authored enable bits, deliberately.** The parser reads this
/// whole group only on the `version < 8` branch — the v>=8 layout spends those
/// bytes on the translucency suite instead — so on a modern BGSM `rim_power`
/// and `subsurface_lighting_rolloff` still hold their struct defaults of
/// 2.0 / 0.3. Those are never-parsed values, and forwarding them would be
/// fabrication, not translation. `rim_lighting` / `subsurface_lighting` are
/// false in exactly that case, which makes them the correct and complete gate.
///
/// `back_light_power` shares the rim enable bit: the format gives it none of
/// its own, and the Bethesda Material Editor authors it in the rim-lighting
/// group.
///
/// Child-first precedence via the caller's sentinels, matching every other
/// payload-carrying field in the walk. Extracted from the merge loop for the
/// same reason as [`forward_bgsm_phase1_flags`] (#2702): so the regression
/// tests drive the real production logic rather than a hand-copied mirror.
pub(crate) fn forward_bgsm_rim_subsurface(
    material: &mut ImportedMaterial,
    bgsm: &BgsmFile,
    set_rim: &mut bool,
    set_subsurface: &mut bool,
    touched: &mut bool,
) {
    if !*set_rim && (bgsm.rim_lighting || bgsm.back_lighting) {
        material.rimlight_power = bgsm.rim_power;
        material.backlight_power = bgsm.back_light_power;
        material.rim_lighting = bgsm.rim_lighting;
        material.back_lighting = bgsm.back_lighting;
        *set_rim = true;
        *touched = true;
    }
    if !*set_subsurface && bgsm.subsurface_lighting {
        material.subsurface_rolloff = bgsm.subsurface_lighting_rolloff;
        material.soft_lighting = true;
        *set_subsurface = true;
        *touched = true;
    }
}

/// #2608 (FO4-D7-03) — forward BGSM's authored env-map mask scale.
///
/// Same drop-at-the-merge-boundary class as [`forward_bgsm_rim_subsurface`]:
/// `merge_external_material` forwards env-map *textures* but was dropping the
/// scale that modulates them.
///
/// **Gated on `base.environment_mapping`, deliberately.**
/// `BaseMaterial::parse_after_magic` reads the `(environment_mapping,
/// environment_mapping_mask_scale)` pair only when `version < 10`; from v10
/// those bytes became `depth_bias` and the parser substitutes a synthetic
/// `(false, 1.0)`. Forwarding unconditionally would stamp `env_map_scale =
/// 1.0` onto every modern BGSM, and `Material::resolve_pbr`'s
/// `env_map_scale > 0.3` arm reads that as authored reflection intent — a
/// fabricated input driving real roughness on the majority of FO4 content.
/// The enable bit is false in precisely the never-parsed case.
///
/// (BGSM has no v>=10 re-read of this pair, so the base bit is the whole
/// story here. BGEM's `env_mapping_enabled()` accessor exists because BGEM
/// *does* re-read it in its own subclass section.)
pub(crate) fn forward_bgsm_env_map_scale(
    material: &mut ImportedMaterial,
    bgsm: &BgsmFile,
    set_env_map_scale: &mut bool,
    touched: &mut bool,
) {
    if !*set_env_map_scale && bgsm.base.environment_mapping {
        material.env_map_scale = bgsm.base.environment_mapping_mask_scale;
        *set_env_map_scale = true;
        *touched = true;
    }
}

/// Conductor diffuse-tint blend (#1591). When saturation-derived
/// `metalness > 0.5`, bias the diffuse albedo halfway toward the authored
/// spec CHROMATICITY so the shader's `F0 = mix(0.04, albedo, metalness)`
/// lands on the right conductor tint even when the DDS albedo is
/// BC1-desaturated. The half weight keeps the diffuse texture's detail
/// (rivets, wear, edge highlights) visually present.
///
/// Blends toward the mult-free `specular_color`, NOT `specular_color ×
/// specular_mult`: per #1476 the `mult` only scales highlight strength —
/// it's not an albedo/F0 quantity — so folding it in darkened the tint
/// toward black for `mult < 1` and overshot past 1.0 (unclamped into
/// `GpuMaterial.diffuse_*`) for `mult > 1`. Making `mult` structurally
/// absent from this signature is the guarantee. Output is clamped to `[0,1]`.
pub(crate) fn conductor_diffuse_tint(diffuse: [f32; 3], specular_color: [f32; 3]) -> [f32; 3] {
    [
        (0.5 * diffuse[0] + 0.5 * specular_color[0]).clamp(0.0, 1.0),
        (0.5 * diffuse[1] + 0.5 * specular_color[1]).clamp(0.0, 1.0),
        (0.5 * diffuse[2] + 0.5 * specular_color[2]).clamp(0.0, 1.0),
    ]
}

/// Derive scalar metalness from a BGSM leaf's authored specular (#1476,
/// `08ed03be`). `spec` is `specular_color * specular_mult` for the pbr
/// branch, or raw `specular_color` for the legacy branch — see call site.
///
/// - `pbr = true`: true metallic-roughness authoring, `spec` is F0 —
///   metalness follows F0 luminance.
/// - `pbr = false`: legacy spec-glossiness. `mult` only scales highlight
///   TINT, not F0 — it is ~white `[1,1,1]` for every dielectric (concrete,
///   wood, plaster, painted metal). Keying metalness off luminance here is
///   BACKWARDS: vanilla `paintpeelingconcrete` authors `spec=[1,1,1]
///   mult=1.0` (lum 1.0 → metalness 1.0, mirror-chrome concrete) while real
///   metals author lower, often tinted spec — `metallocker` `[1,0.85,0.70]
///   mult=0.45`. The only legacy signal that distinguishes a conductor is
///   spec CHROMATICITY (conductor F0 is tinted; dielectric F0 is
///   achromatic grey), so metalness is derived from spec-color saturation
///   `(max-min)/max`, which is mult-invariant: white spec → 0, tinted
///   spec → metallic.
pub(crate) fn bgsm_metalness(spec: [f32; 3], pbr: bool) -> f32 {
    if pbr {
        let spec_lum = 0.2126 * spec[0] + 0.7152 * spec[1] + 0.0722 * spec[2];
        ((spec_lum - 0.04) / 0.96).clamp(0.0, 1.0)
    } else {
        let mx = spec[0].max(spec[1]).max(spec[2]);
        let mn = spec[0].min(spec[1]).min(spec[2]);
        if mx > 1.0e-4 {
            ((mx - mn) / mx).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// Select the shared transmissive-glass behavior from BGEM authoring.
///
/// Modern BGEM v21+ files expose `glass_enabled`, while older FO4 BGEMs
/// predate that field. Vanilla still authors clear hard-surface shells in
/// those files through a coherent feature set: standard alpha blending,
/// no depth write, two-sided/non-occluding geometry, lit view-angle falloff,
/// and an environment-map + mask + normal-map stack. The Port-A-Diner dome
/// is the canonical v2 example. Treat that feature bundle as the legacy
/// spelling of the same shared glass behavior; the individual maps remain
/// material overlays after classification.
pub(crate) fn bgem_uses_glass_behavior(bgem: &BgemFile) -> bool {
    // #2626 / SF-D9-2026-08-07-01 — `base.refraction` used to short-circuit
    // this too. It's a shared BaseMaterial screen-distortion bit authored
    // on heat shimmer, cloaking shells, force-field ripple, and fire/plasma
    // distortion — none of which are glass — and unlike `glass_enabled`
    // (a v21+ field authored specifically to mean glass) it's neither
    // version-gated nor bundled with any of the other glass-shaped
    // conjuncts below. Checking it unconditionally fired on v2 through v22
    // alike, demoting correctly-classified effect-shader content (the
    // #2297 fire-refraction corpus) to MATERIAL_KIND_GLASS.
    if bgem.glass_enabled {
        return true;
    }

    let blend = bgem.base.alpha_blend_mode;
    let standard_alpha = blend.function > 0 && blend.src_blend == 6 && blend.dst_blend == 7;
    let hard_transparent_shell = standard_alpha
        && bgem.base.alpha > 0.0
        && bgem.base.alpha < 1.0
        && !bgem.base.alpha_test
        && !bgem.base.z_buffer_write
        && bgem.base.z_buffer_test
        && bgem.base.two_sided
        && bgem.base.non_occluder
        && !bgem.base.decal;
    let reflective_surface_maps = bgem.env_mapping_enabled()
        && !bgem.envmap_texture.is_empty()
        && !bgem.envmap_mask_texture.is_empty()
        && !bgem.normal_texture.is_empty();
    let lit_fresnel_falloff = bgem.effect_lighting_enabled
        && bgem.falloff_enabled
        && !bgem.soft_enabled
        && !bgem.blood_enabled
        && !bgem.base.grayscale_to_palette_color
        && !bgem.grayscale_to_palette_alpha
        && bgem.grayscale_texture.is_empty();

    bgem.base.version < 21
        && hard_transparent_shell
        && reflective_surface_maps
        && lit_fresnel_falloff
}

/// Select the thin-shell variant of the shared glass behavior.
///
/// `non_occluder` is behavioral authoring, not merely a culling hint: the
/// surface is meant to composite over geometry behind it and does not define
/// the boundary of a closed optical volume. Keep this decision in the source
/// translator so downstream rendering stays format-agnostic.
pub(crate) fn bgem_uses_thin_glass_behavior(bgem: &BgemFile) -> bool {
    bgem.base.non_occluder && bgem_uses_glass_behavior(bgem)
}

/// #951 / SAFE-26 — bounded-cache caps for `MaterialProvider`. Sized to
/// comfortably hold the unique BGEM/BGSM-ref count of any single vanilla
/// cell (~100s) plus a few cells of streaming residency.
/// The roughness a near-mirror BGSM falls back to when no gloss map can
/// modulate the clamp floor per-texel (#3639). Matches the neutral
/// `classify_pbr_keyword`'s arms already use
/// (`crates/core/src/ecs/components/material.rs`) rather than inventing a
/// second "no data" convention. Shared with
/// `material_translate::resolve_unresolved_gloss_neutral_roughness`, which
/// owns the authored-but-unresolvable half of the same rule (#3905).
pub(crate) const NEAR_MIRROR_NEUTRAL_ROUGHNESS: f32 = 0.5;

/// The floor `(1.0 - smoothness).clamp(..)` pins a near-mirror BGSM to. A
/// material still sitting exactly here at spawn has had no gloss map recover
/// it, which is what [`NEAR_MIRROR_NEUTRAL_ROUGHNESS`] exists to fix (#3905).
pub(crate) const NEAR_MIRROR_ROUGHNESS_FLOOR: f32 = 0.04;

/// Narrow a BGSM/BGEM `src_blend`/`dst_blend` value to the `u8` the
/// Gamebryo `NiAlphaProperty` blend-factor field (and
/// [`gamebryo_to_vk_blend_factor`](byroredux_renderer)) expects.
///
/// **No translation happens here** — `src_blend`/`dst_blend` are
/// already Gamebryo-native values (`ONE=0, ZERO=1, DST_COLOR=4,
/// SRC_ALPHA=6, ONE_MINUS_SRC_ALPHA=7, …`, the same scale
/// `gamebryo_to_vk_blend_factor` reads), re-derived directly from the
/// reference implementation
/// (`Material-Editor:BaseMaterialFile.cs::ConvertAlphaBlendMode`):
/// `Standard = (src=6,dst=7)`, `Additive = (src=6,dst=0)`,
/// `Multiplicative = (src=4,dst=1)`. Feeding those straight through
/// `gamebryo_to_vk_blend_factor` already produces the correct blend
/// state for all three.
///
/// This function used to be named `gl_to_gamebryo_blend` and swap
/// `0↔1` on the premise that these fields were a "GL-style enum"
/// inverted from the Gamebryo nibble. That premise was false (no such
/// GL-style enum appears anywhere in the reference source — real GL
/// blend enums are large hex constants like `GL_SRC_ALPHA = 0x0302`,
/// not small integers). The swap (#1651) fixed its motivating case (an
/// additive BGEM rendering invisible) only by accident — the fixture
/// used to justify it was a synthetic `(function=2, src=1, dst=1)`
/// tuple the reference parser never actually emits — and broke the two
/// real modes that touch `0`/`1`: Additive's `dst=0` swapped to `1`
/// (`ZERO`, killing the additive accumulation) and Multiplicative's
/// `dst=1` swapped to `0` (`ONE`, leaking the destination through).
/// Standard's `(6,7)` pair is a fixed point of the swap, which is why
/// the regression went unnoticed. Renamed on the #1823 fix so the name
/// no longer implies a translation direction that doesn't exist — a
/// future reader should not "restore" the swap.
pub(crate) fn bgsm_blend_to_gamebryo(raw: u32) -> u8 {
    raw as u8
}
