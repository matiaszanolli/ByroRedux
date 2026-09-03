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
        name: None,
    }
}

/// Regression for #2530 / NIFAL-D3-NEW-01: `spawn_nif_lights` (widened
/// to `pub(crate)` so `scene::nif_loader::load_nif_bytes_with_skeleton`
/// — the loose-NIF / NPC-part load path — can call the exact same
/// construction the cell loader uses) must spawn a real `LightSource`
/// entity for a spawnable authored light. `spawn_nif_lights` takes no
/// `VulkanContext`, so this exercises the full parse-to-ECS contract
/// without standing up a GPU device — the one piece of #2530's fix that
/// `load_nif_bytes_with_skeleton` itself can't be unit-tested through
/// (mesh GPU upload requires a real Vulkan device).
#[test]
fn spawn_nif_lights_attaches_light_source_for_spawnable_light() {
    let mut world = World::new();
    let lights = vec![ImportedLight {
        translation: [10.0, 20.0, 30.0],
        direction: [0.0, 0.0, -1.0],
        color: [0.8, 0.2, 0.1],
        radius: 512.0,
        kind: LightKind::Point,
        outer_angle: 0.0,
        affected_node_names: Vec::new(),
        name: None,
    }];

    // No REFR / ESM context (the loose-loader case): identity ref
    // transform, no LightData to prefer a radius from.
    spawn_nif_lights(&mut world, &lights, Vec3::ZERO, Quat::IDENTITY, 1.0, None);

    let q = world.query::<LightSource>().expect("LightSource query");
    let spawned: Vec<_> = q.iter().collect();
    assert_eq!(
        spawned.len(),
        1,
        "spawn_nif_lights must attach exactly one LightSource for the one spawnable light"
    );
    let (_, light_source) = spawned[0];
    assert_eq!(
        light_source.emitter.radiant_intensity.get(),
        [0.8, 0.2, 0.1]
    );
    assert_eq!(
        light_source.emitter.range.to_bethesda_units(),
        512.0,
        "authored radius (no ESM override) must survive"
    );
}

/// #3232 — NiLight direction is NIF-local just like its translation. A
/// placed reference's rotation must carry the cone/directional axis into
/// world space before the canonical emitter is constructed.
#[test]
fn spawn_nif_lights_rotates_direction_by_reference_rotation() {
    let mut world = World::new();
    let lights = vec![ImportedLight {
        translation: [0.0, 0.0, 0.0],
        direction: [1.0, 0.0, 0.0],
        color: [1.0, 1.0, 1.0],
        radius: 512.0,
        kind: LightKind::Directional,
        outer_angle: 0.0,
        affected_node_names: Vec::new(),
        name: None,
    }];
    let quarter_turn = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);

    spawn_nif_lights(&mut world, &lights, Vec3::ZERO, quarter_turn, 1.0, None);

    let q = world.query::<LightSource>().expect("LightSource query");
    let (_, spawned) = q.iter().next().expect("spawned directional light");
    let actual = Vec3::from_array(spawned.emitter.direction);
    let expected = quarter_turn * Vec3::X;
    assert!(
        actual.abs_diff_eq(expected, 1.0e-6),
        "reference rotation must transform NIF-local direction: expected {expected:?}, got {actual:?}"
    );
}

