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
    ActorValues, ActorVitals, Dead, EscortBehavior, FollowBehavior, GuardBehavior, PatrolBehavior,
    SandboxBehavior, TravelBehavior, WanderBehavior,
};

#[test]
fn fnv_spawned_actor_gets_derived_health_and_combat_consumes_it() {
    use byroredux_core::character::CharacterRulesProfile;
    use byroredux_plugin::esm::records::{AvifRecord, ClassRecord};

    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    world.register::<ActorValues>();
    world.register::<ActorVitals>();
    world.register::<EquippedWeapon>();
    world.register::<Dead>();

    let health = 0x450;
    let mut index = EsmIndex {
        character_rules: CharacterRulesProfile::FALLOUT_NEW_VEGAS,
        ..EsmIndex::default()
    };
    index.actor_values.insert(
        0x3EA,
        AvifRecord {
            form_id: 0x3EA,
            editor_id: "AVEndurance".to_owned(),
            ..AvifRecord::default()
        },
    );
    index.actor_values.insert(
        health,
        AvifRecord {
            form_id: health,
            editor_id: "AVHealth".to_owned(),
            ..AvifRecord::default()
        },
    );
    index.classes.insert(
        0x2000,
        ClassRecord {
            form_id: 0x2000,
            base_attributes: [5; 7],
            ..ClassRecord::default()
        },
    );
    let npc = NpcRecord {
        class_form_id: 0x2000,
        level: 1,
        ..NpcRecord::default()
    };

    let target = world.spawn();
    stamp_actor_values(&mut world, target, &npc, &index);
    assert_eq!(world.get::<ActorVitals>(target).unwrap().health, health);
    assert_eq!(
        world.get::<ActorValues>(target).unwrap().current(health),
        200.0
    );

    let aggressor = world.spawn();
    world.insert(
        aggressor,
        EquippedWeapon {
            inventory_index: InventoryIndex(0),
            base_form_id: 0x1CB64,
            damage: 18.0,
            reach: 0.0,
            speed: 0.0,
        },
    );
    world.insert(
        target,
        byroredux_scripting::HitEvent {
            aggressor,
            source: aggressor,
            projectile: 0,
            damage: 18.0,
            power_attack: false,
            sneak_attack: false,
            bash_attack: false,
            blocked: false,
        },
    );

    crate::combat::combat_damage_system(&world, 0.0);
    assert_eq!(
        world.get::<ActorValues>(target).unwrap().current(health),
        182.0
    );
}

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
    // A vanilla SSE `Skyrim.esm` NPC FormID. Path scheme verified by BSA
    // scan 2026-04-28.
    //
    // #3361 — this comment used to name `0x00013BBE` as "Whiterun Mikael".
    // It isn't: Mikael is `0x0001A670` (see `BANNERED_MARE`). The FormID
    // here is arbitrary as far as this test is concerned — it only pins the
    // path *format*, and never opens an ESM — so the value stands; only the
    // false attribution is removed.
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
        let paths = humanoid_body_paths(game, Gender::Male, false);
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

#[test]
fn body_paths_kf_era_select_gender_and_child_variants() {
    assert_eq!(
        humanoid_body_paths(GameKind::Fallout3NV, Gender::Female, false),
        &[
            r"meshes\characters\_male\femaleupperbody.nif",
            r"meshes\characters\_male\femalelefthand.nif",
            r"meshes\characters\_male\femalerighthand.nif",
        ],
    );
    assert_eq!(
        humanoid_body_paths(GameKind::Fallout3NV, Gender::Male, true),
        &[
            r"meshes\characters\_male\childupperbody.nif",
            r"meshes\characters\_male\lefthand.nif",
            r"meshes\characters\_male\righthand.nif",
        ],
    );
    assert_eq!(
        humanoid_body_paths(GameKind::Fallout3NV, Gender::Female, true),
        &[
            r"meshes\characters\_male\childfemaleupperbody.nif",
            r"meshes\characters\_male\femalelefthand.nif",
            r"meshes\characters\_male\femalerighthand.nif",
        ],
    );

    // Oblivion shares FO3/FNV's historical `_male` directory and female
    // filename prefix, but DATA bit 2 means BeastRace there, not Child.
    assert_eq!(
        humanoid_body_paths(GameKind::Oblivion, Gender::Female, false),
        &[
            r"meshes\characters\_male\femaleupperbody.nif",
            r"meshes\characters\_male\femalelefthand.nif",
            r"meshes\characters\_male\femalerighthand.nif",
        ],
    );
}

#[test]
fn kf_body_piece_masks_follow_each_games_hand_layout() {
    assert_eq!(
        humanoid_body_path_biped_mask(GameKind::Fallout3NV, r"x\upperbody.nif"),
        1 << 2
    );
    assert_eq!(
        humanoid_body_path_biped_mask(GameKind::Fallout3NV, r"x\lefthand.nif"),
        1 << 3
    );
    assert_eq!(
        humanoid_body_path_biped_mask(GameKind::Fallout3NV, r"x\righthand.nif"),
        1 << 4
    );
    assert_eq!(
        humanoid_body_path_biped_mask(GameKind::Oblivion, r"x\lefthand.nif"),
        1 << 4
    );
    assert_eq!(
        humanoid_body_path_biped_mask(GameKind::Oblivion, r"x\righthand.nif"),
        1 << 4
    );
}

