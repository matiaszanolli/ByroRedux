//! Tests for `secondary_slot_tests` extracted from ../material.rs (refactor stage A).
//!
//! Same qualified path preserved (`secondary_slot_tests::FOO`).

use super::*;

#[test]
fn vertex_color_mode_decodes_all_three_values() {
    assert_eq!(
        VertexColorMode::from_source_mode(0),
        VertexColorMode::Ignore
    );
    assert_eq!(
        VertexColorMode::from_source_mode(1),
        VertexColorMode::Emissive
    );
    assert_eq!(
        VertexColorMode::from_source_mode(2),
        VertexColorMode::AmbientDiffuse
    );
}

// ── #694 / O4-02 regression guards ─────────────────────────────────
//
// `NiVertexColorProperty` carries a (vertex_mode, lighting_mode)
// pair. Pre-fix only `vertex_mode` was read, so LIGHTING_E meshes
// got their material colors double-counted (vertex color
// multiplied albedo even though Gamebryo's lighting equation had
// dropped the ambient + diffuse contributions). `from_property`
// collapses the 2D enum into the 1D source-mode axis so the
// renderer's existing `Ignore` branch handles the case.

#[test]
fn lighting_e_with_amb_diff_demotes_to_ignore() {
    // The pathological case the audit flagged: SOURCE_AMB_DIFF +
    // LIGHTING_E. Engine drops the diffuse contribution → vertex
    // color is invisible → `Ignore` is the visually correct mode.
    assert_eq!(
        VertexColorMode::from_property(2, 0),
        VertexColorMode::Ignore
    );
}

#[test]
fn lighting_e_with_emissive_stays_emissive() {
    // SOURCE_EMISSIVE + LIGHTING_E: vertex color drives the only
    // term that contributes (Emissive). Stays Emissive — the
    // collapse only applies when vertex color would be invisible.
    assert_eq!(
        VertexColorMode::from_property(1, 0),
        VertexColorMode::Emissive
    );
}

#[test]
fn lighting_e_with_ignore_stays_ignore() {
    // SOURCE_IGNORE + any lighting_mode: vertex color disabled at
    // the source, stays Ignore.
    assert_eq!(
        VertexColorMode::from_property(0, 0),
        VertexColorMode::Ignore
    );
    assert_eq!(
        VertexColorMode::from_property(0, 1),
        VertexColorMode::Ignore
    );
}

#[test]
fn lighting_e_a_d_default_keeps_source_mode_unchanged() {
    // LIGHTING_E_A_D (= 1, the engine default) is the
    // pre-#694 behaviour — every (source_mode, lighting_mode=1)
    // pair must still decode to its source_mode component. Guards
    // against the fix accidentally regressing the common case.
    assert_eq!(
        VertexColorMode::from_property(0, 1),
        VertexColorMode::Ignore
    );
    assert_eq!(
        VertexColorMode::from_property(1, 1),
        VertexColorMode::Emissive
    );
    assert_eq!(
        VertexColorMode::from_property(2, 1),
        VertexColorMode::AmbientDiffuse
    );
}

#[test]
fn unknown_lighting_mode_treated_as_default_e_a_d() {
    // The packed-flags decoder emits `lighting_mode = 0 | 1` only
    // (it's a 1-bit field on FO3+), but pre-10.0.5 streams read a
    // raw u32 — defensive guard that anything other than 0 keeps
    // the LIGHTING_E_A_D semantics so corrupt bytes don't silently
    // hide vertex colors.
    assert_eq!(
        VertexColorMode::from_property(2, 0xFFFF_FFFF),
        VertexColorMode::AmbientDiffuse
    );
}

#[test]
fn vertex_color_mode_unknown_falls_back_to_default() {
    // Gamebryo uses values > 2 in some test/mod content — fall back
    // to AmbientDiffuse instead of a hard error.
    assert_eq!(
        VertexColorMode::from_source_mode(99),
        VertexColorMode::AmbientDiffuse
    );
}