/// Companion: a NIF authoring only zero-colour placeholder lights must
/// spawn NO `LightSource` entity through this path either — same
/// `is_spawnable_nif_light` gate the cell loader's own light spawn
/// already respects.
#[test]
fn spawn_nif_lights_skips_zero_color_placeholder() {
    let mut world = World::new();
    let lights = vec![light_with_color([0.0, 0.0, 0.0])];

    spawn_nif_lights(&mut world, &lights, Vec3::ZERO, Quat::IDENTITY, 1.0, None);

    // `query::<T>()` returns `None` when no entity has EVER had `T` — the
    // expected outcome here, since nothing should spawn at all.
    let count = world
        .query::<LightSource>()
        .map(|q| q.iter().count())
        .unwrap_or(0);
    assert_eq!(
        count, 0,
        "a zero-colour placeholder light must not spawn a LightSource"
    );
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

// ── #3557 (RT-11) — exporter-artifact default-light de-dup ────────

/// The headline regression: two REFRs each placing a mesh that carries
/// an identically-named `__max_default_light` node must yield exactly
/// ONE `LightSource` entity, not two byte-identical ones. Simulated
/// here as two separate `spawn_nif_lights` calls sharing one `World`
/// (matching how `spawn_placed_instances` is invoked once per REFR from
/// the cell loader's REFR loop — see the issue's own evidence).
#[test]
fn spawn_nif_lights_deduplicates_known_exporter_artifact_by_name() {
    let mut world = World::new();
    world.insert_resource(byroredux_core::string::StringPool::new());
    let artifact_light = || ImportedLight {
        translation: [0.0, 0.0, 0.0],
        direction: [0.8947, 0.3716, 0.2478],
        color: [1.0, 1.0, 1.0],
        radius: 512.0,
        kind: LightKind::Directional,
        outer_angle: 0.0,
        affected_node_names: Vec::new(),
        name: Some(std::sync::Arc::from("__max_default_light")),
    };

    // First contributing NIF/REFR.
    spawn_nif_lights(
        &mut world,
        &[artifact_light()],
        Vec3::ZERO,
        Quat::IDENTITY,
        1.0,
        None,
    );
    // Second contributing NIF/REFR — identical artifact light.
    spawn_nif_lights(
        &mut world,
        &[artifact_light()],
        Vec3::ZERO,
        Quat::IDENTITY,
        1.0,
        None,
    );

    let count = world
        .query::<LightSource>()
        .map(|q| q.iter().count())
        .unwrap_or(0);
    assert_eq!(
        count, 1,
        "two REFRs contributing the same synthetic default light must \
         yield one emitter, not double its contribution"
    );
}

/// An ordinary content light sharing a name (however unlikely) across
/// two REFRs must NOT be deduplicated — only names on the exporter-
/// artifact allowlist trigger the #3557 skip.
#[test]
fn spawn_nif_lights_does_not_deduplicate_ordinary_named_lights() {
    let mut world = World::new();
    world.insert_resource(byroredux_core::string::StringPool::new());
    let named_light = || ImportedLight {
        translation: [0.0, 0.0, 0.0],
        direction: [0.0, 0.0, -1.0],
        color: [0.9, 0.6, 0.2],
        radius: 256.0,
        kind: LightKind::Point,
        outer_angle: 0.0,
        affected_node_names: Vec::new(),
        name: Some(std::sync::Arc::from("Torch01Light")),
    };

    spawn_nif_lights(
        &mut world,
        &[named_light()],
        Vec3::ZERO,
        Quat::IDENTITY,
        1.0,
        None,
    );
    spawn_nif_lights(
        &mut world,
        &[named_light()],
        Vec3::ZERO,
        Quat::IDENTITY,
        1.0,
        None,
    );

    let count = world
        .query::<LightSource>()
        .map(|q| q.iter().count())
        .unwrap_or(0);
    assert_eq!(
        count, 2,
        "an ordinary content light name must never be deduplicated, \
         only names on the exporter-artifact allowlist"
    );
}

#[test]
fn known_exporter_artifact_light_name_matches_only_the_documented_name() {
    assert!(is_known_exporter_artifact_light_name("__max_default_light"));
    assert!(!is_known_exporter_artifact_light_name("Torch01Light"));
    assert!(!is_known_exporter_artifact_light_name(""));
    // Case-sensitive on purpose — the allowlist is evidence-bound to the
    // exact confirmed string, not a heuristic.
    assert!(!is_known_exporter_artifact_light_name("__MAX_DEFAULT_LIGHT"));
}

// ── M46.0 / #561 multi-plugin helpers ─────────────────────────────

#[test]
fn plugin_basename_lc_strips_path_and_lowercases() {
    assert_eq!(
        super::load_order::plugin_basename_lc("Skyrim.esm"),
        "skyrim.esm"
    );
    assert_eq!(
        super::load_order::plugin_basename_lc("/some/abs/Path/Dawnguard.esm"),
        "dawnguard.esm"
    );
    assert_eq!(
        super::load_order::plugin_basename_lc("Update.ESM"),
        "update.esm",
        "Bethesda content uses case-insensitive plugin names"
    );
}

#[test]
fn plugin_for_form_id_resolves_top_byte_to_load_order_basename() {
    let load_order = super::load_order::LoadOrder::all_regular(vec![
        "skyrim.esm".to_string(),
        "update.esm".to_string(),
        "dawnguard.esm".to_string(),
    ]);
    // Top byte 0 → first plugin in the order.
    assert_eq!(
        super::load_order::plugin_for_form_id(0x0001_2345, &load_order),
        Some("skyrim.esm")
    );
    assert_eq!(
        super::load_order::plugin_for_form_id(0x0100_BEEF, &load_order),
        Some("update.esm")
    );
    assert_eq!(
        super::load_order::plugin_for_form_id(0x0200_DEAD, &load_order),
        Some("dawnguard.esm")
    );
    // Out-of-range mod-index byte (e.g. malformed FormID, or a
    // plugin not in the loaded order) returns None so the
    // diagnostic can mark it as `???` instead of indexing past.
    assert_eq!(
        super::load_order::plugin_for_form_id(0xFF00_0000, &load_order),
        None,
        "out-of-range mod-index byte must return None, not panic"
    );
}

/// #3366 — with an ESL anywhere but last, load-order *position* and global
/// *slot* diverge, and the diagnostic must follow the slot.
///
/// `allocate_global_slot` draws from two counters: regular plugins take
/// `0x00..=0xFD`, light masters take a 12-bit sub-index in the `0xFE` space. In
/// the order below `_resourcepack.esl` sits third, so `dragonborn.esm` is at
/// position 4 but holds regular slot `0x03`. Pre-fix, indexing the name list by
/// the top byte reported every Dragonborn-owned form as `_resourcepack.esl`,
/// and every ESL-owned form (top byte `0xFE` = 254) fell off the end as `None`
/// — which callers render as `"???"` / `"Engine.esm"`, sending the user to add
/// a master they already have.
///
/// The order is legal: `_ResourcePack.esl` declares
/// `["Skyrim.esm","Update.esm","HearthFires.esm"]` as its masters.
#[test]
fn plugin_for_form_id_follows_global_slots_not_positions_with_an_esl() {
    use byroredux_plugin::esm::reader::GlobalSlot;

    let load_order = super::load_order::LoadOrder::new(
        vec![
            "skyrim.esm".to_string(),
            "update.esm".to_string(),
            "hearthfires.esm".to_string(),
            "_resourcepack.esl".to_string(),
            "dragonborn.esm".to_string(),
        ],
        vec![
            GlobalSlot::Regular(0x00),
            GlobalSlot::Regular(0x01),
            GlobalSlot::Regular(0x02),
            GlobalSlot::Light(0x000),
            // Position 4, but slot 0x03 — the ESL consumed no regular byte.
            GlobalSlot::Regular(0x03),
        ],
    );

    // The regression: top byte 0x03 is Dragonborn, NOT the ESL at position 3.
    // These are real Dragonborn FormIDs / editor IDs from the audit.
    for form_id in [0x0302_8434u32, 0x0303_84C1, 0x0301_85ED] {
        assert_eq!(
            super::load_order::plugin_for_form_id(form_id, &load_order),
            Some("dragonborn.esm"),
            "{form_id:08X} is Dragonborn content; pre-#3366 it was attributed \
             to the ESL sitting at load-order position 3"
        );
    }

    // ESL-owned forms are nameable now instead of reporting None.
    for form_id in [0xFE00_00E4u32, 0xFE00_014D] {
        assert_eq!(
            super::load_order::plugin_for_form_id(form_id, &load_order),
            Some("_resourcepack.esl"),
            "{form_id:08X} lives in the 0xFE light-master space and must resolve \
             to the ESL, not fall off the end of the name list"
        );
    }

    // Plugins before the ESL are unaffected — position and slot still agree.
    assert_eq!(
        super::load_order::plugin_for_form_id(0x0001_2345, &load_order),
        Some("skyrim.esm")
    );
    assert_eq!(
        super::load_order::plugin_for_form_id(0x0200_0001, &load_order),
        Some("hearthfires.esm")
    );

    // A slot nobody holds still returns None so the caller can print `???`.
    assert_eq!(
        super::load_order::plugin_for_form_id(0x7F00_0000, &load_order),
        None,
        "an unallocated regular slot must return None, not panic"
    );
    assert_eq!(
        super::load_order::plugin_for_form_id(0xFE00_F001, &load_order),
        None,
        "an unallocated light sub-index must return None"
    );
}

/// The 12-bit light sub-index is read from bits 12..24, mirroring
/// `GlobalSlot::compose`, so two ESLs are told apart by their sub-index rather
/// than by the shared `0xFE` byte.
#[test]
fn plugin_for_form_id_distinguishes_multiple_esls_by_sub_index() {
    use byroredux_plugin::esm::reader::GlobalSlot;

    let load_order = super::load_order::LoadOrder::new(
        vec![
            "skyrim.esm".to_string(),
            "first.esl".to_string(),
            "second.esl".to_string(),
        ],
        vec![
            GlobalSlot::Regular(0x00),
            GlobalSlot::Light(0x000),
            GlobalSlot::Light(0x001),
        ],
    );

    // compose(Light(0), raw) -> 0xFE000000 | (0 << 12) | raw
    assert_eq!(
        super::load_order::plugin_for_form_id(GlobalSlot::Light(0x000).compose(0x123), &load_order),
        Some("first.esl")
    );
    assert_eq!(
        super::load_order::plugin_for_form_id(GlobalSlot::Light(0x001).compose(0x123), &load_order),
        Some("second.esl")
    );
}