#[test]
#[ignore = "needs FNV game data on disk; parses the whole master (~850 MB resident)"]
fn installed_fnv_sunny_smiles_selects_the_female_body() {
    let data = std::env::var_os("BYROREDUX_FNV_DATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(
                "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data",
            )
        });
    let esm = data.join("FalloutNV.esm");
    let Ok(bytes) = std::fs::read(&esm) else {
        eprintln!(
            "skipping real-data #3037 regression: {} absent",
            esm.display()
        );
        return;
    };
    let index = byroredux_plugin::esm::parse_esm(&bytes).expect("parse FalloutNV.esm");
    let sunny = index.npcs.get(&0x0010_4E84).expect("GSSunnySmiles NPC_");
    let race = index
        .races
        .get(&sunny.race_form_id)
        .expect("Sunny's Hispanic race");
    let gender = Gender::from_acbs_flags(sunny.acbs_flags);
    let is_child = race.race_flags & 0x04 != 0;

    assert_eq!(gender, Gender::Female);
    assert!(!is_child);
    assert_eq!(
        humanoid_body_paths(GameKind::Fallout3NV, gender, is_child)[0],
        r"meshes\characters\_male\femaleupperbody.nif",
    );
    assert_eq!(
        humanoid_skeleton_path(GameKind::Fallout3NV),
        Some(r"meshes\characters\_male\skeleton.nif"),
        "female bodies retain the shared vanilla skeleton",
    );
}

