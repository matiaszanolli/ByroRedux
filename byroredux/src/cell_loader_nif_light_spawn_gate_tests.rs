//! Tests for `nif_light_spawn_gate_tests` extracted from ../cell_loader.rs (refactor stage A).
//!
//! Same qualified path preserved (`nif_light_spawn_gate_tests::FOO`).

//! Regression coverage for #632 / FNV-D3-03 — the ESM-fallback
//! `LightSource` must attach when a NIF authored only zero-colour
//! placeholder lights. Pre-fix `spawn_placed_instances` gated on
//! `nif_lights.is_empty()`; placeholders survived the array but
//! got filtered out at spawn time, leaving the cell dark even
//! when both NIF intent and ESM authority agreed it should be
//! lit. Vulkan-free helpers `is_spawnable_nif_light` /
//! `count_spawnable_nif_lights` carry the predicate the gate
//! consults; testing them here pins the contract without a full
//! cell-load harness.
use super::*;
use byroredux_nif::import::{ImportedLight, LightKind};

fn light_with_color(rgb: [f32; 3]) -> ImportedLight {
    ImportedLight {
        translation: [0.0, 0.0, 0.0],
        direction: [0.0, 0.0, 0.0],
        color: rgb,
        radius: 100.0,
        kind: LightKind::Point,
        outer_angle: 0.0,
        affected_node_names: Vec::new(),
    }
}

/// Pure-zero RGB → not spawnable. The audit's exact case: an
/// authored-off `NiPointLight` placeholder.
#[test]
fn zero_color_light_is_not_spawnable() {
    let placeholder = light_with_color([0.0, 0.0, 0.0]);
    assert!(!is_spawnable_nif_light(&placeholder));
}

/// Just under the `1e-4` threshold — also not spawnable. Locks
/// the boundary so the threshold doesn't drift silently.
#[test]
fn near_zero_color_light_below_threshold_is_not_spawnable() {
    // Sum = 9e-5, below the 1e-4 cutoff.
    let almost = light_with_color([3e-5, 3e-5, 3e-5]);
    assert!(!is_spawnable_nif_light(&almost));
}

/// Any single non-trivial channel → spawnable.
#[test]
fn nonzero_color_light_is_spawnable() {
    let red = light_with_color([0.6, 0.0, 0.0]);
    let green = light_with_color([0.0, 0.4, 0.0]);
    let dim_blue = light_with_color([0.0, 0.0, 0.001]); // sum = 1e-3 > 1e-4
    assert!(is_spawnable_nif_light(&red));
    assert!(is_spawnable_nif_light(&green));
    assert!(is_spawnable_nif_light(&dim_blue));
}

/// The audit's headline scenario: a NIF carrying ONE
/// zero-colour placeholder. `nif_lights.is_empty()` returns
/// `false` (there's an entry in the array), but
/// `count_spawnable_nif_lights` returns 0 — so the ESM-fallback
/// gate fires and the LIGH-authored colour reaches the cell.
#[test]
fn placeholder_only_array_counts_zero_so_esm_fallback_fires() {
    let nif_lights = vec![light_with_color([0.0, 0.0, 0.0])];
    // Pre-#632 logic would have looked at `nif_lights.is_empty()`
    // here and seen `false`, blocking the fallback.
    assert!(!nif_lights.is_empty());
    // Post-#632 the gate consults the predicate-based count and
    // sees zero spawnable lights, allowing the ESM fallback.
    assert_eq!(count_spawnable_nif_lights(&nif_lights), 0);
}

/// Mixed array: a real light + a placeholder → count = 1
/// (only the real light spawns). ESM fallback DOESN'T fire
/// because `count > 0` — the NIF already supplied a real light.
#[test]
fn mixed_real_and_placeholder_counts_only_the_real_one() {
    let nif_lights = vec![
        light_with_color([0.5, 0.5, 0.5]), // real
        light_with_color([0.0, 0.0, 0.0]), // placeholder
    ];
    assert_eq!(count_spawnable_nif_lights(&nif_lights), 1);
}

