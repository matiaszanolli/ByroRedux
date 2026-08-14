//! Tests for NPC spawning.
//!
//! Moved out of `npc_spawn.rs` under #2198 alongside the `ai_package`
//! extraction: the dispatcher split alone left the file at 2504 LOC, still
//! over the 2000 threshold, because 656 of those lines were this module.
//! Sibling-file tests are the convention elsewhere in the tree
//! (`asset_provider/tests.rs`, `bsa/archive/tests.rs`,
//! `papyrus_demo/quest_advance/tests.rs`). Bodies are unchanged.

use super::*;
use byroredux_core::ecs::components::{
    EscortBehavior, FollowBehavior, GuardBehavior, PatrolBehavior, SandboxBehavior, TravelBehavior,
    WanderBehavior,
};

// ── M41.5 Phase A — idle desync + pool selection ──────────────────

#[test]
fn idle_desync_is_deterministic_per_form_id() {
    // Same FormId → identical seed every call (save/reload + cell
    // re-stream must re-derive the same phase).
    let a = idle_desync(0x0010_A2F3, 2.0);
    let b = idle_desync(0x0010_A2F3, 2.0);
    assert_eq!(a, b);
}

#[test]
fn idle_desync_phase_in_range_and_varies() {
    let duration = 2.0_f32;
    // Sequential FormIds (Bethesda hands them out in runs) must not
    // collapse to near-identical phases.
    let (p0, s0) = idle_desync(0x0100_0001, duration);
    let (p1, s1) = idle_desync(0x0100_0002, duration);
    let (p2, s2) = idle_desync(0x0100_0003, duration);
    for p in [p0, p1, p2] {
        assert!(p >= 0.0 && p < duration, "phase {p} out of [0,{duration})");
    }
    for s in [s0, s1, s2] {
        assert!(
            (s - 1.0).abs() <= IDLE_SPEED_JITTER + 1e-6,
            "speed {s} outside ±{IDLE_SPEED_JITTER}",
        );
    }
    // Adjacent ids diverge in phase (avalanche hash, not sequential).
    assert!((p0 - p1).abs() > 1e-4 && (p1 - p2).abs() > 1e-4);
}

#[test]
fn idle_desync_zero_duration_yields_zero_phase() {
    // Empty / released clip slot: no phase, speed still jittered.
    let (phase, speed) = idle_desync(0xDEAD_BEEF, 0.0);
    assert_eq!(phase, 0.0);
    assert!((speed - 1.0).abs() <= IDLE_SPEED_JITTER + 1e-6);
}

#[test]
fn pick_idle_handle_empty_pool_is_none() {
    assert_eq!(pick_idle_handle(&[], 0x1234), None);
}

#[test]
fn pick_idle_handle_size_one_always_resolves() {
    // Today's floor: the size-1 pool always yields the single handle
    // regardless of FormId.
    assert_eq!(pick_idle_handle(&[7], 0x0001), Some(7));
    assert_eq!(pick_idle_handle(&[7], 0xF00D_BABE), Some(7));
}

#[test]
fn pick_idle_handle_is_deterministic_and_in_bounds() {
    // Forward-compatible: a >1 pool selects stably and in-range.
    let pool = [10_u32, 11, 12, 13];
    for id in [0x01_u32, 0x02, 0xABCD, 0x0010_A2F3] {
        let h = pick_idle_handle(&pool, id).unwrap();
        assert!(pool.contains(&h));
        assert_eq!(pick_idle_handle(&pool, id), Some(h), "stable per id");
    }
}

#[test]
fn prebaked_facegen_nif_path_matches_vanilla_layout() {
    // Vanilla SSE Whiterun Mikael (FormID 0x00013BBE in
    // Skyrim.esm). Path scheme verified by BSA scan 2026-04-28.
    assert_eq!(
        prebaked_facegen_nif_path("Skyrim.esm", 0x00013BBE),
        Some(r"meshes\actors\character\facegendata\facegeom\skyrim.esm\00013bbe.nif".to_string(),),
    );
    // Plugin name is lower-cased; FormID rendered as 8 lowercase hex.
    assert_eq!(
        prebaked_facegen_nif_path("Dawnguard.esm", 0x0001684C),
        Some(
            r"meshes\actors\character\facegendata\facegeom\dawnguard.esm\0001684c.nif".to_string(),
        ),
    );
}

