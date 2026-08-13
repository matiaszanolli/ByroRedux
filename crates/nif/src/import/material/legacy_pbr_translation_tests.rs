//! Stage 2 of the `feedback_format_translation.md` rollout —
//! `MaterialInfo::classify_legacy_pbr` derives PBR `(metalness,
//! roughness)` at NIF-import time so every legacy
//! Oblivion / FO3 / FNV / pre-Skyrim mesh leaves the parser with
//! explicit `metalness_override` / `roughness_override` populated.
//!
//! The classifier itself is shared with `Material::classify_pbr` via
//! `byroredux_core::ecs::components::material::classify_pbr_keyword`,
//! so the heavy keyword-arm coverage lives next to that function in
//! the core crate. The tests here pin the parser-side adapter — that
//! MaterialInfo's three relevant fields (`texture_path`, `glossiness`,
//! `env_map_scale`, `normal_map.is_some()`) reach the classifier in
//! the right shape, and that the StringPool resolution round-trip
//! preserves the texture-path string.

use super::super::material::MaterialInfo;
use byroredux_core::string::StringPool;

#[test]
fn classifier_routes_metal_path_to_conductor() {
    let mut pool = StringPool::new();
    let path = pool.intern(r"Textures\Weapons\Iron\IronSword.dds");
    let mut info = MaterialInfo::default();
    info.texture_path = Some(path);

    let pbr = info.classify_legacy_pbr(&pool);
    assert!(pbr.metalness > 0.8, "metal keyword routes to conductor");
    // Roughness raised from 0.3 → 0.6 (worn/industrial metal, not mirror chrome).
    assert!(pbr.roughness >= 0.5 && pbr.roughness < 0.8);
}

#[test]
fn classifier_routes_wood_path_to_dielectric() {
    let mut pool = StringPool::new();
    let path = pool.intern("textures/clutter/barrel/barrel01.dds");
    let mut info = MaterialInfo::default();
    info.texture_path = Some(path);

    let pbr = info.classify_legacy_pbr(&pool);
    assert_eq!(pbr.metalness, 0.0, "wood is dielectric");
    assert!(pbr.roughness > 0.6);
}

#[test]
fn classifier_falls_back_to_glossiness_on_unknown_path() {
    let mut pool = StringPool::new();
    let path = pool.intern("textures/unknown/thing.dds");
    let mut info = MaterialInfo::default();
    info.texture_path = Some(path);
    info.glossiness = 20.0;
    info.env_map_scale = 0.0;

    let pbr = info.classify_legacy_pbr(&pool);
    assert_eq!(pbr.metalness, 0.0);
    assert!(
        pbr.roughness > 0.5,
        "low glossiness → high roughness on no-keyword fallback"
    );
}

#[test]
fn classifier_handles_missing_texture_path() {
    let pool = StringPool::new();
    let info = MaterialInfo::default();

    let pbr = info.classify_legacy_pbr(&pool);
    // Default glossiness 80 + no normal map → falls to dielectric
    // with the glossiness-fallback roughness; never panics.
    assert_eq!(pbr.metalness, 0.0);
    assert!(pbr.roughness > 0.0 && pbr.roughness < 1.0);
}

// REN-2026-07-04-M01 / #1873 — a `BSShaderPPLightingProperty`-only mesh
// (no co-bound `NiMaterialProperty`/`BSLightingShaderProperty`) authors
// `env_map_scale` but never touches `specular_color`, leaving it at
// `MaterialInfo`'s unauthored `[1.0; 3]` struct default and
// `has_material_data = false`. Pre-fix the classifier read that default's
// luminance as "authored white specular" and chromed decorative FO3/FNV
// flyers/posters that never had a real specular tint.

#[test]
fn classifier_unauthored_specular_default_does_not_chrome_flyer() {
    let pool = StringPool::new();
    let mut info = MaterialInfo::default();
    // Mirrors the PPLighting walker arm: env_map_scale authored, but no
    // NiMaterialProperty/BSLightingShaderProperty bound, so
    // has_material_data and specular_color stay at their defaults.
    info.env_map_scale = 1.0;
    assert!(!info.has_material_data);
    assert_eq!(info.specular_color, [1.0, 1.0, 1.0]);

    let pbr = info.classify_legacy_pbr(&pool);
    assert_eq!(
        pbr.metalness, 0.0,
        "unauthored specular_color default must not be read as chrome"
    );
    assert!(
        pbr.roughness >= 0.6,
        "must stay above the RT reflection gate"
    );
}

