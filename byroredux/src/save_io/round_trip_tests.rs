//! Extracted from `save_io.rs`'s inline `mod tests` (#2407 / TD1-004).
//! Production code there is ~1030 LOC; the test bulk alone pushed the
//! file past 3000. Split by topic, contents unchanged.

use super::*;
use byroredux_core::ecs::components::{LightFlicker, LightSource, Transform};
use byroredux_core::form_id::FormIdPool;
use byroredux_core::math::Vec3;
use byroredux_core::string::StringPool;
use byroredux_save::{decode, restore_world};
use byroredux_scripting::ScriptTimer;

/// Tripwire for the [`MUTABLE_DELTA_COLUMNS`] invariant (SAVE-D1-02 /
/// SAVE-D6-01): the live overlay applies component *values* verbatim
/// onto a reloaded cell without re-installing the saved `StringPool`
/// or remapping value-embedded entity ids, so a delta column may carry
/// only session-stable fields — **no `FixedString`, no `EntityId`, no
/// session-local registry handle**.
///
/// Rust has no field reflection, so this can't auto-scan the structs.
/// Instead it pins the exact set against an audited expectation: adding
/// a column makes this fail, forcing the maintainer to confirm the new
/// type is delta-safe (per the doc comment) and update `AUDITED` here.
/// `Name` (FixedString) and `AnimationPlayer`/`AnimationStack`
/// (EntityId + registry handle) are the registered-but-excluded types
/// this guard exists to keep out.
#[test]
fn delta_columns_carry_only_session_stable_fields() {
    // Each entry was hand-verified free of FixedString / EntityId /
    // session-handle fields (Transform: glam f32s; Inventory: u32 +
    // ItemInstancePool index; EquipmentSlots: Option<u32> array;
    // LightSource/LightFlicker: f32/u32 + [f32;3]; ScriptTimer: u32+f32).
    const AUDITED: &[&str] = &[
        "Transform",
        "Inventory",
        "EquipmentSlots",
        "LightSource",
        "LightFlicker",
        "ScriptTimer",
        // #2291 — bool-only activator state plus ScriptVariables' stable
        // ConditionStringId(u64) → f32 map. No session-local identity.
        "TwoStateActivator",
        "ScriptVariables",
        // #1834 — ActorValues: HashMap<u32 AVIF-FormID, [f32; 4] layers>.
        // Keys are global-space FormIDs (stable across reload); values are
        // plain f32s. No FixedString / EntityId / session handle → delta-safe.
        "ActorValues",
        // EquippedWeapon: u32 inventory index + u32 base FormID + f32
        // damage. Dead: empty marker. Neither carries session identity.
        "EquippedWeapon",
        "Dead",
        // #2014 — WanderState/PatrolState: home/target Vec3 + WanderPhase
        // enum (Walking, or Paused{remaining: f32}) + pick_count u32.
        // TravelState: destination Vec3. GuardState: anchor Vec3.
        // Traveled/Escorted: empty unit-struct terminal markers. None
        // carry FixedString / EntityId / session handle → delta-safe.
        "WanderState",
        "TravelState",
        "Traveled",
        "GuardState",
        "PatrolState",
        "Escorted",
        // #2292 — ActorControlState: a single bool (`restrained`). No
        // FixedString / EntityId / session handle → delta-safe.
        "ActorControlState",
        // #3165 — CharacterController is f32/bool-only. Breath and drowning
        // carry are gameplay state; pose restore clears its three transient
        // movement fields after the overlay.
        "CharacterController",
        // #2379 — RigidBodyData: MotionType enum (no payload) + 5 plain
        // f32s. No FixedString / EntityId / session handle → delta-safe.
        "RigidBodyData",
        // #2382 — RumbleOnActivate: f32/bool property fields +
        // RumbleState enum (Busy carries one f32). No FixedString /
        // EntityId / session handle → delta-safe.
        "RumbleOnActivate",
    ];
    assert_eq!(
        MUTABLE_DELTA_COLUMNS, AUDITED,
        "MUTABLE_DELTA_COLUMNS changed: a delta column must carry no \
         FixedString / EntityId / session-handle field (see the type's \
         doc comment). If the new type is delta-safe, add it to AUDITED.",
    );
}