#[test]
fn prebaked_facegen_tint_path_mirrors_geom_layout() {
    assert_eq!(
        prebaked_facegen_tint_path("Skyrim.esm", 0x00013BBE),
        Some(r"textures\actors\character\facegendata\facetint\skyrim.esm\00013bbe.dds".to_string(),),
    );
}

#[test]
fn prebaked_paths_reject_empty_plugin() {
    assert!(prebaked_facegen_nif_path("", 0x42).is_none());
    assert!(prebaked_facegen_tint_path("", 0x42).is_none());
}

#[test]
fn facegen_sidecar_path_swaps_extension() {
    assert_eq!(
        facegen_sidecar_path(r"meshes\characters\head\headhuman.nif", "egm"),
        Some(r"meshes\characters\head\headhuman.egm".to_string()),
    );
    // Mixed-case suffix still matches.
    assert_eq!(
        facegen_sidecar_path(r"Characters\Head\HeadHuman.NIF", "egt"),
        Some(r"Characters\Head\HeadHuman.egt".to_string()),
    );
    // Wrong extension → None.
    assert!(facegen_sidecar_path(r"foo\bar\baz.dds", "egm").is_none());
}

#[test]
fn gender_decodes_acbs_bit_0() {
    assert_eq!(Gender::from_acbs_flags(0), Gender::Male);
    assert_eq!(Gender::from_acbs_flags(0x0000_0001), Gender::Female);
    // High bits unrelated to gender; bit 0 is the only authority.
    assert_eq!(Gender::from_acbs_flags(0xFFFF_FFFE), Gender::Male);
    assert_eq!(Gender::from_acbs_flags(0xFFFF_FFFF), Gender::Female);
}

#[test]
fn skeleton_path_per_game() {
    assert_eq!(
        humanoid_skeleton_path(GameKind::Fallout3NV),
        Some(r"meshes\characters\_male\skeleton.nif"),
    );
    // Skyrim alone uses the space-separated `character assets`
    // folder. Bethesda compressed it to `characterassets` for
    // FO4 onward; the function's docstring carries the BA2-scan
    // evidence (2026-05-26).
    assert_eq!(
        humanoid_skeleton_path(GameKind::Skyrim),
        Some(r"meshes\actors\character\character assets\skeleton.nif"),
    );
    assert_eq!(
        humanoid_skeleton_path(GameKind::Fallout4),
        Some(r"meshes\actors\character\characterassets\skeleton.nif"),
    );
    assert_eq!(
        humanoid_skeleton_path(GameKind::Fallout76),
        Some(r"meshes\actors\character\characterassets\skeleton.nif"),
    );
    // Starfield humanoids live under `\human\`, not `\character\`.
    assert_eq!(
        humanoid_skeleton_path(GameKind::Starfield),
        Some(r"meshes\actors\human\characterassets\skeleton.nif"),
    );
}

/// Regression test for #793: kf-era humanoids must surface
/// `lefthand.nif` and `righthand.nif` alongside `upperbody.nif`.
/// Pre-fix the resolver returned a single path and every NPC
/// rendered handless because the hand mesh was never loaded.
#[test]
fn body_paths_kf_era_include_separate_hand_meshes() {
    for game in [GameKind::Oblivion, GameKind::Fallout3NV] {
        let paths = humanoid_body_paths(game);
        assert_eq!(
            paths.len(),
            3,
            "{game:?} should ship upperbody + 2 hands, got {paths:?}",
        );
        assert!(
            paths.iter().any(|p| p.ends_with("upperbody.nif")),
            "{game:?} missing upperbody: {paths:?}",
        );
        assert!(
            paths.iter().any(|p| p.ends_with("lefthand.nif")),
            "{game:?} missing lefthand: {paths:?}",
        );
        assert!(
            paths.iter().any(|p| p.ends_with("righthand.nif")),
            "{game:?} missing righthand: {paths:?}",
        );
    }
}