#[test]
fn classifier_genuinely_authored_white_specular_stays_metallic() {
    let pool = StringPool::new();
    let mut info = MaterialInfo::default();
    // A real NiMaterialProperty/BSLightingShaderProperty bind authoring
    // a genuine white specular tint (chrome/polished metal intent) must
    // still classify as metallic — the fix discriminates on provenance,
    // not on the specular value itself.
    info.env_map_scale = 1.0;
    info.specular_color = [1.0, 1.0, 1.0];
    info.has_material_data = true;
    info.specular_authored = true;

    let pbr = info.classify_legacy_pbr(&pool);
    assert!(
        pbr.metalness > 0.0,
        "authored white specular must still lift metalness"
    );
}

// #2352 (SF-D8-01) — `apply_bs_effect_shader`/`apply_bs_sky_shader`/
// `apply_bs_water_shader` all set `has_material_data = true` without ever
// touching `specular_color`, which stays at its unauthored `[1.0; 3]`
// struct default. Pre-fix, `specular_authored` was wired straight to
// `has_material_data`, so these three arms fed the classifier the same
// "authored white specular" signal as a genuine NiMaterialProperty/
// BSLightingShaderProperty bind — fabricating chrome-tier metalness/
// roughness on every effect-shader/sky/water surface with
// `env_map_scale > 0.3`. This is exactly the #1873 chrome-flyer bug,
// reached through a different set of walker arms.
#[test]
fn classifier_effect_shader_arm_shape_does_not_chrome() {
    let pool = StringPool::new();
    let mut info = MaterialInfo::default();
    // Mirrors what apply_bs_effect_shader/apply_bs_sky_shader/
    // apply_bs_water_shader actually leave behind: has_material_data set,
    // specular_color untouched (still the struct default), and
    // specular_authored correctly NOT set (the fix under test).
    info.env_map_scale = 1.0;
    info.has_material_data = true;
    assert_eq!(info.specular_color, [1.0, 1.0, 1.0]);
    assert!(
        !info.specular_authored,
        "has_material_data alone must not imply specular authorship"
    );

    let pbr = info.classify_legacy_pbr(&pool);
    assert_eq!(
        pbr.metalness, 0.0,
        "unauthored specular_color must not be read as chrome on the \
         effect/sky/water arm shape"
    );
    assert!(
        pbr.roughness >= 0.6,
        "must stay above the RT reflection gate"
    );
}

// #2707 (SF-D8-01) — `into_imported_material` must leave
// `metalness_override`/`roughness_override` as `None` (deferring to
// `Material::resolve_pbr`'s NaN-sentinel backstop) rather than stamping
// the classifier's terminal fallback as a fabricated `Some(...)`, when
// `MaterialInfo` carries no PBR classifier signal at all — exactly the
// state a Starfield material-reference stub is left in (the walker
// returns before writing a single field). Any real signal, however
// partial, must still stamp `Some(...)` unchanged from before this fix.

#[test]
fn has_no_pbr_classifier_signal_is_true_on_an_untouched_material_info() {
    let info = MaterialInfo::default();
    assert!(
        info.has_no_pbr_classifier_signal(),
        "a completely untouched MaterialInfo (the Starfield stub case) \
         must report no classifier signal"
    );
}

#[test]
fn has_no_pbr_classifier_signal_is_false_once_any_signal_is_present() {
    let mut pool = StringPool::new();
    // Just a texture path — the FO3/FNV BSShaderPPLightingProperty-only
    // shape, which never sets `has_material_data` (#2457) but DOES carry
    // a real signal the classifier can use.
    let mut info = MaterialInfo::default();
    info.texture_path = Some(pool.intern("textures/clutter/barrel/barrel01.dds"));
    assert!(
        !info.has_no_pbr_classifier_signal(),
        "a real texture path is a classifier signal even without has_material_data"
    );
}

#[test]
fn into_imported_material_leaves_overrides_none_for_an_empty_stub() {
    let mut pool = StringPool::new();
    let info = MaterialInfo::default();
    let imported = info.into_imported_material(&mut pool, None);
    assert_eq!(
        imported.metalness_override, None,
        "an empty stub must leave metalness_override unset so \
         Material::resolve_pbr's NaN-sentinel backstop can classify \
         from whatever real data merges in later, instead of a value \
         fabricated from an input set that was empty by construction"
    );
    assert_eq!(imported.roughness_override, None);
}

#[test]
fn into_imported_material_keeps_overrides_some_when_any_signal_present() {
    let mut pool = StringPool::new();
    let mut info = MaterialInfo::default();
    info.texture_path = Some(pool.intern(r"Textures\Weapons\Iron\IronSword.dds"));
    let imported = info.into_imported_material(&mut pool, None);
    assert!(
        imported.metalness_override.is_some(),
        "real classifier signal (even partial) must still stamp Some(...), \
         unchanged from before this fix"
    );
    assert!(imported.roughness_override.is_some());
}