/// Empty array (truly no NIF lights) → count = 0, ESM
/// fallback fires. Locks the no-regression case for cells that
/// rely on the legacy gate.
#[test]
fn empty_array_counts_zero() {
    let nif_lights: Vec<ImportedLight> = Vec::new();
    assert_eq!(count_spawnable_nif_lights(&nif_lights), 0);
}

// ── RT-9 / #672 radius-zero sanitisation ──────────────────────────

/// Authored Bethesda XCLL radii are 256–4096 units; any positive
/// value here must pass through unchanged. Locks the "ground
/// truth from the level designer is preserved" half of the
/// contract.
#[test]
fn light_radius_or_default_passes_positive_radii_through() {
    assert_eq!(super::light_radius_or_default(256.0), 256.0);
    assert_eq!(super::light_radius_or_default(1024.0), 1024.0);
    assert_eq!(super::light_radius_or_default(4096.0), 4096.0);
    // Sub-unit positive — still preserved. The shader's
    // `radius * 0.025 = 0.025u` disk is degenerate, but that's
    // an authored value and the contract says positives ride
    // through unchanged.
    assert_eq!(super::light_radius_or_default(1.0), 1.0);
}

/// Exact zero — the audit's headline failure mode. A LIGH
/// `DATA` sub-record that ships `radius=0` would otherwise
/// zero the shader's `effectiveRange = radius * 4.0`
/// attenuation window AND collapse the shadow-ray jitter disk
/// to the dead 1.5u floor. Sanitisation kicks the radius up
/// to the existing 4096u cell-scale fallback.
#[test]
fn light_radius_or_default_kicks_zero_to_cell_scale() {
    assert_eq!(super::light_radius_or_default(0.0), 4096.0);
}

/// Negative values are nonsensical (radius is a length) but
/// could arrive from a malformed record's `u32 → f32` reading
/// of a value that overflowed sign somewhere upstream. Treated
/// the same as zero.
#[test]
fn light_radius_or_default_kicks_negative_to_cell_scale() {
    assert_eq!(super::light_radius_or_default(-1.0), 4096.0);
    assert_eq!(super::light_radius_or_default(f32::MIN), 4096.0);
}

/// `f32::NAN` propagates through every comparison as `false`,
/// so the `radius > 0.0` guard rejects it and we fall back to
/// the cell-scale default. Without the guard the shader would
/// see `position_radius.w = NaN` and contaminate the entire
/// lighting reservoir downstream — every comparison against a
/// NaN evaluates to false, so the WRS would silently lose this
/// light AND any ratio-based culling that touched it.
#[test]
fn light_radius_or_default_handles_nan() {
    let result = super::light_radius_or_default(f32::NAN);
    assert_eq!(result, 4096.0);
    assert!(result.is_finite());
}

// ── M46.0 / #561 multi-plugin helpers ─────────────────────────────

#[test]
fn plugin_basename_lc_strips_path_and_lowercases() {
    assert_eq!(super::plugin_basename_lc("Skyrim.esm"), "skyrim.esm");
    assert_eq!(
        super::plugin_basename_lc("/some/abs/Path/Dawnguard.esm"),
        "dawnguard.esm"
    );
    assert_eq!(
        super::plugin_basename_lc("Update.ESM"),
        "update.esm",
        "Bethesda content uses case-insensitive plugin names"
    );
}

#[test]
fn plugin_for_form_id_resolves_top_byte_to_load_order_basename() {
    let load_order = vec![
        "skyrim.esm".to_string(),
        "update.esm".to_string(),
        "dawnguard.esm".to_string(),
    ];
    // Top byte 0 → first plugin in the order.
    assert_eq!(
        super::plugin_for_form_id(0x0001_2345, &load_order),
        Some("skyrim.esm")
    );
    assert_eq!(
        super::plugin_for_form_id(0x0100_BEEF, &load_order),
        Some("update.esm")
    );
    assert_eq!(
        super::plugin_for_form_id(0x0200_DEAD, &load_order),
        Some("dawnguard.esm")
    );
    // Out-of-range mod-index byte (e.g. malformed FormID, or a
    // plugin not in the loaded order) returns None so the
    // diagnostic can mark it as `???` instead of indexing past.
    assert_eq!(
        super::plugin_for_form_id(0xFF00_0000, &load_order),
        None,
        "out-of-range mod-index byte must return None, not panic"
    );
}