/// Skyrim+/FO4+ stand on the pre-baked-FaceGen track — head + body
/// ship in one per-NPC `facegeom.nif`. The body resolver must
/// return an empty slice for those variants so the NPC-spawn loop
/// no-ops on body load and lets the FaceGen path (Phase 4) fill in.
#[test]
fn body_paths_facegen_era_returns_empty_slice() {
    for game in [
        GameKind::Skyrim,
        GameKind::Fallout4,
        GameKind::Fallout76,
        GameKind::Starfield,
    ] {
        let paths = humanoid_body_paths(game);
        assert!(
            paths.is_empty(),
            "{game:?} should defer body to FaceGen path, got {paths:?}",
        );
    }
}

#[test]
fn idle_kf_path_only_for_kf_era_games() {
    // FNV / FO3 ship `.kf` clips.
    assert!(humanoid_default_idle_kf_path(GameKind::Fallout3NV).is_some());
    assert!(humanoid_default_idle_kf_path(GameKind::Oblivion).is_some());

    // Skyrim+ uses Havok `.hkx` — no `.kf` exists in vanilla.
    // Verified by BSA scan 2026-04-28 (Skyrim SE Meshes0 + Meshes1
    // + Animations BSAs all return 0 `.kf` hits).
    assert!(humanoid_default_idle_kf_path(GameKind::Skyrim).is_none());
    assert!(humanoid_default_idle_kf_path(GameKind::Fallout4).is_none());
    assert!(humanoid_default_idle_kf_path(GameKind::Fallout76).is_none());
    assert!(humanoid_default_idle_kf_path(GameKind::Starfield).is_none());
}

// ── #1658 (SKY-D3-02): prebaked equip state routes inventory through
//    the TPLT chain, identical to the kf-era path. ──────────────────

/// Minimal `NpcRecord` for the equip-state tests (mirrors the 21-field
/// shape; callers tweak template / inventory fields).
fn test_npc(form_id: u32, edid: &str) -> NpcRecord {
    NpcRecord {
        form_id,
        editor_id: edid.to_string(),
        full_name: String::new(),
        model_path: String::new(),
        race_form_id: 0,
        class_form_id: 0,
        voice_form_id: 0,
        factions: Vec::new(),
        inventory: Vec::new(),
        default_outfit: None,
        ai_packages: Vec::new(),
        death_item_form_id: 0,
        level: 1,
        disposition_base: 50,
        acbs_flags: 0,
        has_script: false,
        script_form_id: 0,
        script_instance: None,
        face_morphs: None,
        runtime_facegen: None,
        template_form_id: 0,
        template_flags: 0,
        ..Default::default()
    }
}

/// A known (non-leveled) MISC item so `expand_leveled_form_id` lands
/// the form in the inventory (it only pushes forms present in
/// `index.items` or expandable as an LVLI).
fn misc_item(form_id: u32) -> byroredux_plugin::esm::records::ItemRecord {
    byroredux_plugin::esm::records::ItemRecord {
        form_id,
        common: byroredux_plugin::esm::records::common::CommonItemFields::default(),
        kind: ItemKind::Misc,
    }
}