/// A `MUTABLE_DELTA_COLUMNS` type's disposition once the scan below finds a
/// production removal site for it (SAVE-D1-2026-08-30-02 / #3793).
enum RemovalDisposition {
    /// A reconciler in `execute_pending_save_loads`' tail rebuilds the
    /// removal after `apply_deltas` — must be called from `save_io.rs`.
    Reconciler(&'static str),
    /// Explicitly audited as needing no reconciler, with the reason
    /// stated here instead of the column silently not appearing in
    /// `RECONCILED` at all (which is indistinguishable, to a reader,
    /// from "nobody checked").
    NoReconcilerNeeded(&'static str),
}

/// #3488 — the additive-only overlay cannot express a removal, so any
/// `MUTABLE_DELTA_COLUMNS` type that a *production* path removes at runtime
/// needs a reconciler in `execute_pending_save_loads`' tail, exactly as
/// `Dead` has `reconcile_dead_actor_runtime_state` (#3022) — UNLESS the
/// removal is on an entity that never outlives a cell reload, in which case
/// [`RemovalDisposition::NoReconcilerNeeded`] states why. Otherwise the
/// removal silently fails to survive a live load on any entity that
/// outlives the cell reload — which the process-lifetime player body
/// always does.
///
/// SAVE-D1-2026-08-30-02 (#3793) — this used to claim it "scans the tree"
/// but only grepped two fixed strings for one hardcoded column
/// (`EquippedWeapon`), so a removal through any other spelling — notably
/// the local `remove_component::<T>` helper duplicated in
/// `npc_spawn/ai_package.rs` and `combat.rs` — was invisible to it. It now
/// does what the doc always claimed: walks the same scan roots
/// [`super::registry_completeness_tests::discover_scan_roots`] finds,
/// strips test code, and matches BOTH `remove::<T>(` (covers
/// `world.remove::<T>(`) and `remove_component::<T>(` (the helper) against
/// every `MUTABLE_DELTA_COLUMNS` name. Adding a new removal site — under
/// either spelling — now makes this fail until `RECONCILED` states its
/// disposition.
#[test]
fn delta_columns_removed_at_runtime_have_a_load_reconciler() {
    use super::registry_completeness_tests::{collect_rs_files, discover_scan_roots};

    /// Every `MUTABLE_DELTA_COLUMNS` type with a production (non-test)
    /// removal site, paired with its audited disposition.
    const RECONCILED: &[(&str, RemovalDisposition)] = &[
        // inventory.rs' `reconcile_equipped_weapon` else-arm, reached from
        // the pause menu's unequip action (main.rs' `inventory_actions`).
        (
            "EquippedWeapon",
            RemovalDisposition::Reconciler("reconcile_player_equipped_weapon"),
        ),
        // npc_spawn/ai_package.rs' `clear_all_ai_behavior_state` removes
        // these six via the local `remove_component::<T>` helper when an
        // actor's package re-evaluates to a different procedure. Audited
        // by the AUDIT_SAVE_2026-08-30 report (SAVE-D1-2026-08-30-02) as
        // needing no reconciler: every carrier is an NPC actor, which the
        // cell reload destroys and rebuilds from scratch (fresh package
        // evaluation re-derives whichever of these it needs) — unlike
        // `EquippedWeapon`/`Dead`, no entity that outlives a reload
        // (the process-lifetime player body) ever carries them.
        (
            "WanderState",
            RemovalDisposition::NoReconcilerNeeded(
                "NPC-only carrier; destroyed and rebuilt by the cell reload",
            ),
        ),
        (
            "TravelState",
            RemovalDisposition::NoReconcilerNeeded(
                "NPC-only carrier; destroyed and rebuilt by the cell reload",
            ),
        ),
        (
            "Traveled",
            RemovalDisposition::NoReconcilerNeeded(
                "NPC-only carrier; destroyed and rebuilt by the cell reload",
            ),
        ),
        (
            "GuardState",
            RemovalDisposition::NoReconcilerNeeded(
                "NPC-only carrier; destroyed and rebuilt by the cell reload",
            ),
        ),
        (
            "PatrolState",
            RemovalDisposition::NoReconcilerNeeded(
                "NPC-only carrier; destroyed and rebuilt by the cell reload",
            ),
        ),
        (
            "Escorted",
            RemovalDisposition::NoReconcilerNeeded(
                "NPC-only carrier; destroyed and rebuilt by the cell reload",
            ),
        ),
    ];

    let save_io = include_str!("../save_io.rs");
    for (column, disposition) in RECONCILED {
        assert!(
            MUTABLE_DELTA_COLUMNS.contains(column),
            "{column} is listed as reconciled but is no longer a delta column"
        );
        match disposition {
            RemovalDisposition::Reconciler(reconciler) => {
                assert!(
                    save_io.contains(reconciler),
                    "{column} is removed at runtime but `{reconciler}` is not called from \
                     save_io.rs — the additive-only overlay would leave the live component \
                     standing after a load (#3488)"
                );
            }
            RemovalDisposition::NoReconcilerNeeded(reason) => {
                assert!(
                    !reason.is_empty(),
                    "{column}'s NoReconcilerNeeded exemption must state a reason, not be silent"
                );
            }
        }
    }

    // The scan itself (#3793): find every production removal site for a
    // MUTABLE_DELTA_COLUMNS type, matching both `remove::<T>(` (the
    // `world.remove::<T>(` idiom) and `remove_component::<T>(` (the local
    // helper duplicated in ai_package.rs/combat.rs — its own body is
    // `query.remove(actor)`, which carries no type name, so only the
    // *call site* spelling is greppable).
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for root in discover_scan_roots(manifest) {
        collect_rs_files(&root, &mut files);
    }

    let mut removed_columns = std::collections::BTreeSet::new();
    for path in &files {
        let src = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("SAVE-D1-2026-08-30-02 guard can't read {}: {e}", path.display()));
        // Same test-code stripping as the SAVE-D1-12 guard: repository
        // convention keeps cfg(test) modules at file tails; standalone
        // *_tests.rs files are all-test.
        if path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .is_some_and(|stem| stem.ends_with("_tests"))
            || path.components().any(|part| part.as_os_str() == "tests")
        {
            continue;
        }
        let production_src = src.split("#[cfg(test)]").next().unwrap_or(&src);
        for column in MUTABLE_DELTA_COLUMNS {
            let direct = format!("remove::<{column}>(");
            let via_helper = format!("remove_component::<{column}>(");
            if production_src.contains(&direct) || production_src.contains(&via_helper) {
                removed_columns.insert(*column);
            }
        }
    }

    let reconciled_columns: std::collections::BTreeSet<&str> =
        RECONCILED.iter().map(|(name, _)| *name).collect();

    let undocumented: Vec<&str> = removed_columns
        .difference(&reconciled_columns)
        .copied()
        .collect();
    assert!(
        undocumented.is_empty(),
        "found a production removal site for MUTABLE_DELTA_COLUMNS type(s) \
         {undocumented:?} with no entry in RECONCILED — the additive-only \
         overlay cannot express a removal (#3488). Add an entry: either \
         `RemovalDisposition::Reconciler(\"fn_name\")` if the removal needs \
         one, or `RemovalDisposition::NoReconcilerNeeded(\"reason\")` if the \
         carrier entity never outlives a cell reload.",
    );

    let stale: Vec<&str> = reconciled_columns
        .difference(&removed_columns.iter().copied().collect())
        .copied()
        .collect();
    assert!(
        stale.is_empty(),
        "RECONCILED lists {stale:?} but the scan found no production removal \
         site for it any more — the removal this entry documents moved or \
         was deleted; re-verify and update RECONCILED (#3793)",
    );
}

/// The binary's curated registry must round-trip its full type set —
/// including the cross-crate `ScriptTimer`, a stable form id, and
/// (SAVE-D2-04 / #2021) `LightSource`/`LightFlicker` — through
/// encode → decode → restore into a fresh World.
///
/// SIBLING sweep (#2021): the other registered types with an
/// audit-confirmed flat shape (no `FixedString`/`EntityId`) —
/// `Inventory`/`EquipmentSlots` (round-tripped by
/// `crates/save/tests/round_trip.rs`'s `build_source_world`),
/// `ActorValues` (`actor_values_survive_save_load_round_trip`), and
/// `ScriptTimer` (this test) — already have dedicated round-trip
/// coverage. `FollowState`/`EscortState`/`Seated` are excluded from
/// this sweep: per the `MUTABLE_DELTA_COLUMNS` doc comment above they
/// carry `EntityId` fields, so they're not flat and a gap there would
/// be a different (higher-risk) finding, not this one.
#[test]
fn binary_registry_round_trips_including_scripttimer() {
    let reg = build_save_registry();

    let mut src = World::new();
    src.insert_resource(StringPool::new());
    src.insert_resource(FormIdPool::new());
    let e = src.spawn();
    let other = src.spawn();
    src.insert(e, Transform::from_translation(Vec3::new(4.0, 5.0, 6.0)));
    src.insert(
        e,
        ScriptTimer {
            id: 42,
            remaining: 3.5,
        },
    );
    src.insert(
        other,
        ScriptTimer {
            id: 7,
            remaining: 0.25,
        },
    );
    let mut saved_light = LightSource::from_legacy_world_units(
        512.0,
        [0.9, 0.6, 0.2],
        0x0000_0008, // LIGHT_FLAG_FLICKER
        2.0,
        byroredux_core::ecs::LightKind::Point,
        [0.0; 3],
        0.0,
        0,
    );
    saved_light.dimmer = 0.75;
    saved_light.intensity = 1.25;
    src.insert(e, saved_light);
    src.insert(
        e,
        LightFlicker {
            animation_flags: byroredux_core::ecs::LIGHT_FLAG_FLICKER,
            period_secs: 0.5,
            intensity_amplitude: 0.25,
            movement_amplitude: 1.5,
            base_translation: [10.0, 20.0, 30.0],
            phase_offset_secs: 0.125,
        },
    );

    let snap = save_world(&src, &reg).unwrap();
    let bytes = encode(&snap, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).unwrap();

    assert_eq!(dst.next_entity_id(), 2);
    let q = dst.query::<ScriptTimer>().unwrap();
    let timers: std::collections::HashMap<u32, (u32, f32)> =
        q.iter().map(|(en, t)| (en, (t.id, t.remaining))).collect();
    assert_eq!(timers[&0], (42, 3.5));
    assert_eq!(timers[&1], (7, 0.25));

    let qt = dst.query::<Transform>().unwrap();
    assert_eq!(
        qt.iter().next().unwrap().1.translation,
        Vec3::new(4.0, 5.0, 6.0)
    );

    let light = dst.query::<LightSource>().unwrap().get(0).copied().unwrap();
    assert_eq!(light.emitter.range.to_bethesda_units(), 512.0);
    assert_eq!(light.emitter.radiant_intensity.get(), [0.9, 0.6, 0.2]);
    assert_eq!(light.flags, 0x0000_0008);
    assert_eq!(light.dimmer, 0.75);
    assert_eq!(light.intensity, 1.25);
    assert_eq!(light.emitter.falloff_exponent, 2.0);

    let flicker = dst
        .query::<LightFlicker>()
        .unwrap()
        .get(0)
        .copied()
        .unwrap();
    assert_eq!(flicker.period_secs, 0.5);
    assert_eq!(flicker.intensity_amplitude, 0.25);
    assert_eq!(flicker.movement_amplitude, 1.5);
    assert_eq!(flicker.base_translation, [10.0, 20.0, 30.0]);
    assert_eq!(flicker.phase_offset_secs, 0.125);
}

/// #1834 — an actor's layered `ActorValues` (class-auto-calc base plus any
/// `setav`/`modav` console edit) must survive a save → load round-trip.
/// Pre-fix the component was neither registered nor serialised, so a
/// reload dropped every permanent/temporary/damage layer and re-derived
/// only the spawn base.
#[test]
fn actor_values_survive_save_load_round_trip() {
    use byroredux_core::ecs::components::ActorValues;
    const AV_HEALTH: u32 = 0x0000_02C9;
    let reg = build_save_registry();

    let mut src = World::new();
    src.insert_resource(StringPool::new());
    src.insert_resource(FormIdPool::new());
    let e = src.spawn();
    let mut av = ActorValues::new();
    av.set_base(AV_HEALTH, 100.0); // class auto-calc base
    av.mod_permanent(AV_HEALTH, 25.0); // e.g. a `modav` edit
    av.apply_damage(AV_HEALTH, 40.0);
    src.insert(e, av);

    let snap = save_world(&src, &reg).unwrap();
    let bytes = encode(&snap, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).unwrap();

    let q = dst.query::<ActorValues>().unwrap();
    let (_, restored) = q.iter().next().expect("ActorValues must round-trip");
    // All four layers survive (pre-#1834 the whole component was dropped).
    assert_eq!(restored.current(AV_HEALTH), 85.0, "100 + 25 − 40");
    let layer = restored.get(AV_HEALTH).expect("entry present after reload");
    assert_eq!(layer.base, 100.0);
    assert_eq!(layer.permanent_mod, 25.0);
    assert_eq!(layer.damage, 40.0);
}

/// #2014 / SAVE-D1-NEW-01 — the seven M42 AI-procedure runtime-state
/// components must survive a save → load round-trip. Pre-fix, none were
/// registered, so a reload silently dropped them — the sharpest edge
/// being a terminal one-shot marker like `Traveled`: an NPC that had
/// already finished its Travel behavior would come back unfinished and
/// silently redo it. Covers one delta-safe state type (`WanderState`),
/// one terminal marker (`Traveled`), and one `EntityId`-carrying type
/// (`Seated`) — the three distinct save shapes this fix introduces.
#[test]
fn ai_procedure_state_and_terminal_markers_survive_save_load_round_trip() {
    use byroredux_core::ecs::components::{Seated, Traveled, WanderPhase, WanderState};
    let reg = build_save_registry();

    let mut src = World::new();
    src.insert_resource(StringPool::new());
    src.insert_resource(FormIdPool::new());

    let wanderer = src.spawn();
    src.insert(
        wanderer,
        WanderState {
            home: Vec3::new(1.0, 2.0, 3.0),
            target: Vec3::new(4.0, 5.0, 6.0),
            phase: WanderPhase::Paused { remaining: 2.5 },
            pick_count: 7,
        },
    );

    let arrived = src.spawn();
    src.insert(arrived, Traveled);

    let furniture = src.spawn();
    let sitter = src.spawn();
    src.insert(
        sitter,
        Seated {
            furniture,
            animation_restore: Default::default(),
        },
    );

    let snap = save_world(&src, &reg).unwrap();
    let bytes = encode(&snap, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).unwrap();

    let wq = dst.query::<WanderState>().unwrap();
    let (_, restored_wander) = wq.iter().next().expect("WanderState must round-trip");
    assert_eq!(restored_wander.home, Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(restored_wander.target, Vec3::new(4.0, 5.0, 6.0));
    assert_eq!(
        restored_wander.phase,
        WanderPhase::Paused { remaining: 2.5 }
    );
    assert_eq!(restored_wander.pick_count, 7);

    let tq = dst.query::<Traveled>().unwrap();
    assert_eq!(
        tq.iter().count(),
        1,
        "Traveled must round-trip — losing it makes an already-arrived NPC redo its Travel"
    );

    let sq = dst.query::<Seated>().unwrap();
    let (_, restored_seated) = sq.iter().next().expect("Seated must round-trip");
    // restore_world preserves entity ids verbatim, so the furniture
    // reference survives even though it wasn't a MUTABLE_DELTA_COLUMN.
    assert_eq!(restored_seated.furniture, furniture);
}

/// #2291 — recognized default2StateActivator instances carry live state
/// in two components. Dropping either one reverts the object's behavior
/// or makes GetVMScriptVariable observe stale values after reload.
#[test]
fn two_state_activator_and_script_variables_survive_save_load_round_trip() {
    use byroredux_scripting::{ScriptVariables, TwoStateActivator};

    let reg = build_save_registry();
    let mut src = World::new();
    src.insert_resource(StringPool::new());
    src.insert_resource(FormIdPool::new());

    let activator = src.spawn();
    src.insert(
        activator,
        TwoStateActivator {
            is_open: true,
            is_animating: false,
            do_once: true,
            activated_once: true,
        },
    );
    let mut variables = ScriptVariables::default();
    variables.set_by_name("::isOpen_var", 1.0);
    variables.set_by_name("::isAnimating_var", 0.0);
    src.insert(activator, variables);

    let snapshot = save_world(&src, &reg).unwrap();
    let bytes = encode(&snapshot, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).unwrap();

    let restored = dst
        .get::<TwoStateActivator>(activator)
        .expect("TwoStateActivator must round-trip");
    assert_eq!(
        *restored,
        TwoStateActivator {
            is_open: true,
            is_animating: false,
            do_once: true,
            activated_once: true,
        }
    );

    let restored_variables = dst
        .get::<ScriptVariables>(activator)
        .expect("ScriptVariables must round-trip");
    assert_eq!(restored_variables.get_by_name("::isOpen_var"), Some(1.0));
    assert_eq!(
        restored_variables.get_by_name("::isAnimating_var"),
        Some(0.0)
    );
}

/// Regression: #2292 (SAVE-D1-09) — `Game.DisablePlayerControls`/
/// `Actor.SetRestrained` lock state must survive a save/load round
/// trip. Pre-fix, neither `PlayerControlState` (a `Resource`) nor
/// `ActorControlState` (a per-actor component) was in
/// `build_save_registry`, so a save taken mid-scripted-sequence
/// silently reset both to their all-enabled / unrestrained defaults
/// on reload.
#[test]
fn player_and_actor_control_state_survive_save_load_round_trip() {
    use byroredux_scripting::{ActorControlState, PlayerControlState};

    let reg = build_save_registry();
    let mut src = World::new();
    src.insert_resource(StringPool::new());
    src.insert_resource(FormIdPool::new());
    src.insert_resource(PlayerControlState {
        movement_enabled: false,
        fighting_enabled: false,
        camera_switching_enabled: true,
        looking_enabled: true,
        sneaking_enabled: false,
        menu_enabled: true,
        activation_enabled: false,
        journal_tabs_enabled: true,
        disabled_pov_type: 1,
        ai_driven: true,
        hud_cart_mode: true,
    });

    let npc = src.spawn();
    src.insert(npc, ActorControlState { restrained: true });

    let snapshot = save_world(&src, &reg).unwrap();
    let bytes = encode(&snapshot, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).unwrap();

    let restored_controls = dst
        .try_resource::<PlayerControlState>()
        .expect("PlayerControlState must round-trip");
    assert!(!restored_controls.movement_enabled);
    assert!(!restored_controls.fighting_enabled);
    assert!(restored_controls.camera_switching_enabled);
    assert!(!restored_controls.sneaking_enabled);
    assert!(!restored_controls.activation_enabled);
    assert_eq!(restored_controls.disabled_pov_type, 1);
    assert!(restored_controls.ai_driven);
    assert!(restored_controls.hud_cart_mode);
    drop(restored_controls);

    let restored_actor = dst
        .get::<ActorControlState>(npc)
        .expect("ActorControlState must round-trip");
    assert!(restored_actor.restrained);
}

/// Regression: #2379 (SAVE-D1-14) — a scripted `.SetMotionType()` call
/// must survive a save/load round trip. Pre-fix, `RigidBodyData` was
/// absent from `build_save_registry`, so a scripted motion-type change
/// (e.g. making a normally-static prop dynamic for a scripted sequence)
/// silently reverted to the ESM-derived default on reload.
#[test]
fn rigid_body_data_survives_save_load_round_trip() {
    use byroredux_core::ecs::components::{MotionType, RigidBodyData};

    let reg = build_save_registry();
    let mut src = World::new();
    src.insert_resource(StringPool::new());
    src.insert_resource(FormIdPool::new());

    let prop = src.spawn();
    src.insert(
        prop,
        RigidBodyData {
            motion_type: MotionType::Dynamic,
            mass: 12.5,
            friction: 0.4,
            restitution: 0.2,
            linear_damping: 0.1,
            angular_damping: 0.05,
            collidable: true,
        },
    );

    let snapshot = save_world(&src, &reg).unwrap();
    let bytes = encode(&snapshot, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).unwrap();

    let restored = dst
        .get::<RigidBodyData>(prop)
        .expect("RigidBodyData must round-trip");
    assert_eq!(restored.motion_type, MotionType::Dynamic);
    assert_eq!(restored.mass, 12.5);
    assert_eq!(restored.friction, 0.4);
    assert_eq!(restored.restitution, 0.2);
    assert_eq!(restored.linear_damping, 0.1);
    assert_eq!(restored.angular_damping, 0.05);
}

/// Regression: #2382 (SAVE-D1-17) — a `defaultRumbleOnActivate` lever
/// mid-wait (or already one-shot-fired) must survive a save/load round
/// trip instead of silently resetting to `Active`.
#[test]
fn rumble_on_activate_survives_save_load_round_trip() {
    use byroredux_scripting::papyrus_demo::{RumbleOnActivate, RumbleState};

    let reg = build_save_registry();
    let mut src = World::new();
    src.insert_resource(StringPool::new());
    src.insert_resource(FormIdPool::new());

    let lever = src.spawn();
    src.insert(
        lever,
        RumbleOnActivate {
            camera_intensity: 0.5,
            duration: 1.5,
            repeatable: false,
            shake_left: 0.75,
            shake_right: 0.75,
            state: RumbleState::Busy {
                wait_remaining_secs: 0.9,
            },
        },
    );

    let snapshot = save_world(&src, &reg).unwrap();
    let bytes = encode(&snapshot, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).unwrap();

    let restored = dst
        .get::<RumbleOnActivate>(lever)
        .expect("RumbleOnActivate must round-trip");
    assert_eq!(restored.camera_intensity, 0.5);
    assert_eq!(restored.duration, 1.5);
    assert!(!restored.repeatable);
    assert_eq!(restored.shake_left, 0.75);
    assert_eq!(restored.shake_right, 0.75);
    assert_eq!(
        restored.state,
        RumbleState::Busy {
            wait_remaining_secs: 0.9
        }
    );
}

/// Regression: #2378 (SAVE-D1-13) — a live `mat.set`-edited `Material`
/// must survive a save/load round trip. Pre-fix, `Material` was absent
/// from `build_save_registry`, so a debug-console material edit
/// silently reverted to whatever the NIF importer produced on reload.
#[test]
fn material_survives_save_load_round_trip() {
    use byroredux_core::ecs::components::material::Material;

    let reg = build_save_registry();
    let mut src = World::new();
    src.insert_resource(StringPool::new());
    src.insert_resource(FormIdPool::new());

    let mesh = src.spawn();
    src.insert(
        mesh,
        Material {
            metalness: 0.9,
            roughness: 0.15,
            alpha: 0.5,
            material_kind: 100, // MATERIAL_KIND_GLASS
            diffuse_color: [0.2, 0.4, 0.6],
            texture_path: Some("textures/edited_by_mat_set.dds".to_owned()),
            ..Material::default()
        },
    );

    let snapshot = save_world(&src, &reg).unwrap();
    let bytes = encode(&snapshot, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).unwrap();

    let restored = dst.get::<Material>(mesh).expect("Material must round-trip");
    assert_eq!(restored.metalness, 0.9);
    assert_eq!(restored.roughness, 0.15);
    assert_eq!(restored.alpha, 0.5);
    assert_eq!(restored.material_kind, 100);
    assert_eq!(restored.diffuse_color, [0.2, 0.4, 0.6]);
    assert_eq!(
        restored.texture_path.as_deref(),
        Some("textures/edited_by_mat_set.dds")
    );
}

/// Regression: #2381 (SAVE-D1-16) — a fragment suspended mid-
/// `Utility.Wait` must survive a save/load round trip and still fire
/// its queued tail afterward. Pre-fix, `FragmentExecutionQueue` was
/// absent from `build_save_registry`, so a save taken mid-wait
/// silently dropped the pending `SetHudCartMode` the wait was gating.
#[test]
fn fragment_execution_queue_survives_save_load_round_trip_and_resumes() {
    use std::sync::{Arc, Mutex};

    use byroredux_scripting::papyrus_demo::PapyrusPlayerEntity;
    use byroredux_scripting::quest_stages::{QuestStageAdvancedBatch, QuestStageState};
    use byroredux_scripting::translate::effects::{Effect, FragmentProviderCall};
    use byroredux_scripting::{
        fragment_continuation_system, quest_fragment_dispatch_system, FragmentExecutionQueue,
        PapyrusProviderCallback, PapyrusProviderCatalog, PapyrusProviderRuntime,
        PlayerControlState, QuestFormId, QuestStageFragments,
    };
    use byroredux_sdk::identity::PrincipalId;
    use byroredux_sdk::script_function::ScriptValue;

    const Q: QuestFormId = QuestFormId(0x0001_2345);
    let principal = PrincipalId::new("legacy.scripts.saved-fragment").unwrap();

    let reg = build_save_registry();
    let mut src = World::new();
    src.insert_resource(StringPool::new());
    src.insert_resource(FormIdPool::new());
    byroredux_scripting::register(&mut src);
    let player = src.spawn();
    src.insert_resource(PapyrusPlayerEntity(player));
    src.insert_resource(QuestStageState::default());

    {
        let mut frags = src.resource_mut::<QuestStageFragments>();
        frags.insert(
            Q,
            10,
            vec![
                Effect::Wait { seconds: 5.0 },
                Effect::ProviderCall(FragmentProviderCall {
                    route: byroredux_sdk::compatibility::PAPYRUS_GAME_GET_MOD_COUNT_ROUTE
                        .to_owned(),
                    arguments: Vec::new(),
                    principal: Some(principal.clone()),
                }),
                Effect::SetHudCartMode { cart_mode: true },
            ],
        );
    }
    src.resource_mut::<QuestStageState>().set_stage(Q, 10);
    {
        let mut q = src.query_mut::<QuestStageAdvancedBatch>().unwrap();
        q.insert(
            player,
            QuestStageAdvancedBatch(vec![
                byroredux_scripting::quest_stages::QuestStageAdvanced {
                    quest: Q,
                    previous_stage: 0,
                    new_stage: 10,
                },
            ]),
        );
    }
    quest_fragment_dispatch_system(&src);
    assert_eq!(
        src.resource::<FragmentExecutionQueue>().len(),
        1,
        "Utility.Wait must suspend into FragmentExecutionQueue"
    );
    assert!(!src.resource::<PlayerControlState>().hud_cart_mode);

    let snapshot = save_world(&src, &reg).unwrap();
    let bytes = encode(&snapshot, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).unwrap();
    dst.insert_resource(PapyrusProviderRuntime::default());
    let calls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_callback = Arc::clone(&calls);
    let callback = Arc::new(
        move |principal: Option<&PrincipalId>, route: &str, _arguments: &[ScriptValue]| {
            calls_for_callback
                .lock()
                .unwrap()
                .push((principal.map(ToString::to_string), route.to_owned()));
            Ok(ScriptValue::Integer(1))
        },
    ) as Arc<PapyrusProviderCallback>;
    byroredux_scripting::set_papyrus_provider_runtime(
        &dst,
        Arc::new(PapyrusProviderCatalog::engine_compatibility()),
        Some(callback),
    );

    assert_eq!(
        dst.resource::<FragmentExecutionQueue>().len(),
        1,
        "FragmentExecutionQueue must round-trip"
    );

    // Resuming after the restored wait still fires the queued tail —
    // proof the round-trip preserved real, resumable effect data, not
    // just an opaque blob.
    fragment_continuation_system(&dst, 5.0);
    assert!(
        dst.resource::<PlayerControlState>().hud_cart_mode,
        "the queued SetHudCartMode effect must still fire after restore"
    );
    assert_eq!(
        *calls.lock().unwrap(),
        vec![(
            Some(principal.to_string()),
            byroredux_sdk::compatibility::PAPYRUS_GAME_GET_MOD_COUNT_ROUTE.to_owned(),
        )],
        "the restored fragment provider call must retain its script owner"
    );
}

#[test]
fn provider_continuation_queue_survives_save_load_and_resumes() {
    use std::sync::{Arc, Mutex};

    use byroredux_papyrus::parse_script;
    use byroredux_scripting::{
        attach_owned_papyrus_provider_program, lower_provider_program, papyrus_provider_system,
        set_papyrus_provider_form_resolver, set_papyrus_provider_mod_event_publisher,
        set_papyrus_provider_runtime, OnCellLoadEvent, PapyrusProviderCallback,
        PapyrusProviderCatalog, PapyrusProviderContinuationQueue,
    };
    use byroredux_sdk::{
        identity::{FormRef, PrincipalId},
        script_function::ScriptValue,
    };

    let reg = build_save_registry();
    let (script, errors) = parse_script(
        "ScriptName SaveFixture extends Quest\n\
         Event OnLoad()\n\
           String pluginName = \"Update.esm\"\n\
           Game.GetModCount()\n\
           Utility.Wait(5.0)\n\
           Game.IsPluginInstalled(pluginName)\n\
           SendModEvent(\"SaveReady\", \"resumed\", 5.0)\n\
         EndEvent\n",
    )
    .unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let catalog = Arc::new(PapyrusProviderCatalog::engine_compatibility());
    let program = lower_provider_program(&script, catalog.as_ref())
        .unwrap()
        .unwrap();

    let mut src = World::new();
    src.insert_resource(StringPool::new());
    let mut form_ids = FormIdPool::new();
    let owner_id = form_ids.intern(byroredux_core::form_id::FormIdPair {
        plugin: byroredux_core::form_id::PluginId::from_filename("Fixture.esm"),
        local: byroredux_core::form_id::LocalFormId(0x1234),
    });
    src.insert_resource(form_ids);
    byroredux_scripting::register(&mut src);
    let callback = Arc::new(
        |_principal: Option<&byroredux_sdk::identity::PrincipalId>,
         _route: &str,
         _arguments: &[ScriptValue]| Ok(ScriptValue::None),
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&src, Arc::clone(&catalog), Some(callback));
    let expected_sender = FormRef::new([5; 16], 0x1234);
    set_papyrus_provider_form_resolver(&src, Some(Arc::new(move |_form_id| Ok(expected_sender))));
    let entity = src.spawn();
    src.insert(
        entity,
        byroredux_core::ecs::components::FormIdComponent(owner_id),
    );
    let principal = PrincipalId::new("legacy.scripts.save-fixture").unwrap();
    attach_owned_papyrus_provider_program(&mut src, entity, program, principal.clone());
    src.insert(entity, OnCellLoadEvent);
    papyrus_provider_system(&src, 0.0);
    assert_eq!(src.resource::<PapyrusProviderContinuationQueue>().len(), 1);

    let snapshot = save_world(&src, &reg).unwrap();
    let bytes = encode(&snapshot, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    byroredux_scripting::register(&mut dst);
    restore_world(&mut dst, &reg, &decoded).unwrap();
    assert_eq!(dst.resource::<PapyrusProviderContinuationQueue>().len(), 1);

    let resumed_calls = Arc::new(Mutex::new(Vec::new()));
    let resumed_calls_for_callback = Arc::clone(&resumed_calls);
    let callback = Arc::new(
        move |principal: Option<&byroredux_sdk::identity::PrincipalId>,
              route: &str,
              arguments: &[ScriptValue]| {
            resumed_calls_for_callback.lock().unwrap().push((
                principal.map(ToString::to_string),
                route.to_owned(),
                arguments.to_vec(),
            ));
            Ok(ScriptValue::None)
        },
    ) as Arc<PapyrusProviderCallback>;
    set_papyrus_provider_runtime(&dst, catalog, Some(callback));
    let resumed_events = Arc::new(Mutex::new(Vec::new()));
    let resumed_events_for_callback = Arc::clone(&resumed_events);
    set_papyrus_provider_mod_event_publisher(
        &dst,
        Some(Arc::new(move |principal, command| {
            resumed_events_for_callback
                .lock()
                .unwrap()
                .push((principal.clone(), command));
            Ok(())
        })),
    );
    papyrus_provider_system(&dst, 5.0);

    assert!(dst
        .resource::<PapyrusProviderContinuationQueue>()
        .is_empty());
    assert_eq!(
        resumed_calls.lock().unwrap().as_slice(),
        &[(
            Some(principal.to_string()),
            byroredux_sdk::compatibility::PAPYRUS_GAME_IS_PLUGIN_INSTALLED_ROUTE.to_owned(),
            vec![ScriptValue::String("Update.esm".to_owned())],
        )]
    );
    let resumed_events = resumed_events.lock().unwrap();
    assert_eq!(resumed_events.len(), 1);
    assert_eq!(resumed_events[0].0, principal);
    let payload =
        byroredux_sdk::event::LegacySkseModEventPayload::decode(&resumed_events[0].1.payload)
            .unwrap();
    assert_eq!(payload.string_arg, "resumed");
    assert_eq!(payload.number_arg(), 5.0);
    assert_eq!(payload.sender, Some(expected_sender));
}

/// Regression: #2380 (SAVE-D1-15) — the MQ101 cinematic fragment-effect
/// state must survive a save/load round trip. Pre-fix,
/// `ActorCinematicState`/`HorseTetherState`/`CinematicPresentationState`
/// were absent from `build_save_registry`, so a save taken mid-cinematic
/// (riding a tethered cart, mid-`PlayIdle`, non-default sitting rotation)
/// silently reverted to default state on reload.
#[test]
fn cinematic_trio_survives_save_load_round_trip() {
    use byroredux_scripting::{ActorCinematicState, CinematicPresentationState, HorseTetherState};

    let reg = build_save_registry();
    let mut src = World::new();
    src.insert_resource(StringPool::new());
    src.insert_resource(FormIdPool::new());

    let vehicle = src.spawn();
    let actor = src.spawn();
    src.insert(
        actor,
        ActorCinematicState {
            vehicle: Some(vehicle),
            vehicle_local_translation: Some(Vec3::new(1.0, 2.0, 3.0)),
            requested_idle_form_id: Some(0xDEAD_BEEF),
            idle_request_serial: 7,
            ..Default::default()
        },
    );

    let horse = src.spawn();
    let cart = src.spawn();
    src.insert(
        cart,
        HorseTetherState {
            horse,
            horse_local_translation: Vec3::new(4.0, 5.0, 6.0),
            horse_local_rotation: byroredux_core::math::Quat::IDENTITY,
            route_target_form_id: Some(0xBEEF),
        },
    );

    let mut presentation = CinematicPresentationState::default();
    presentation.sitting_rotation_degrees = 42.0;
    src.insert_resource(presentation);

    let snapshot = save_world(&src, &reg).unwrap();
    let bytes = encode(&snapshot, reg.schema_fingerprint()).unwrap();
    let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

    let mut dst = World::new();
    dst.insert_resource(FormIdPool::new());
    restore_world(&mut dst, &reg, &decoded).unwrap();

    let restored_actor = dst
        .get::<ActorCinematicState>(actor)
        .expect("ActorCinematicState must round-trip");
    // restore_world preserves entity ids verbatim, so the EntityId
    // reference resolves to the same numeric id it had at save time.
    assert_eq!(restored_actor.vehicle, Some(vehicle));
    assert_eq!(
        restored_actor.vehicle_local_translation,
        Some(Vec3::new(1.0, 2.0, 3.0))
    );
    assert_eq!(restored_actor.requested_idle_form_id, Some(0xDEAD_BEEF));
    assert_eq!(restored_actor.idle_request_serial, 7);

    let restored_tether = dst
        .get::<HorseTetherState>(cart)
        .expect("HorseTetherState must round-trip");
    assert_eq!(restored_tether.horse, horse);
    assert_eq!(
        restored_tether.horse_local_translation,
        Vec3::new(4.0, 5.0, 6.0)
    );

    let restored_presentation = dst
        .try_resource::<CinematicPresentationState>()
        .expect("CinematicPresentationState must round-trip");
    assert_eq!(restored_presentation.sitting_rotation_degrees, 42.0);
}

/// #1835 — every gameplay-state component `NpcSpawnJob` stamps on an
/// NPC placement root must be a deliberate save decision: registered in
/// [`build_save_registry`] (persisted + restored) XOR listed as
/// re-derived-from-static-ESM-at-respawn (write-once, no runtime mutator,
/// so not saving it is a correct no-op). A new spawn-stamp that is neither
/// — or one wrongly in both — trips this test. This is the structural
/// guard the `ActorValues` (#1834) gap lacked, so the pattern can't
/// silently repeat a third time.
///
/// Manually maintained — Rust has no reflection over `world.insert` sites,
/// same tripwire philosophy as `delta_columns_carry_only_session_stable_fields`.
/// When a runtime mutator lands for a re-derived type (leveling XP,
/// `AddPerk`, a faction-rank command), register it AND drop it from the
/// allowlist in the SAME commit (per #1835).
#[test]
fn npc_spawn_stamped_components_are_saved_or_intentionally_rederived() {
    // Persistent gameplay-state components stamped on the placement root by
    // `NpcSpawnJob::advance` + its `stamp_*` helpers (`npc_spawn/resumable.rs`). Pure
    // placement scaffolding (Parent/Children), GPU handles, and transient
    // markers are out of scope — this guards actor state, the #1834 class.
    const NPC_SPAWN_STAMPED: &[&str] = &[
        "Transform",
        "Name",
        "Inventory",
        "EquipmentSlots",
        "ActorValues",
        // #3027 — registered (not re-derived), so it satisfies the XOR
        // below via `saved`, not `REDERIVED_NOT_SAVED`. See the
        // registration-site comment in `save_io.rs` for why it's still
        // excluded from `MUTABLE_DELTA_COLUMNS`.
        "ActorVitals",
        // #3762 — CREA.DATA.Damage, stamped by `stamp_creature_attack`.
        "CreatureAttack",
        "FactionRanks",
        "CharacterLevel",
        "Background",
        "Perks",
        "AmbientPackageRuntime",
    ];
    // Re-derived from static ESM `NPC_` data. Most entries are write-once;
    // AmbientPackageRuntime is the deliberate exception: its first
    // post-load tick recomputes the winner from PKID plus restored
    // clock/CTDA state, so persisting its cached winner is unnecessary.
    //
    // #2947 — CharacterLevel holds only *while no leveling runtime exists*:
    // `npc_spawn.rs` always stamps `CharacterLevel { xp: 0, .. }`, so there
    // is nothing accumulated to lose today. `xp` is CHARAL-defined runtime
    // progress toward the next level — not re-derivable from a static ESM
    // record by construction — so the exempt premise breaks the moment XP
    // starts accumulating. That is not left to this allow-list to notice on
    // its own: `crates/save/src/validate.rs::validate_progression_state`
    // aborts any save where a `CharacterLevel.xp != 0` slips through with it
    // still unregistered, so the exemption fails loudly rather than
    // silently discarding progress.
    //
    // #3491 — Perks holds for a DIFFERENT reason, and `validate_
    // progression_state` does not guard it (it inspects `CharacterLevel`
    // only — grep confirms zero `Perks` references anywhere in
    // `crates/save`). Unlike XP, an ESM-authored NPC's `Perks` is routinely
    // non-empty, so "flag any non-empty Perks" isn't a valid guard the way
    // "flag xp != 0" is — there is no known-safe baseline value to compare
    // against. The exemption instead rests on there being no production
    // mutator at all: `npc_spawn.rs` stamps `Perks` verbatim from `PRKR`
    // and nothing else ever calls `Perks::set_rank`/`try_set_rank` outside
    // `#[cfg(test)]`. Register it — no loud-failure guard needed first — the
    // moment an `AddPerk`-style effect or a perk-selection UI lands
    // (`docs/engine/charal.md`, #3004/#2986).
    const REDERIVED_NOT_SAVED: &[&str] = &[
        "CreatureAttack",
        "FactionRanks",
        "CharacterLevel",
        "Background",
        "Perks",
        // M42.9 — rebuilt from NPC_.PKID plus the restored clock/CTDA
        // state on the first ambient-package tick after a cell reload.
        "AmbientPackageRuntime",
    ];

    let registered: std::collections::HashSet<&str> =
        build_save_registry().component_names().collect();

    for name in NPC_SPAWN_STAMPED {
        let saved = registered.contains(name);
        let rederived = REDERIVED_NOT_SAVED.contains(name);
        assert!(
            saved ^ rederived,
            "NPC-spawn-stamped {name:?}: must be EITHER registered in \
             build_save_registry (saved={saved}) OR in REDERIVED_NOT_SAVED \
             (rederived={rederived}) — never both/neither. If it gained a \
             runtime mutator without deterministic reload re-derivation, \
             register it (#1834); otherwise document that re-derivation \
             here (#1835).",
        );
    }
}

#[test]
fn character_controller_breath_state_survives_live_delta_overlay() {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, LocalFormId, PluginId};
    use byroredux_physics::CharacterController;

    let pair = FormIdPair {
        plugin: PluginId::from_filename("Player.esm"),
        local: LocalFormId(0x14),
    };
    let registry = build_save_registry();
    let mut saved = World::new();
    saved.insert_resource(StringPool::new());
    saved.insert_resource(FormIdPool::new());
    let old = saved.spawn();
    let fid = saved.resource_mut::<FormIdPool>().intern(pair);
    saved.insert(old, FormIdComponent(fid));
    let mut controller = CharacterController::HUMAN;
    controller.breath_remaining = 12.25;
    controller.drowning_damage_accumulator = 0.625;
    saved.insert(old, controller);
    let snapshot = save_world(&saved, &registry).unwrap();

    let mut live = World::new();
    live.insert_resource(FormIdPool::new());
    let current = live.spawn();
    let fid = live.resource_mut::<FormIdPool>().intern(pair);
    live.insert(current, FormIdComponent(fid));
    let mut stale = CharacterController::HUMAN;
    stale.breath_remaining = 0.2;
    stale.drowning_damage_accumulator = 9.0;
    live.insert(current, stale);

    let remap = byroredux_save::build_form_id_remap(&live, &registry, &snapshot);
    byroredux_save::apply_deltas(
        &mut live,
        &registry,
        &snapshot,
        &remap,
        MUTABLE_DELTA_COLUMNS,
    )
    .unwrap();
    let query = live.query::<CharacterController>().unwrap();
    let restored = query.get(current).unwrap();
    assert_eq!(restored.breath_remaining, 12.25);
    assert_eq!(restored.drowning_damage_accumulator, 0.625);
}