/// Skyrim+/FO4+ stand on the pre-baked-FaceGen track. Their head comes from
/// per-NPC FaceGen while the body comes from the race's default skin armor,
/// so the KF-era loose-body resolver must return an empty slice.
#[test]
fn body_paths_facegen_era_returns_empty_slice() {
    for game in [
        GameKind::Skyrim,
        GameKind::Fallout4,
        GameKind::Fallout76,
        GameKind::Starfield,
    ] {
        let paths = humanoid_body_paths(game, Gender::Male, false);
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

fn weapon_item(form_id: u32, damage: u32) -> byroredux_plugin::esm::records::ItemRecord {
    byroredux_plugin::esm::records::ItemRecord {
        form_id,
        common: byroredux_plugin::esm::records::common::CommonItemFields::default(),
        kind: ItemKind::Weapon {
            ammo_form: 0,
            damage,
            clip_size: 0,
            anim_type: 0,
            ap_cost: 0.0,
            skill_form: 0,
            min_spread: 0.0,
            spread: 0.0,
            crit_mult: 0.0,
            reach: 0.0,
            speed: 0.0,
            reload_anim: 0,
            vats: None,
        },
    }
}

#[test]
fn prebaked_equip_state_selects_one_highest_damage_weapon() {
    use byroredux_plugin::esm::records::NpcInventoryEntry;

    const NPC: u32 = 0x0100_0010;
    const BATTLEAXE: u32 = 0x0001_CB64;
    const GREATSWORD: u32 = 0x0002_36A5;
    let mut npc = test_npc(NPC, "DraugrWeaponFixture");
    // Deliberately duplicate both candidates, matching the frozen P2 fixture's
    // multiple leveled-list outcomes. Inventory can represent each row, but
    // combat must own exactly one deterministic weapon.
    for form_id in [GREATSWORD, BATTLEAXE, GREATSWORD, BATTLEAXE] {
        npc.inventory.push(NpcInventoryEntry {
            item_form_id: form_id,
            count: 1,
        });
    }
    let mut index = EsmIndex {
        game: GameKind::Skyrim,
        ..Default::default()
    };
    index.items.insert(BATTLEAXE, weapon_item(BATTLEAXE, 18));
    index.items.insert(GREATSWORD, weapon_item(GREATSWORD, 17));

    let state = build_npc_equip_state(&npc, &index, GameKind::Skyrim, Gender::Male);
    let equipped = state.equipped_weapon.expect("one weapon must be equipped");
    assert_eq!(equipped.base_form_id, BATTLEAXE);
    assert_eq!(equipped.damage, 18.0);
    assert_eq!(
        state
            .inventory
            .get(equipped.inventory_index)
            .map(|stack| stack.base_form_id),
        Some(BATTLEAXE),
        "equipped state must point at the winning inventory row"
    );
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

/// #2956 — `stamp_character_components` end-to-end: a templated shell with
/// `Use Stats` set must land `CharacterLevel`/`Background.class_form_id`
/// from the template, and (independently, via `Use Traits`)
/// `Background.race_form_id` from the template too — not the shell's own
/// (deliberately wrong / unresolvable) values.
#[test]
fn stamp_character_components_follows_use_stats_and_use_traits_templates() {
    use byroredux_core::character::{Background, CharacterLevel};
    use byroredux_plugin::equip::{TEMPLATE_FLAG_USE_STATS, TEMPLATE_FLAG_USE_TRAITS};

    const SHELL: u32 = 0x0100_0010;
    const TEMPLATE: u32 = 0x0100_0011;
    const TEMPLATE_CLASS: u32 = 0x0000_C1A5;
    const TEMPLATE_RACE: u32 = 0x0000_2ACE;
    const SHELL_CLASS: u32 = 0xBAD_C1A55; // unresolvable — proves it's ignored
    const SHELL_RACE: u32 = 0xBAD_2ACE; // unresolvable — proves it's ignored

    let mut template = test_npc(TEMPLATE, "BaseTemplatedNpc");
    template.class_form_id = TEMPLATE_CLASS;
    template.race_form_id = TEMPLATE_RACE;
    template.level = 20;

    let mut shell = test_npc(SHELL, "LvlTemplatedNpc");
    shell.class_form_id = SHELL_CLASS;
    shell.race_form_id = SHELL_RACE;
    shell.level = 1;
    shell.template_form_id = TEMPLATE;
    shell.template_flags = TEMPLATE_FLAG_USE_STATS | TEMPLATE_FLAG_USE_TRAITS;

    let mut index = EsmIndex::default();
    index.npcs.insert(TEMPLATE, template);

    let mut world = World::new();
    world.register::<CharacterLevel>();
    world.register::<Background>();
    let target = world.spawn();
    stamp_character_components(&mut world, target, &shell, &index);

    assert_eq!(
        world.get::<CharacterLevel>(target).unwrap().level,
        20,
        "CharacterLevel must come from the Use-Stats template, not the shell"
    );
    let background = world.get::<Background>(target).unwrap();
    assert_eq!(
        background.class_form_id, TEMPLATE_CLASS,
        "Background.class_form_id must come from the Use-Stats template"
    );
    assert_eq!(
        background.race_form_id, TEMPLATE_RACE,
        "Background.race_form_id must come from the Use-Traits template"
    );
}

/// Control: no `template_flags` set → both `CharacterLevel` and
/// `Background` keep the NPC's own values, matching pre-#2956 behavior for
/// the common (untemplated, unique-named) NPC.
#[test]
fn stamp_character_components_uses_own_values_without_template_flags() {
    use byroredux_core::character::{Background, CharacterLevel};

    const NPC: u32 = 0x0100_0012;
    const TEMPLATE: u32 = 0x0100_0013;

    let mut template = test_npc(TEMPLATE, "IgnoredTemplate");
    template.class_form_id = 0xBAD_C1A55;
    template.race_form_id = 0xBAD_2ACE;
    template.level = 99;

    let mut npc = test_npc(NPC, "UniqueNpc");
    npc.class_form_id = 0x0000_C1A5;
    npc.race_form_id = 0x0000_2ACE;
    npc.level = 7;
    npc.template_form_id = TEMPLATE; // present, but no flags gate it
    npc.template_flags = 0;

    let mut index = EsmIndex::default();
    index.npcs.insert(TEMPLATE, template);

    let mut world = World::new();
    world.register::<CharacterLevel>();
    world.register::<Background>();
    let target = world.spawn();
    stamp_character_components(&mut world, target, &npc, &index);

    assert_eq!(world.get::<CharacterLevel>(target).unwrap().level, 7);
    let background = world.get::<Background>(target).unwrap();
    assert_eq!(background.class_form_id, 0x0000_C1A5);
    assert_eq!(background.race_form_id, 0x0000_2ACE);
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
            female_model_path: String::new(),
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

fn legacy_armor_item(
    form_id: u32,
    biped_flags: u32,
    mesh_path: &str,
) -> byroredux_plugin::esm::records::ItemRecord {
    byroredux_plugin::esm::records::ItemRecord {
        form_id,
        common: byroredux_plugin::esm::records::common::CommonItemFields {
            model_path: mesh_path.to_owned(),
            ..Default::default()
        },
        kind: ItemKind::Armor {
            female_model_path: String::new(),
            biped_flags,
            dt: 0.0,
            dr: 0,
            health: 0,
            slot_mask: biped_flags as u16,
            armor_rating_x100: 0,
            armor_type: None,
            armatures: Vec::new(),
        },
    }
}

/// Runtime-FaceGen games now consume the same canonical equip result as the
/// Skyrim+ path. This pins the old path's two important semantics at that
/// shared boundary: authored CNTO counts survive and a winning upper-body
/// armor occupies the slot used to suppress the loose naked torso mesh.
#[test]
fn unified_equip_state_covers_fallout_runtime_body_and_preserves_count() {
    use byroredux_core::ecs::components::InventoryIndex;
    use byroredux_plugin::esm::records::NpcInventoryEntry;

    const ARMOR: u32 = 0x000A_AAAA;
    const UPPER_BODY: u32 = 1 << 2;
    let mut npc = test_npc(0x000B_BBBB, "RuntimeSkinFixture");
    npc.inventory.push(NpcInventoryEntry {
        item_form_id: ARMOR,
        count: 3,
    });
    let mut index = EsmIndex {
        game: GameKind::Fallout3NV,
        ..Default::default()
    };
    index.items.insert(
        ARMOR,
        legacy_armor_item(ARMOR, UPPER_BODY, r"armor\vaultsuit.nif"),
    );

    let state = build_npc_equip_state(&npc, &index, GameKind::Fallout3NV, Gender::Female);

    assert!(state.main_body_covered(GameKind::Fallout3NV));
    assert_eq!(state.armor_to_spawn.len(), 1);
    assert_eq!(state.armor_to_spawn[0].form_id, ARMOR);
    assert_eq!(state.armor_to_spawn[0].source_form_id, ARMOR);
    assert_eq!(state.armor_to_spawn[0].model_path, r"armor\vaultsuit.nif");
    assert_eq!(
        state
            .inventory
            .get(InventoryIndex(0))
            .map(|stack| stack.count),
        Some(3),
        "the unified builder must retain runtime CNTO stack counts"
    );
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
        starting_health: None,
        starting_magicka: None,
        starting_stamina: None,
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
    assert_eq!(
        state
            .armor_to_spawn
            .iter()
            .find(|armor| armor.form_id == SKIN)
            .unwrap()
            .hidden_biped_mask,
        0,
        "gear outside the skin's authored slots must not hide skin partitions"
    );
}

/// A single race-skin NIF commonly contains several dismember partitions.
/// Torso armor must hide only the torso partition while the same skin mesh
/// remains queued to supply uncovered hands.
#[test]
fn prebaked_equip_state_marks_only_partially_displaced_skin_slots() {
    const RACE: u32 = 0x0100_0040;
    const SKIN: u32 = 0x0100_0041;
    const SKIN_ARMA: u32 = 0x0100_0042;
    const TORSO: u32 = 0x0100_0043;
    const TORSO_ARMA: u32 = 0x0100_0044;
    const TORSO_BIT: u32 = 0x0004;
    const HANDS_BIT: u32 = 0x0010;

    let race = byroredux_plugin::esm::records::RaceRecord {
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
        starting_health: None,
        starting_magicka: None,
        starting_stamina: None,
        base_attributes: None,
        default_hair: None,
        voice_forms: None,
        facegen_main_clamp: None,
        facegen_face_clamp: None,
        race_reactions: Vec::new(),
        default_skin: Some(SKIN),
    };
    let mut npc = test_npc(0x0100_0045, "PartialSkinDisplacementNpc");
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
    index.items.insert(
        SKIN,
        skyrim_armor_item(SKIN, TORSO_BIT | HANDS_BIT, vec![SKIN_ARMA]),
    );
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
    assert_eq!(state.armor_to_spawn.len(), 2);
    let skin = state
        .armor_to_spawn
        .iter()
        .find(|armor| armor.form_id == SKIN)
        .unwrap();
    assert_eq!(skin.hidden_biped_mask, TORSO_BIT);
    assert_eq!(skin.hidden_biped_mask & HANDS_BIT, 0);
}

/// #3408 (SKY-2026-08-27b-D3-01) — a creature race whose default skin
/// authors `BOD2 == 0` must keep its body mesh.
///
/// `EquipmentSlots::equip` iterates the SET bits of the mask, so a zero mask
/// occupies nothing, so the #2094 occupancy retain could never be satisfied
/// and the mesh was discarded unconditionally. On real `Skyrim.esm` this hit
/// 351 of 5,118 NPC_ records — every Draugr, sabrecat, skeever, frostbite
/// spider and slaughterfish — of which 170 ended with no mesh source at all.
#[test]
fn prebaked_equip_state_keeps_zero_mask_race_skin() {
    const RACE: u32 = 0x0100_0060;
    const SKIN: u32 = 0x0100_0061;
    const SKIN_ARMA: u32 = 0x0100_0062;

    let mut race = byroredux_plugin::esm::records::RaceRecord {
        form_id: RACE,
        editor_id: "DraugrRaceFixture".to_string(),
        full_name: String::new(),
        description: String::new(),
        skill_bonuses: Vec::new(),
        body_models: Vec::new(),
        head_parts: Vec::new(),
        base_height: (1.0, 1.0),
        base_weight: (1.0, 1.0),
        race_flags: 0,
        starting_health: None,
        starting_magicka: None,
        starting_stamina: None,
        base_attributes: None,
        default_hair: None,
        voice_forms: None,
        facegen_main_clamp: None,
        facegen_face_clamp: None,
        race_reactions: Vec::new(),
        default_skin: None,
    };
    race.default_skin = Some(SKIN);

    let mut npc = test_npc(0x0100_0063, "ZeroMaskSkinNpc");
    npc.race_form_id = RACE;

    let mut index = EsmIndex {
        game: GameKind::Skyrim,
        ..Default::default()
    };
    index.races.insert(RACE, race);
    // `SkinDraugr` (0x00016EE3) ships exactly this shape: BOD2 == 0 with a
    // real ARMA behind it.
    index
        .items
        .insert(SKIN, skyrim_armor_item(SKIN, 0, vec![SKIN_ARMA]));
    index.armor_addons.insert(
        SKIN_ARMA,
        arma(SKIN_ARMA, r"actors\draugr\character assets\draugr.nif"),
    );

    let state = build_npc_equip_state(&npc, &index, GameKind::Skyrim, Gender::Male);
    assert_eq!(
        state.armor_to_spawn.len(),
        1,
        "a BOD2==0 race skin claims no biped bit, so the #2094 occupancy \
         filter has no opinion about it — it must not be dropped (#3408)"
    );
    assert_eq!(state.armor_to_spawn[0].form_id, SKIN);
    assert_eq!(
        state.armor_to_spawn[0].hidden_biped_mask, 0,
        "nothing can displace a mask that claims no region"
    );
}

/// The #3408 exemption must not disable the #2094 filter for the ordinary
/// case: a skin with a real mask, fully covered by gear, is still dropped.
#[test]
fn zero_mask_exemption_does_not_disable_the_occupancy_filter() {
    const RACE: u32 = 0x0100_0070;
    const SKIN: u32 = 0x0100_0071;
    const SKIN_ARMA: u32 = 0x0100_0072;
    const TORSO: u32 = 0x0100_0073;
    const TORSO_ARMA: u32 = 0x0100_0074;
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
        starting_health: None,
        starting_magicka: None,
        starting_stamina: None,
        base_attributes: None,
        default_hair: None,
        voice_forms: None,
        facegen_main_clamp: None,
        facegen_face_clamp: None,
        race_reactions: Vec::new(),
        default_skin: None,
    };
    race.default_skin = Some(SKIN);

    let mut npc = test_npc(0x0100_0075, "FullyDisplacedSkinNpc");
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
    assert!(
        !state
            .armor_to_spawn
            .iter()
            .any(|armor| armor.form_id == SKIN),
        "a skin with a real mask, fully covered by gear, is still displaced"
    );
}

/// #3409 (SKY-2026-08-27b-D3-02) — the pre-baked FaceGen head's own
/// displacement mask. Vanilla `Skyrim.esm` masks, measured:
///   closed helm (Dwarven / Daedric / Nord Plate / Guard FullReach) = bits
///   0,1,12,13; open helm (Iron / Hide / Studded / Steel light) = bits 1,12;
///   circlet = bit 12; `SkinNaked` = bits 0,2,3,7.
fn facegen_mask_fixture(helmet_bits: u32, skin_bits: u32) -> u32 {
    const RACE: u32 = 0x0100_0080;
    const SKIN: u32 = 0x0100_0081;
    const SKIN_ARMA: u32 = 0x0100_0082;
    const HELM: u32 = 0x0100_0083;
    const HELM_ARMA: u32 = 0x0100_0084;

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
        starting_health: None,
        starting_magicka: None,
        starting_stamina: None,
        base_attributes: None,
        default_hair: None,
        voice_forms: None,
        facegen_main_clamp: None,
        facegen_face_clamp: None,
        race_reactions: Vec::new(),
        default_skin: None,
    };
    race.default_skin = Some(SKIN);

    let mut npc = test_npc(0x0100_0085, "FacegenMaskNpc");
    npc.race_form_id = RACE;
    if helmet_bits != 0 {
        npc.inventory
            .push(byroredux_plugin::esm::records::NpcInventoryEntry {
                item_form_id: HELM,
                count: 1,
            });
    }

    let mut index = EsmIndex {
        game: GameKind::Skyrim,
        ..Default::default()
    };
    index.races.insert(RACE, race);
    index
        .items
        .insert(SKIN, skyrim_armor_item(SKIN, skin_bits, vec![SKIN_ARMA]));
    index
        .armor_addons
        .insert(SKIN_ARMA, arma(SKIN_ARMA, r"actors\character\skin.nif"));
    index
        .items
        .insert(HELM, skyrim_armor_item(HELM, helmet_bits, vec![HELM_ARMA]));
    index
        .armor_addons
        .insert(HELM_ARMA, arma(HELM_ARMA, r"armor\iron\helmet.nif"));

    build_npc_equip_state(&npc, &index, GameKind::Skyrim, Gender::Male).facegen_hidden_mask
}

/// The race skin's OWN bits must never reach the head's mask. `SkinNaked`
/// authors bit 0 (Head) and 47 of Skyrim's 99 races point `WNAM` at a skin
/// that does — folding those in would hide partition 130 and delete the face
/// of most humanoid NPCs.
#[test]
fn facegen_mask_excludes_the_race_skin_own_bits() {
    const SKIN_NAKED: u32 = 0b1000_1101; // bits 0, 2, 3, 7
    assert_eq!(
        facegen_mask_fixture(0, SKIN_NAKED),
        0,
        "an unarmoured NPC must hide nothing on their own head"
    );
}

/// An OPEN helm (bits 1 + 12) hides the hair partition and leaves the face:
/// bit 0 stays with the skin because the helmet never claimed it.
#[test]
fn facegen_mask_open_helm_hides_hair_but_not_the_face() {
    const OPEN_HELM: u32 = 0b0001_0000_0000_0010; // bits 1, 12
    const SKIN_NAKED: u32 = 0b1000_1101;
    let mask = facegen_mask_fixture(OPEN_HELM, SKIN_NAKED);
    assert_eq!(
        mask & (1 << 1),
        1 << 1,
        "hair (partition 131) must be hidden"
    );
    assert_eq!(
        mask & 1,
        0,
        "the face (partition 130) must survive an open helm — bit 0 is still \
         the skin's, so nothing displaced it"
    );
}

/// A CLOSED helm (bits 0,1,12,13) displaces bit 0 from the skin, so the head
/// and beard partitions go too — which is correct: those helms ship their own
/// partition-30 geometry (Dwarven: 1514 triangles) to replace them.
#[test]
fn facegen_mask_closed_helm_displaces_the_head_bit() {
    const CLOSED_HELM: u32 = 0b0011_0000_0000_0011; // bits 0, 1, 12, 13
    const SKIN_NAKED: u32 = 0b1000_1101;
    let mask = facegen_mask_fixture(CLOSED_HELM, SKIN_NAKED);
    assert_eq!(
        mask & 1,
        1,
        "a closed helm claims bit 0 and replaces the head"
    );
    assert_eq!(mask & (1 << 1), 1 << 1, "hair too");
    assert_eq!(mask & (1 << 13), 1 << 13, "ears too");
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
        starting_health: None,
        starting_magicka: None,
        starting_stamina: None,
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
#[ignore = "needs Oblivion game data on disk; parses the whole master (~1.4 GB resident)"]
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

// ── #2955 — the ACBS level field is overloaded ────────────────────────────

/// `NpcRecord::level` is not a level when the ACBS `PC Level Mult` bit is set:
/// it is a fixed-point multiplier on the *player's* level. Vanilla FNV has 268
/// such records (the bit and `level > 100` select exactly the same set), with
/// values that are round steps — `1000` alone covers 184 of them.
///
/// Two consumers read the value as a number: `expand_leveled_form_id` filters
/// `entry.level <= actor_level` and then takes the **highest** eligible tier,
/// so a multiplier of 1000 made every entry eligible and always drew the top
/// one; and `xp_to_next` asked for 150 050 XP instead of ~200.
#[test]
fn pc_level_mult_actors_resolve_to_calc_min_not_the_raw_multiplier() {
    // #3171 — imported explicitly from `byroredux_plugin`, not from this
    // crate's re-export: the rule now lives beside the `NPC_` record, and
    // this pin has to exercise *that* function so a fresh binary-side copy
    // (the #3081 / #3171 failure mode, twice over) has something to fail
    // against.
    use byroredux_plugin::esm::records::{effective_actor_level, ACBS_PC_LEVEL_MULT};

    // A plain actor: the flag is clear, so the field IS the level.
    let mut plain = test_npc(0x0100, "PlainRaider");
    plain.level = 12;
    plain.calc_min = 0;
    plain.acbs_flags = 0;
    assert_eq!(effective_actor_level(&plain), 12);

    // A PC-level-scaled actor, shaped like vanilla FNV: level carries the
    // multiplier, calc_min carries the real floor.
    let mut scaled = test_npc(0x0101, "ScaledTrooper");
    scaled.level = 1000;
    scaled.calc_min = 6;
    scaled.acbs_flags = ACBS_PC_LEVEL_MULT;
    assert_eq!(
        effective_actor_level(&scaled),
        6,
        "a PC-level-mult actor must resolve to its ACBS calcMin, never the \
         raw multiplier — 1000 makes every leveled-list entry eligible and \
         always draws the top tier (#2955)"
    );

    // calc_min == 0 (record carries none) floors at 1, not 0: level 0 would
    // make a leveled list resolve to nothing, trading over-levelled gear for
    // no gear at all.
    let mut no_floor = test_npc(0x0102, "ScaledNoFloor");
    no_floor.level = 500;
    no_floor.calc_min = 0;
    no_floor.acbs_flags = ACBS_PC_LEVEL_MULT;
    assert_eq!(effective_actor_level(&no_floor), 1);

    // The gate keys on the multiplier bit specifically, not on "level looks
    // too big" — a hand-authored level of 120 with the bit clear is a real
    // level and must survive.
    let mut high_but_real = test_npc(0x0103, "Deathclaw");
    high_but_real.level = 120;
    high_but_real.calc_min = 0;
    high_but_real.acbs_flags = 0x0010; // auto-calc stats, NOT the mult bit
    assert_eq!(effective_actor_level(&high_but_real), 120);

    // Negative levels still clamp to 0 on the non-mult path (pre-existing
    // behaviour, preserved).
    let mut negative = test_npc(0x0104, "Odd");
    negative.level = -5;
    negative.acbs_flags = 0;
    assert_eq!(effective_actor_level(&negative), 0);
}

/// #3158 — the end-to-end pin the issue asks for: a *Skyrim-parsed* NPC must
/// make `HasPerk` return non-zero.
///
/// This is deliberately the whole chain, not `stamp_character_components` in
/// isolation, because the bug lived at the seam between its three links and
/// each link looked correct on its own:
///
///   1. `parse_npc` reads `PRKR` into `NpcRecord::perks` — but only under
///      `uses_npc_perk_entries`, which before #3158 was
///      `uses_actor_value_properties` and excluded Skyrim;
///   2. `stamp_character_components` inserts `Perks` — but skips the
///      component entirely when the list is empty, so link 1's silence
///      became an absent component rather than an empty one;
///   3. `HasPerk` returns `0.0` for an absent `Perks` — which is the correct
///      Bethesda default for "actor lacks the perk", and therefore
///      indistinguishable from "no Skyrim actor can hold one".
///
/// A test on link 1 or 2 alone would have stayed green through the bug.
#[test]
fn skyrim_parsed_npc_perk_reaches_the_hasperk_condition() {
    use byroredux_core::character::Perks;
    use byroredux_plugin::esm::reader::{GameKind, SubRecord};
    use byroredux_plugin::esm::records::parse_npc;
    use byroredux_scripting::condition::{evaluate_function, ConditionFunction};

    const PERK: u32 = 0x0005_820C;
    const OTHER_PERK: u32 = 0x000C_44B7;

    // Skyrim `PRKR`: u32 PERK FormID + u8 rank + three unused bytes.
    let mut prkr = PERK.to_le_bytes().to_vec();
    prkr.extend_from_slice(&[2, 0, 0, 0]);
    let subs = vec![
        SubRecord {
            sub_type: *b"EDID",
            data: b"SkyrimPerkNpc\0".to_vec(),
        },
        SubRecord {
            sub_type: *b"PRKR",
            data: prkr,
        },
    ];
    let npc = parse_npc(0x0100_0020, &subs, GameKind::Skyrim, &None);
    assert_eq!(
        npc.perks,
        vec![(PERK, 2)],
        "link 1: Skyrim PRKR must survive the parse gate"
    );

    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    world.register::<Perks>();
    let index = EsmIndex::default();
    let actor = world.spawn();
    stamp_character_components(&mut world, actor, &npc, &index);
    assert!(
        world.get::<Perks>(actor).is_some(),
        "link 2: a Skyrim NPC with perks must receive the Perks component"
    );

    let has = |perk: u32| {
        let mut condition = byroredux_plugin::esm::records::condition::Condition::default();
        condition.function_index = 448; // HasPerk (Skyrim); 449 on FO3/FNV
        condition.param_1 = perk;
        evaluate_function(ConditionFunction::HasPerk, &condition, actor, &world)
    };
    assert_eq!(has(PERK), 1.0, "link 3: HasPerk must see the owned perk");
    assert_eq!(
        has(OTHER_PERK),
        0.0,
        "an unowned perk still reads 0.0 — the fix must not make HasPerk always true"
    );
}

// ── #3361 — the real-ESM equip-chain guard ────────────────────────────────
//
// Every other `build_npc_equip_state` test in this file builds a synthetic
// `EsmIndex`, and none of them gives a skin ARMO more than one ARMA — which
// is exactly why #3356 (OTFT INAM array truncation) and #3357 (single-ARMA
// skin resolution) both shipped and survived a prior audit pass. Nothing in
// the tree drove the equip chain against real game data on any game.
//
// `#[ignore]`-gated because it needs a Skyrim SE install, matching the
// `crates/plugin/tests/parse_real_esm.rs` convention. Opt in with:
//
//   cargo test -p byroredux --bin byroredux bannered_mare -- --ignored

/// The six named NPCs of `WhiterunBanneredMare`, the bench-of-record cell.
/// FormIDs read from `Skyrim.esm`.
const BANNERED_MARE: &[(&str, u32, Gender)] = &[
    ("Saadia", 0x0001_3BA2, Gender::Female),
    ("Hulda", 0x0001_3BA3, Gender::Female),
    ("Brenuin", 0x0001_3BA7, Gender::Male),
    ("Mikael", 0x0001_A670, Gender::Male),
    ("Sinmir", 0x0008_13B5, Gender::Male),
    ("AmaundMotierreEnd", 0x0004_E64F, Gender::Male),
];

fn skyrim_data_dir() -> Option<std::path::PathBuf> {
    if let Ok(v) = std::env::var("BYROREDUX_SKYRIMSE_DATA") {
        let p = std::path::PathBuf::from(v);
        if p.is_dir() {
            return Some(p);
        }
    }
    let p = std::path::PathBuf::from(
        "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
    );
    p.is_dir().then_some(p)
}

/// #3361 — pin the whole equip chain on real data: every Bannered Mare NPC
/// must reach Inventory + EquipmentSlots, and their race skin must supply a
/// TORSO mesh, not just the feet.
///
/// This is the guard that would have caught #3357: pre-fix every one of
/// these six resolved their skin to a `*Feet_1.nif` because `SkinNaked`'s
/// `NakedFeet` ARMA sorts ahead of `NakedTorso` and the resolver stopped at
/// the first race match.
#[test]
#[ignore]
fn bannered_mare_npcs_resolve_a_full_equip_state_on_real_skyrim_data() {
    let Some(data) = skyrim_data_dir() else {
        eprintln!("[#3361] skipping: Skyrim SE data unavailable");
        return;
    };
    let bytes = std::fs::read(data.join("Skyrim.esm")).expect("read Skyrim.esm");
    let index = byroredux_plugin::esm::parse_esm(&bytes).expect("parse Skyrim.esm");

    for &(name, form_id, gender) in BANNERED_MARE {
        let npc = index
            .npcs
            .get(&form_id)
            .unwrap_or_else(|| panic!("{name} ({form_id:08X}) must be present in Skyrim.esm"));

        let state = build_npc_equip_state(npc, &index, GameKind::Skyrim, gender);

        assert!(
            !state.inventory.is_empty(),
            "{name} must end up with a non-empty inventory"
        );
        assert!(
            state.equipment_slots.occupants.iter().any(Option::is_some),
            "{name} must occupy at least one biped slot"
        );

        // #3357 — the race skin must contribute a torso mesh. Pre-fix every
        // one of these resolved to a Feet nif and nothing else.
        let has_torso = state.armor_to_spawn.iter().any(|a| {
            let p = a.model_path.to_ascii_lowercase();
            p.contains("body") || p.contains("torso")
        });
        assert!(
            has_torso,
            "{name}: no torso mesh among {:?} — the race skin resolved to the \
             first race-matching ARMA only (#3357)",
            state
                .armor_to_spawn
                .iter()
                .map(|a| a.model_path)
                .collect::<Vec<_>>()
        );
    }
}

/// #3408 — the creature-race guard. #3361's Bannered Mare sweep walks six
/// humans, all on `NordRace` (`SkinNaked`, `BOD2 != 0`), so it could not
/// see the zero-mask defect at all: every Draugr, sabrecat, skeever,
/// frostbite spider and slaughterfish in vanilla Skyrim spawned bodyless.
///
/// One NPC per affected race, FormIDs read from `Skyrim.esm`.
const CREATURE_RACE_NPCS: &[(&str, u32)] = &[
    ("EncDraugr02MissileHeadM01", 0x0002_2401),
    ("EncDraugr03AmbushMelee2HHeadM07", 0x000E_A50E),
    ("dunFellglow_WarlockPet", 0x0007_3989),
];

/// #3408 — an NPC on a race whose default skin authors `BOD2 == 0` must
/// resolve at least one mesh. Pre-fix the #2094 occupancy retain dropped
/// every one of them (measured: 351 skin meshes dropped, 0 kept).
#[test]
#[ignore]
fn creature_race_npcs_keep_their_skin_mesh_on_real_skyrim_data() {
    let Some(data) = skyrim_data_dir() else {
        eprintln!("[#3408] skipping: Skyrim SE data unavailable");
        return;
    };
    let bytes = std::fs::read(data.join("Skyrim.esm")).expect("read Skyrim.esm");
    let index = byroredux_plugin::esm::parse_esm(&bytes).expect("parse Skyrim.esm");

    for &(name, form_id) in CREATURE_RACE_NPCS {
        let npc = index
            .npcs
            .get(&form_id)
            .unwrap_or_else(|| panic!("{name} ({form_id:08X}) must be present in Skyrim.esm"));
        let state = build_npc_equip_state(npc, &index, GameKind::Skyrim, Gender::Male);
        assert!(
            !state.armor_to_spawn.is_empty(),
            "{name} ({form_id:08X}) resolved no mesh at all — its race skin \
             authors BOD2==0 and the #2094 occupancy filter dropped it (#3408)"
        );
    }

    // Corpus-level floor: sweep every NPC_ whose race points WNAM at a
    // zero-mask skin. Pre-fix this was 351 dropped / 0 kept.
    let mut zero_mask_race_npcs = 0usize;
    let mut with_mesh = 0usize;
    for npc in index.npcs.values() {
        let Some(race) = index.races.get(&npc.race_form_id) else {
            continue;
        };
        let Some(skin_fid) = race.default_skin else {
            continue;
        };
        let Some(skin) = index.items.get(&skin_fid) else {
            continue;
        };
        let ItemKind::Armor { biped_flags: 0, .. } = skin.kind else {
            continue;
        };
        zero_mask_race_npcs += 1;
        let state = build_npc_equip_state(npc, &index, GameKind::Skyrim, Gender::Male);
        if state
            .armor_to_spawn
            .iter()
            .any(|armor| armor.form_id == skin_fid)
        {
            with_mesh += 1;
        }
    }
    eprintln!("[#3408] zero-mask-skin race NPCs: {zero_mask_race_npcs}, with mesh: {with_mesh}");
    assert!(
        zero_mask_race_npcs >= 351,
        "expected >= 351 NPC_ records on zero-mask-skin races in Skyrim.esm, \
         got {zero_mask_race_npcs} — the census this guard rests on has moved"
    );
    assert_eq!(
        with_mesh,
        zero_mask_race_npcs,
        "every NPC on a zero-mask-skin race must keep its skin mesh; \
         {} of {zero_mask_race_npcs} lost it (#3408)",
        zero_mask_race_npcs - with_mesh
    );
}

/// #3409 — the real-data guard. Sweeps every NPC_ in `Skyrim.esm` and pins
/// the population the fix produces, rather than restating the fold that
/// produces it: how many actors end up hiding head partitions at all, and
/// that BOTH helm classes are represented — closed helms displacing bit 0
/// (face + beard replaced by the helm's own partition-30 geometry) and open
/// helms leaving it with the race skin (face visible under the helm).
///
/// Pre-fix every one of these was 0: the FaceGen phase passed `None` for its
/// pre-spawn hook, so no head partition was ever hidden.
#[test]
#[ignore]
fn helmeted_npcs_get_a_facegen_hide_mask_on_real_skyrim_data() {
    let Some(data) = skyrim_data_dir() else {
        eprintln!("[#3409] skipping: Skyrim SE data unavailable");
        return;
    };
    let bytes = std::fs::read(data.join("Skyrim.esm")).expect("read Skyrim.esm");
    let index = byroredux_plugin::esm::parse_esm(&bytes).expect("parse Skyrim.esm");

    // Head-family bits: 0 Head, 1 Hair, 12 Circlet, 13 Ears. Bit 11
    // (LongHair) is claimed by exactly 1 of Skyrim's 2,762 ARMOs, which is
    // why partition 141 is a documented residual, not covered here.
    const HEAD_FAMILY: u32 = (1 << 0) | (1 << 1) | (1 << 12) | (1 << 13);

    let mut with_mask = 0usize;
    let mut closed_helm = 0usize;
    let mut open_helm = 0usize;
    for npc in index.npcs.values() {
        let mask =
            build_npc_equip_state(npc, &index, GameKind::Skyrim, Gender::Male).facegen_hidden_mask;
        if mask & HEAD_FAMILY == 0 {
            continue;
        }
        with_mask += 1;
        if mask & 1 != 0 {
            closed_helm += 1;
        } else {
            open_helm += 1;
        }
    }
    eprintln!(
        "[#3409] NPCs hiding head partitions: {with_mask} \
         (closed-helm/face-replaced {closed_helm}, open-helm/face-kept {open_helm})"
    );
    assert!(
        with_mask >= 1_500,
        "expected >= 1500 Skyrim NPCs to hide head partitions, got {with_mask} \
         — pre-#3409 this was 0 because the FaceGen phase passed no pre-spawn hook"
    );
    assert!(
        open_helm > 0,
        "open helms (bits 1+12, no bit 0) must leave the face: a run where \
         EVERY masked NPC also loses partition 130 means the race skin's own \
         bit 0 leaked into the mask — `SkinNaked` claims it on 47 of 99 races"
    );
    // Vanilla authors 587 hair-slot (bit 1) ARMOs against 175 head-slot
    // (bit 0) ones, so open helms must dominate by a wide margin. This is the
    // assertion that catches the specific way this fix can be broken: drop
    // the race-skin exclusion from the fold and the split inverts to
    // 3826 closed / 165 open — i.e. 3826 NPCs rendering faceless, because
    // `SkinNaked`'s own bit 0 leaked in.
    assert!(
        open_helm > closed_helm,
        "open helms must outnumber closed ones ({open_helm} vs {closed_helm}); \
         an inverted split means the race skin's bit 0 is being treated as a \
         displacement of the head it actually supplies"
    );
    assert!(
        closed_helm > 0,
        "closed helms (bits 0,1,12,13) must displace the head bit — 175 of \
         Skyrim's 2,762 ARMOs are authored that way and ship their own \
         partition-30 geometry to replace it"
    );
}

/// #3356 — the OTFT `INAM` array. Every one of these five Bannered Mare
/// outfits authors more than one item in a single `INAM`; pre-fix each
/// yielded exactly one, so 765 of Skyrim.esm's 1,246 outfit items (61%)
/// never reached an NPC.
#[test]
#[ignore]
fn bannered_mare_outfits_keep_every_inam_entry_on_real_skyrim_data() {
    let Some(data) = skyrim_data_dir() else {
        eprintln!("[#3356] skipping: Skyrim SE data unavailable");
        return;
    };
    let bytes = std::fs::read(data.join("Skyrim.esm")).expect("read Skyrim.esm");
    let index = byroredux_plugin::esm::parse_esm(&bytes).expect("parse Skyrim.esm");

    for (edid, form_id, expected) in [
        ("FarmClothesOutfit02", 0x0002_D75E_u32, 2usize),
        ("BarkeepClothes01", 0x0005_FB81, 2),
        ("BeggarWithHatOutfit", 0x0002_8B61, 3),
        ("ArmorBandedIronAllOutfit", 0x000B_1FAE, 4),
        ("FineClothesOutfit02", 0x000E_40DD, 2),
    ] {
        let outfit = index
            .outfits
            .get(&form_id)
            .unwrap_or_else(|| panic!("{edid} ({form_id:08X}) must be present"));
        assert_eq!(
            outfit.items.len(),
            expected,
            "{edid} must keep all {expected} INAM entries, got {:?} (#3356)",
            outfit.items
        );
    }

    // Corpus-level floor: 481 outfits carrying 1,246 items. Pre-fix this was
    // exactly 481.
    let total: usize = index.outfits.values().map(|o| o.items.len()).sum();
    assert!(
        total >= 1_246,
        "Skyrim.esm OTFT items: expected >= 1246, got {total} — INAM arrays \
         are being truncated again (#3356)"
    );
}