/// A templated Skyrim NPC with an empty own CNTO and
/// `TEMPLATE_FLAG_USE_INVENTORY` set must inherit its base's gear via
/// the TPLT walk — pre-fix `build_npc_equip_state` read `npc.inventory`
/// directly and the actor spawned naked.
#[test]
fn prebaked_equip_state_inherits_templated_inventory() {
    use byroredux_core::ecs::components::InventoryIndex;
    use byroredux_plugin::equip::TEMPLATE_FLAG_USE_INVENTORY;
    use byroredux_plugin::esm::records::NpcInventoryEntry;

    const TEMPLATE: u32 = 0x0100_0001;
    const BASE: u32 = 0x0100_0002;
    const GEAR: u32 = 0x0000_AAAA;

    let mut base = test_npc(BASE, "BaseTemplatedNpc");
    base.inventory.push(NpcInventoryEntry {
        item_form_id: GEAR,
        count: 1,
    });

    let mut templated = test_npc(TEMPLATE, "LvlTemplatedNpc");
    templated.template_form_id = BASE;
    templated.template_flags = TEMPLATE_FLAG_USE_INVENTORY;

    let mut index = EsmIndex {
        game: GameKind::Skyrim,
        ..Default::default()
    };
    index.npcs.insert(BASE, base);
    index.items.insert(GEAR, misc_item(GEAR));

    let state = build_npc_equip_state(&templated, &index, GameKind::Skyrim, Gender::Male);

    assert_eq!(
        state.inventory.len(),
        1,
        "templated NPC must inherit its base's CNTO via TPLT (#1658) — \
         pre-fix the inventory was empty (naked actor)"
    );
    assert_eq!(
        state
            .inventory
            .get(InventoryIndex(0))
            .map(|s| s.base_form_id),
        Some(GEAR),
        "the inherited gear form must be the one that landed in the inventory"
    );
}

/// Control: an NPC with its own CNTO and no template still equips from
/// its own inventory (the named Bannered Mare NPCs the audit flagged as
/// unaffected). `resolve_inherited_inventory` returns the own inventory
/// when no template applies, so this path is unchanged.
#[test]
fn prebaked_equip_state_uses_own_inventory_without_template() {
    use byroredux_core::ecs::components::InventoryIndex;
    use byroredux_plugin::esm::records::NpcInventoryEntry;

    const NPC: u32 = 0x0100_0003;
    const GEAR: u32 = 0x0000_BBBB;

    let mut npc = test_npc(NPC, "OwnInventoryNpc");
    npc.inventory.push(NpcInventoryEntry {
        item_form_id: GEAR,
        count: 1,
    });

    let mut index = EsmIndex {
        game: GameKind::Skyrim,
        ..Default::default()
    };
    index.items.insert(GEAR, misc_item(GEAR));

    let state = build_npc_equip_state(&npc, &index, GameKind::Skyrim, Gender::Male);
    assert_eq!(
        state
            .inventory
            .get(InventoryIndex(0))
            .map(|s| s.base_form_id),
        Some(GEAR),
        "no-template NPC equips from its own inventory unchanged"
    );
}

// ── #2093 (SKY-D3-NEW-01) / #2094 (SKY-D3-NEW-02) — RACE.WNAM skin
//    fallback + displaced-mesh exclusion on the prebaked path. ──────

fn skyrim_armor_item(
    form_id: u32,
    biped_flags: u32,
    armatures: Vec<u32>,
) -> byroredux_plugin::esm::records::ItemRecord {
    byroredux_plugin::esm::records::ItemRecord {
        form_id,
        common: byroredux_plugin::esm::records::common::CommonItemFields::default(),
        kind: ItemKind::Armor {
            biped_flags,
            dt: 0.0,
            dr: 0,
            health: 0,
            slot_mask: 0,
            armor_rating_x100: 0,
            armor_type: Some(1),
            armatures,
        },
    }
}

fn arma(form_id: u32, mesh_path: &str) -> byroredux_plugin::esm::records::ArmaRecord {
    byroredux_plugin::esm::records::ArmaRecord {
        form_id,
        editor_id: String::new(),
        biped_flags: 0,
        general_flags: 0,
        dt: 0,
        dr: 0,
        race_form_id: 0,
        male_biped_model: mesh_path.to_string(),
        female_biped_model: mesh_path.to_string(),
        additional_races: Vec::new(),
    }
}

