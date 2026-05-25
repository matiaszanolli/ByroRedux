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
    assert!(pbr.roughness < 0.4);
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