#[test]
fn vertex_color_mode_repr_u8_matches_gamebryo_source_mode() {
    // Pin the discriminant layout — `Ignore=0, Emissive=1,
    // AmbientDiffuse=2` matches Gamebryo's nif.xml `SourceMode`
    // enum. ImportedMesh stores this as u8 via `as u8` cast and
    // downstream consumers compare against literal 0/1/2.
    assert_eq!(VertexColorMode::Ignore as u8, 0);
    assert_eq!(VertexColorMode::Emissive as u8, 1);
    assert_eq!(VertexColorMode::AmbientDiffuse as u8, 2);
}

#[test]
fn default_material_info_has_no_dark_map() {
    let info = MaterialInfo::default();
    assert!(info.dark_map.is_none(), "dark_map should default to None");
}

#[test]
fn default_material_info_has_no_secondary_maps_and_default_mode() {
    let info = MaterialInfo::default();
    assert!(info.glow_map.is_none());
    assert!(info.detail_map.is_none());
    assert!(info.gloss_map.is_none());
    assert_eq!(info.vertex_color_mode, VertexColorMode::AmbientDiffuse);
}

/// Regression: #438 — `MaterialInfo.diffuse_color` must default to
/// white so meshes without a `NiMaterialProperty` fall back to
/// `[1.0, 1.0, 1.0]` vertex tinting (the pre-#438 hardcoded
/// fallback inside `extract_vertex_colors`).
#[test]
fn default_material_info_diffuse_color_is_white() {
    let info = MaterialInfo::default();
    assert_eq!(info.diffuse_color, [1.0, 1.0, 1.0]);
}

/// Regression: #221 — `MaterialInfo.ambient_color` must default to
/// `[1.0; 3]` so the per-material ambient modulator is identity
/// when no `NiMaterialProperty` is bound. Every BSShader-only
/// Skyrim+/FO4 mesh hits this default — a non-`[1.0; 3]` default
/// would attenuate every modern-game cell's ambient.
#[test]
fn default_material_info_ambient_color_is_white() {
    let info = MaterialInfo::default();
    assert_eq!(info.ambient_color, [1.0, 1.0, 1.0]);
}

// ── #695 / O4-03 regression guards ──────────────────────────────────
//
// `NiVertexColorProperty.vertex_mode = SOURCE_EMISSIVE` (1) routes
// per-vertex colors through the fragment shader's emissive
// accumulator. Pre-fix `extract_vertex_colors` only honored
// `AmbientDiffuse` (2) and silently dropped the authored emissive
// payload on every flickering torch / glowing sign / baked emissive
// card by returning the per-material diffuse-color fill instead.

use super::walker::surfaces_authored_vertex_colors;

#[test]
fn emissive_mode_with_colors_surfaces_authored_data() {
    // The fix: SOURCE_EMISSIVE meshes carry the authored vertex-color
    // emissive payload through to the renderer, where the
    // `MAT_FLAG_VERTEX_COLOR_EMISSIVE` bit on `GpuMaterial.material_flags`
    // routes it into the emissive accumulator.
    assert!(surfaces_authored_vertex_colors(
        VertexColorMode::Emissive,
        /* has_authored_colors */ true
    ));
}

#[test]
fn ambient_diffuse_mode_with_colors_surfaces_authored_data() {
    // Existing AmbientDiffuse path (the only mode that surfaced
    // pre-#695) — must still pass to avoid regressing baked AO,
    // hair-tip cards, eyelash strips, BSEffectShader meshes (#618).
    assert!(surfaces_authored_vertex_colors(
        VertexColorMode::AmbientDiffuse,
        true
    ));
}

#[test]
fn ignore_mode_falls_back_to_diffuse_constant_even_with_colors() {
    // SOURCE_IGNORE: the property explicitly disables vertex color
    // contribution. Even when the data block ships an authored array,
    // it must NOT reach the renderer.
    assert!(!surfaces_authored_vertex_colors(
        VertexColorMode::Ignore,
        true
    ));
}

#[test]
fn empty_vertex_colors_always_falls_back_regardless_of_mode() {
    // No authored colors → diffuse-constant fill in every mode.
    // The per-vertex emissive payload only exists when the data block
    // actually shipped colors; a Mesh + Emissive property with no
    // colors should not synthesize anything.
    for mode in [
        VertexColorMode::Ignore,
        VertexColorMode::Emissive,
        VertexColorMode::AmbientDiffuse,
    ] {
        assert!(!surfaces_authored_vertex_colors(
            mode, /* has_authored_colors */ false
        ));
    }
}