/// The Hulda/Mikael scenario from the audit: OTFT covers only Feet
/// (`0x80`), so without a `WNAM` skin fallback the NPC has zero mesh
/// source for torso/hands. With the fallback wired, the race skin
/// (covering the non-overlapping Torso+Hands bits) and the feet
/// armor both survive into `armor_to_spawn`.
#[test]
fn prebaked_equip_state_falls_back_to_race_skin_for_uncovered_slots() {
    const RACE: u32 = 0x0100_0020;
    const SKIN: u32 = 0x0100_0021;
    const SKIN_ARMA: u32 = 0x0100_0022;
    const FEET: u32 = 0x0100_0023;
    const FEET_ARMA: u32 = 0x0100_0024;
    const TORSO_HANDS: u32 = 0x0004 | 0x0010;
    const FEET_BIT: u32 = 0x0080;

    let mut race = byroredux_plugin::esm::records::RaceRecord {
        form_id: RACE,
        editor_id: String::new(),
        full_name: String::new(),
        description: String::new(),
        skill_bonuses: Vec::new(),
        body_models: Vec::new(),
        head_parts: Vec::new(),
        base_height: (1.0, 1.0),
        base_weight: (1.0, 1.0),
        race_flags: 0,
        base_attributes: None,
        default_hair: None,
        voice_forms: None,
        facegen_main_clamp: None,
        facegen_face_clamp: None,
        race_reactions: Vec::new(),
        default_skin: None,
    };
    race.default_skin = Some(SKIN);

    let mut npc = test_npc(0x0100_0025, "SkinFallbackNpc");
    npc.race_form_id = RACE;
    npc.inventory
        .push(byroredux_plugin::esm::records::NpcInventoryEntry {
            item_form_id: FEET,
            count: 1,
        });

    let mut index = EsmIndex {
        game: GameKind::Skyrim,
        ..Default::default()
    };
    index.races.insert(RACE, race);
    index
        .items
        .insert(SKIN, skyrim_armor_item(SKIN, TORSO_HANDS, vec![SKIN_ARMA]));
    index
        .armor_addons
        .insert(SKIN_ARMA, arma(SKIN_ARMA, r"actors\character\skin.nif"));
    index
        .items
        .insert(FEET, skyrim_armor_item(FEET, FEET_BIT, vec![FEET_ARMA]));
    index
        .armor_addons
        .insert(FEET_ARMA, arma(FEET_ARMA, r"armor\boots\boots.nif"));

    let state = build_npc_equip_state(&npc, &index, GameKind::Skyrim, Gender::Male);

    assert_eq!(
        state.armor_to_spawn.len(),
        2,
        "non-overlapping skin + feet armor must both queue a mesh, got {:?}",
        state
            .armor_to_spawn
            .iter()
            .map(|a| a.model_path)
            .collect::<Vec<_>>()
    );
    assert!(
        state
            .armor_to_spawn
            .iter()
            .any(|a| a.model_path == r"actors\character\skin.nif"),
        "race skin must fall back to cover the torso/hands OTFT/CNTO left uncovered"
    );
    assert!(
        state
            .armor_to_spawn
            .iter()
            .any(|a| a.model_path == r"armor\boots\boots.nif"),
        "the actually-equipped feet armor must still spawn"
    );
}

/// #2094 (SKY-D3-NEW-02) — when the equipped gear fully overlaps the
/// race skin's biped bit, the skin is displaced and must NOT spawn a
/// second (z-fighting) mesh alongside the winner.
#[test]
fn prebaked_equip_state_drops_skin_mesh_fully_displaced_by_gear() {
    const RACE: u32 = 0x0100_0030;
    const SKIN: u32 = 0x0100_0031;
    const SKIN_ARMA: u32 = 0x0100_0032;
    const TORSO: u32 = 0x0100_0033;
    const TORSO_ARMA: u32 = 0x0100_0034;
    const TORSO_BIT: u32 = 0x0004;

    let mut race = byroredux_plugin::esm::records::RaceRecord {
        form_id: RACE,
        editor_id: String::new(),
        full_name: String::new(),
        description: String::new(),
        skill_bonuses: Vec::new(),
        body_models: Vec::new(),
        head_parts: Vec::new(),
        base_height: (1.0, 1.0),
        base_weight: (1.0, 1.0),
        race_flags: 0,
        base_attributes: None,
        default_hair: None,
        voice_forms: None,
        facegen_main_clamp: None,
        facegen_face_clamp: None,
        race_reactions: Vec::new(),
        default_skin: None,
    };
    race.default_skin = Some(SKIN);

    let mut npc = test_npc(0x0100_0035, "SkinDisplacedNpc");
    npc.race_form_id = RACE;
    npc.inventory
        .push(byroredux_plugin::esm::records::NpcInventoryEntry {
            item_form_id: TORSO,
            count: 1,
        });

    let mut index = EsmIndex {
        game: GameKind::Skyrim,
        ..Default::default()
    };
    index.races.insert(RACE, race);
    index
        .items
        .insert(SKIN, skyrim_armor_item(SKIN, TORSO_BIT, vec![SKIN_ARMA]));
    index
        .armor_addons
        .insert(SKIN_ARMA, arma(SKIN_ARMA, r"actors\character\skin.nif"));
    index
        .items
        .insert(TORSO, skyrim_armor_item(TORSO, TORSO_BIT, vec![TORSO_ARMA]));
    index
        .armor_addons
        .insert(TORSO_ARMA, arma(TORSO_ARMA, r"armor\robe\robe.nif"));

    let state = build_npc_equip_state(&npc, &index, GameKind::Skyrim, Gender::Male);

    assert_eq!(
        state.armor_to_spawn.len(),
        1,
        "the skin's fully-overlapping bit must be displaced, leaving only the winner, got {:?}",
        state
            .armor_to_spawn
            .iter()
            .map(|a| a.model_path)
            .collect::<Vec<_>>()
    );
    assert_eq!(state.armor_to_spawn[0].model_path, r"armor\robe\robe.nif");
}

// ── #2052 / TD1-003 — `apply_ai_package_behavior` shared helper ───
//
// Extracted out of `spawn_npc_entity` and now also called by
// `spawn_prebaked_npc_entity` (which previously had no AI-package
// gating at all — the SIBLING gap the issue flagged). Needs only a
// `World` + `NpcRecord` + `EsmIndex`, no Vulkan device, so it's
// testable in isolation unlike the two spawn functions themselves.

fn pack_with_procedure(
    form_id: u32,
    procedure_type: u32,
) -> byroredux_plugin::esm::records::PackRecord {
    byroredux_plugin::esm::records::PackRecord {
        form_id,
        procedure_type,
        ..Default::default()
    }
}

#[test]
fn apply_ai_package_behavior_tags_sandbox_from_active_package() {
    let mut world = World::new();
    let placement_root = world.spawn();
    let npc = NpcRecord {
        ai_packages: vec![0xAAAA],
        ..Default::default()
    };
    let mut index = EsmIndex::default();
    index.packages.insert(
        0xAAAA,
        pack_with_procedure(
            0xAAAA,
            byroredux_plugin::esm::records::misc::pack::PROCEDURE_SANDBOX,
        ),
    );

    apply_ai_package_behavior(&mut world, placement_root, &npc, &index);

    assert!(
        world.get::<SandboxBehavior>(placement_root).is_some(),
        "active Sandbox package must tag SandboxBehavior"
    );
    assert!(world.get::<WanderBehavior>(placement_root).is_none());
    assert!(world.get::<TravelBehavior>(placement_root).is_none());
}

#[test]
fn apply_ai_package_behavior_tags_travel_with_location_from_active_package() {
    let mut world = World::new();
    let placement_root = world.spawn();
    let npc = NpcRecord {
        form_id: 0x1234,
        ai_packages: vec![0xBBBB],
        ..Default::default()
    };
    let mut pk = pack_with_procedure(
        0xBBBB,
        byroredux_plugin::esm::records::misc::pack::PROCEDURE_TRAVEL,
    );
    pk.location = Some(byroredux_plugin::esm::records::PackLocation {
        location_type: 0,
        target: byroredux_plugin::esm::records::PackLocationTarget::NearReference(0xC0FF_EE00),
        radius: 256,
    });
    let mut index = EsmIndex::default();
    index.packages.insert(0xBBBB, pk);

    apply_ai_package_behavior(&mut world, placement_root, &npc, &index);

    let travel = world
        .get::<TravelBehavior>(placement_root)
        .expect("active Travel package must tag TravelBehavior");
    assert_eq!(travel.radius, Some(256.0));
    assert_eq!(travel.target_form_id, Some(0xC0FF_EE00));
    assert_eq!(travel.form_id, 0x1234);
    assert!(world.get::<SandboxBehavior>(placement_root).is_none());
}

/// No `ai_packages` at all → no active package → no Behavior
/// component of any kind gets tagged. Mirrors the pre-#2052 pre-baked
/// path's behavior for NPCs with no packages, and confirms the
/// shared helper doesn't tag anything speculatively.
#[test]
fn apply_ai_package_behavior_tags_nothing_without_ai_packages() {
    let mut world = World::new();
    let placement_root = world.spawn();
    let npc = NpcRecord::default(); // ai_packages: vec![]
    let index = EsmIndex::default();

    apply_ai_package_behavior(&mut world, placement_root, &npc, &index);

    assert!(world.get::<SandboxBehavior>(placement_root).is_none());
    assert!(world.get::<WanderBehavior>(placement_root).is_none());
    assert!(world.get::<TravelBehavior>(placement_root).is_none());
    assert!(world.get::<FollowBehavior>(placement_root).is_none());
    assert!(world.get::<EscortBehavior>(placement_root).is_none());
    assert!(world.get::<GuardBehavior>(placement_root).is_none());
    assert!(world.get::<PatrolBehavior>(placement_root).is_none());
}

// ── #2567 (OBL-D3-01) — creature asset-path derivation ────────────

/// The creature path rules, on the exact shape `Oblivion.esm` authors:
/// `MODL` is the skeleton, `NIFZ` entries are bare filenames beside it.
#[test]
fn creature_paths_derive_from_the_modl_directory() {
    let (skeleton, dir) = creature_skeleton_and_dir(r"Creatures\Rat\Skeleton.NIF")
        .expect("a MODL with a directory resolves");
    assert_eq!(skeleton, r"meshes\Creatures\Rat\Skeleton.NIF");
    assert_eq!(dir, r"meshes\Creatures\Rat\");

    let parts = creature_body_paths(
        &dir,
        &[
            "Rat.NIF".to_string(),
            "Head.NIF".to_string(),
            "Whiskers.NIF".to_string(),
        ],
    );
    assert_eq!(
        parts,
        vec![
            r"meshes\Creatures\Rat\Rat.NIF".to_string(),
            r"meshes\Creatures\Rat\Head.NIF".to_string(),
            r"meshes\Creatures\Rat\Whiskers.NIF".to_string(),
        ],
        "NIFZ entries are authored bare and resolve against the MODL directory"
    );
    assert_eq!(creature_idle_kf_path(&dir), r"meshes\Creatures\Rat\idle.kf");
}

/// A creature whose MODL is already archive-prefixed must not gain a second
/// `meshes\`, and one with no MODL at all must decline rather than derive
/// paths from an empty prefix (which would produce `meshes\` + filename and
/// silently look up the wrong files).
#[test]
fn creature_path_derivation_is_idempotent_and_declines_without_a_modl() {
    let (skeleton, dir) = creature_skeleton_and_dir(r"meshes\Creatures\Rat\Skeleton.NIF").unwrap();
    assert_eq!(skeleton, r"meshes\Creatures\Rat\Skeleton.NIF");
    assert_eq!(dir, r"meshes\Creatures\Rat\");

    assert!(creature_skeleton_and_dir("").is_none());
    // Forward slashes (mod tooling) split on the same rule.
    let (_, dir) = creature_skeleton_and_dir("Creatures/Rat/Skeleton.NIF").unwrap();
    assert_eq!(dir, r"meshes\Creatures/Rat/");
}

/// #2567 corpus check: every path this module derives for a real Oblivion
/// creature must actually exist in the shipped archive. This is what
/// separates "routes through the actor pipeline" from "routes there and
/// loads nothing" — the audit's suggested fix (check both maps, reuse the
/// humanoid recipe) would have passed a routing test while spawning a human
/// torso for a rat.
///
/// Self-skips without the game installed, like the other corpus sweeps in
/// this tree; set `BYROREDUX_OBLIVION_DATA` to override the path.
#[test]
fn installed_oblivion_creature_assets_resolve_from_their_records() {
    let data = std::env::var("BYROREDUX_OBLIVION_DATA")
        .unwrap_or_else(|_| "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data".to_string());
    let esm_path = std::path::Path::new(&data).join("Oblivion.esm");
    let Ok(esm_bytes) = std::fs::read(&esm_path) else {
        eprintln!(
            "skipping installed_oblivion_creature_assets_resolve_from_their_records: \
             {esm_path:?} not available"
        );
        return;
    };
    // EVERY mesh archive, not just the base one: Shivering Isles creatures
    // (Grummite / Elytra / Hunger) live in `DLCShiveringIsles - Meshes.bsa`,
    // and the engine has them all open per load order. Checking one archive
    // would report a two-thirds hit rate for a derivation that is correct.
    let archives: Vec<byroredux_bsa::BsaArchive> = std::fs::read_dir(&data)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.to_string_lossy()
                .to_ascii_lowercase()
                .ends_with("meshes.bsa")
        })
        .filter_map(|path| byroredux_bsa::BsaArchive::open(&path).ok())
        .collect();
    if archives.is_empty() {
        eprintln!(
            "skipping installed_oblivion_creature_assets_resolve_from_their_records: \
             no mesh archives under {data:?}"
        );
        return;
    }
    let archive = |path: &str| archives.iter().any(|a| a.contains(path));
    let index = byroredux_plugin::esm::parse_esm(&esm_bytes).expect("parse Oblivion.esm");
    assert!(
        !index.creatures.is_empty(),
        "Oblivion.esm must yield CREA records"
    );

    let mut skeletons = (0usize, 0usize);
    let mut parts = (0usize, 0usize);
    let mut idles = (0usize, 0usize);
    let mut misses = Vec::new();
    for record in index.creatures.values() {
        assert!(
            record.is_creature,
            "{:08X} came from the CREA group and must be flagged",
            record.form_id
        );
        let Some((skeleton, dir)) = creature_skeleton_and_dir(&record.model_path) else {
            continue;
        };
        if archive(&skeleton) {
            skeletons.0 += 1;
        } else {
            skeletons.1 += 1;
            if misses.len() < 8 {
                misses.push(format!("skeleton {skeleton}"));
            }
        }
        for path in creature_body_paths(&dir, &record.body_part_models) {
            if archive(&path) {
                parts.0 += 1;
            } else {
                parts.1 += 1;
                if misses.len() < 8 {
                    misses.push(format!("part {path}"));
                }
            }
        }
        if archive(&creature_idle_kf_path(&dir)) {
            idles.0 += 1;
        } else {
            idles.1 += 1;
        }
    }

    eprintln!(
        "Oblivion creatures: skeletons {}/{} found, NIFZ parts {}/{} found, idle.kf {}/{} found",
        skeletons.0,
        skeletons.0 + skeletons.1,
        parts.0,
        parts.0 + parts.1,
        idles.0,
        idles.0 + idles.1,
    );
    // The derivation is the thing under test, so with every mesh archive open
    // it has to be right for essentially all of them. A residual few point at
    // meshes shipped only with a DLC plugin the base install may not have.
    let skeleton_total = skeletons.0 + skeletons.1;
    let part_total = parts.0 + parts.1;
    assert!(
        skeletons.0 * 100 >= skeleton_total * 90,
        "only {}/{skeleton_total} creature skeletons resolved: {misses:?}",
        skeletons.0
    );
    assert!(
        parts.0 * 100 >= part_total * 90,
        "only {}/{part_total} creature NIFZ parts resolved: {misses:?}",
        parts.0
    );
}
