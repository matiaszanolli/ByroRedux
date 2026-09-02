//! Unit tests for the debug-console command dispatcher. Extracted
//! from `commands.rs` to keep the production code under ~1400 lines;
//! pulled in via `#[cfg(test)] #[path = "..."] mod tests;`.

use super::*;
use byroredux_core::ecs::components::{GlobalTransform, Name, SkinnedMesh, Transform};
use byroredux_core::ecs::World;
use byroredux_core::math::{Quat, Vec3};

#[test]
fn sdk_compat_command_is_registered_and_reports_an_empty_world() {
    let mut world = World::new();
    world.insert_resource(byroredux_scripting::CompatibilityRegistry::default());

    let output = SdkCompatCommand.execute(&world, "").lines.join("\n");
    assert!(output.contains("0 unique script(s)"), "{output}");
    assert!(
        output.contains("no extender-era calls observed"),
        "{output}"
    );

    let registry = build_command_registry();
    assert!(registry
        .list()
        .iter()
        .any(|(name, _)| *name == "sdk.compat"));
}

#[test]
fn sdk_compat_command_formats_engine_service_mapping() {
    let entry = byroredux_scripting::CompatibilitySummaryEntry {
        provider: "storageutil".to_string(),
        function: "getintvalue".to_string(),
        compatibility: byroredux_scripting::classify_static_call("StorageUtil", "GetIntValue")
            .expect("catalogued extender call"),
        occurrences: 1,
        scripts: 1,
    };
    let output = format_sdk_compat_entry(&entry);
    assert!(
        output.contains("native      storageutil.getintvalue"),
        "{output}"
    );
    assert!(output.contains("service=byro.storage"), "{output}");
    assert!(output.contains("alias=storage.get<signed>"), "{output}");
    assert!(output.contains("ObjKey must be None"), "{output}");
}

#[test]
fn render_debug_command_queues_named_mode_and_bounded_probe_pixel() {
    let mut world = World::new();
    world.insert_resource(crate::components::RenderDebugControl::default());

    let output = RenderDebugCommand
        .execute(&world, "shadow_visibility 320 180")
        .lines
        .join("\n");
    assert!(output.contains("mode=shadow_visibility"));
    assert!(output.contains("probe=(320, 180)"));

    let control = world.resource::<crate::components::RenderDebugControl>();
    assert_eq!(
        control.pending_mode,
        Some(byroredux_renderer::RenderDebugMode::ShadowVisibility)
    );
    assert_eq!(control.pending_probe_pixel, Some([320, 180]));
}

#[test]
fn render_debug_command_rejects_unknown_mode_and_incomplete_probe() {
    let mut world = World::new();
    world.insert_resource(crate::components::RenderDebugControl::default());

    let unknown = RenderDebugCommand
        .execute(&world, "banana")
        .lines
        .join("\n");
    assert!(unknown.contains("unknown render debug mode"));

    let incomplete = RenderDebugCommand
        .execute(&world, "probe 10")
        .lines
        .join("\n");
    assert!(incomplete.contains("expected `render.debug"));
}

#[test]
fn input_hold_command_enters_through_the_action_binding() {
    use crate::components::InputState;
    use crate::interaction::{
        ActionBindings, ActionState, InjectedKeyHold, InjectedKeyPulse, InputAction,
    };

    let mut world = World::new();
    world.insert_resource(InputState::default());
    world.insert_resource(ActionBindings::default());
    world.insert_resource(ActionState::default());
    world.insert_resource(InjectedKeyPulse::default());
    world.insert_resource(InjectedKeyHold::default());

    let output = InputHoldCommand
        .execute(&world, "forward 2")
        .lines
        .join("\n");
    assert!(output.contains("Move forward through the W binding for 2 frames"));

    crate::interaction::refresh_action_state(&world);
    assert!(world
        .resource::<ActionState>()
        .is_held(InputAction::MoveForward));
    crate::interaction::refresh_action_state(&world);
    assert!(world
        .resource::<ActionState>()
        .is_held(InputAction::MoveForward));
    crate::interaction::refresh_action_state(&world);
    assert!(world
        .resource::<ActionState>()
        .was_released(InputAction::MoveForward));
}

#[test]
fn input_look_updates_the_gameplay_accumulator_without_moving_the_camera() {
    use crate::components::InputState;

    let mut world = World::new();
    world.insert_resource(InputState::default());
    let camera = world.spawn();
    world.insert(
        camera,
        Transform::from_translation(Vec3::new(1.0, 2.0, 3.0)),
    );
    world.insert_resource(ActiveCamera(camera));

    let output = InputLookCommand.execute(&world, "-45 120").lines.join("\n");
    assert!(output.contains("yaw=-45.0° pitch=89.0°"));
    let input = world.resource::<InputState>();
    assert!((input.yaw.to_degrees() + 45.0).abs() < 0.01);
    assert!((input.pitch.to_degrees() - 89.0).abs() < 0.01);
    drop(input);
    assert_eq!(
        world.get::<Transform>(camera).unwrap().translation,
        Vec3::new(1.0, 2.0, 3.0)
    );
}

#[test]
fn player_status_reports_character_body_grid_and_grounding() {
    use crate::systems::{PlayerEntity, PlayerMode};

    let mut world = World::new();
    let player = world.spawn();
    world.insert(
        player,
        Transform::from_translation(Vec3::new(24_650.0, 120.0, 7_900.0)),
    );
    let mut controller = byroredux_physics::CharacterController::HUMAN;
    controller.is_grounded = true;
    world.insert(player, controller);
    world.insert_resource(PlayerEntity(Some(player)));
    world.insert_resource(PlayerMode::Character);
    world.insert_resource(ActiveCamera(player));

    let output = PlayerStatusCommand.execute(&world, "").lines.join("\n");
    assert!(output.contains("mode=Character"));
    assert!(output.contains("grid=(6,-2)"));
    assert!(output.contains("grounded=true"));
    assert!(output.contains("camera=(24650.00, 120.00, 7900.00)"));
    assert!(output.contains("input_hold_frames_remaining=0"));
}

#[test]
fn combat_approach_search_is_deterministic_and_stays_inside_melee_reach() {
    let offsets: Vec<Vec3> = super::view::combat_approach_offsets().collect();
    assert_eq!(offsets.len(), 48);
    assert_eq!(offsets[0], Vec3::new(0.0, 0.0, -120.0));
    assert!(offsets.iter().all(|offset| {
        let radius = offset.length();
        offset.y == 0.0
            && radius <= super::view::COMBAT_APPROACH_MAX_RAY_BU
            && [96.0_f32, 120.0, 144.0]
                .iter()
                .any(|expected| (radius - expected).abs() < 0.01)
    }));
}

/// `skin.dump` regression for #841 — the dump must surface the
/// resolved bone entity, its `Name`, the per-bone `bind_inverse`
/// translation, and the composed palette translation in the
/// summary table. Identity-fallback bones (no GT for the bone
/// entity) print `(no GT)` and `palette: identity` so the
/// `SKIN_DROPOUT_DUMPED` slots are obvious in the dump.
#[test]
fn skin_dump_renders_resolved_bone_with_world_and_palette() {
    let mut world = World::new();
    let mut pool = StringPool::new();
    let bip01 = pool.intern("Bip01 Spine");
    world.insert_resource(pool);

    // Bone entity: positioned at world (0, 5, 0), with a Name.
    let bone = world.spawn();
    world.insert(bone, Transform::from_translation(Vec3::new(0.0, 5.0, 0.0)));
    world.insert(
        bone,
        GlobalTransform::new(Vec3::new(0.0, 5.0, 0.0), Quat::IDENTITY, 1.0),
    );
    world.insert(bone, Name(bip01));

    // Skinned mesh: one bone, bind_inverse cancels the bind-pose
    // translation so palette = world * bind_inv = identity (the
    // canonical "bone hasn't moved relative to bind" case).
    let bind_inv = Mat4::from_translation(Vec3::new(0.0, -5.0, 0.0));
    let skin_entity = world.spawn();
    let skin =
        SkinnedMesh::new_with_global(Some(bone), vec![Some(bone)], vec![bind_inv], Mat4::IDENTITY);
    let lines = format_skin_dump(&world, skin_entity, &skin);
    let dump = lines.join("\n");

    // Header — dump is for the right entity and bone count.
    assert!(
        dump.contains(&format!("dump for entity {} (1 bones)", skin_entity)),
        "header missing or wrong: {}",
        dump
    );
    // Bone slot 0 row: shows resolved bone entity + Name +
    // world_t (0, 5, 0) + bind_inv_t (0, -5, 0) + palette_t (0, 0, 0).
    assert!(dump.contains("bip01 spine"), "Name missing: {}", dump);
    assert!(
        dump.contains("(0.00,5.00,0.00)"),
        "world translation missing: {}",
        dump
    );
    assert!(
        dump.contains("(0.00,-5.00,0.00)"),
        "bind_inv translation missing: {}",
        dump
    );
    // palette = T(0,5,0) * T(0,-5,0) = identity → translation (0,0,0).
    assert!(
        dump.contains("(0.00,0.00,0.00)"),
        "palette translation missing: {}",
        dump
    );
}

#[test]
fn skin_dump_marks_unresolved_bone_slots() {
    // Phase 1b.x DROPOUT scenario — a bone slot that didn't
    // resolve to an entity must show up as `(None)` /
    // `(unresolved)` so the operator can correlate against the
    // SKIN_DROPOUT_DUMPED warn.
    let mut world = World::new();
    world.insert_resource(StringPool::new());
    let skin_entity = world.spawn();
    let skin = SkinnedMesh::new_with_global(None, vec![None], vec![Mat4::IDENTITY], Mat4::IDENTITY);
    let lines = format_skin_dump(&world, skin_entity, &skin);
    let dump = lines.join("\n");
    assert!(
        dump.contains("(None)"),
        "unresolved entity missing: {}",
        dump
    );
    assert!(
        dump.contains("(unresolved)"),
        "unresolved name missing: {}",
        dump
    );
}

#[test]
fn skin_dump_reports_non_identity_global_skin_transform() {
    // A non-identity `global_skin_transform` is informational
    // (not multiplied at runtime) but its presence is exactly
    // the kind of authoring quirk #841 surfaced on Doc Mitchell;
    // the dump must call it out so it isn't missed.
    let mut world = World::new();
    world.insert_resource(StringPool::new());
    let skin_entity = world.spawn();
    let global = Mat4::from_quat(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2));
    let skin = SkinnedMesh::new_with_global(None, vec![], vec![], global);
    let lines = format_skin_dump(&world, skin_entity, &skin);
    let dump = lines.join("\n");
    assert!(
        dump.contains("global_skin_transform: NON-IDENTITY"),
        "non-identity global must be flagged: {}",
        dump
    );
}

/// `light.dump` smoke test — exercises both the "no resource"
/// branches (the cold start before any cell is loaded) and the
/// populated branches, so the command is callable from `byro-dbg`
/// at any point in the engine's lifetime without panicking.
#[test]
fn light_dump_handles_missing_and_present_resources() {
    use crate::components::{CellLightingRes, GameTimeRes, SkyParamsRes};

    // Cold start — no resources inserted yet.
    let mut world = World::new();
    let cmd = LightDumpCommand;
    let lines = cmd.execute(&world, "").lines;
    let joined = lines.join("\n");
    assert!(
        joined.contains("CellLightingRes: <not present"),
        "cold start should flag CellLightingRes absence: {}",
        joined
    );
    assert!(
        joined.contains("SkyParamsRes: <not present"),
        "cold start should flag SkyParamsRes absence: {}",
        joined
    );
    assert!(
        joined.contains("GameTimeRes: <not present>"),
        "cold start should flag GameTimeRes absence: {}",
        joined
    );

    // Populated — Markarth-procedural-fallback-shaped values, so
    // the test pins the format the Markarth investigation actually
    // reads.
    world.insert_resource(CellLightingRes {
        ambient: [0.15, 0.14, 0.12],
        directional_color: [1.0, 0.95, 0.8],
        directional_dir: [-0.4, 0.8, -0.45],
        is_interior: false,
        fog_color: [0.65, 0.7, 0.8],
        fog_near: 15000.0,
        fog_far: 80000.0,
        fog_medium: crate::fog::FogMedium::from_legacy_ramp(15000.0, 80000.0, None),
        directional_fade: None,
        fog_clip: None,
        fog_power: None,
        fog_far_color: None,
        fog_max: None,
        light_fade_begin: None,
        light_fade_end: None,
        directional_ambient: None,
        specular_color: None,
        specular_alpha: None,
        fresnel_power: None,
        inheritance_flags: None,
    });
    world.insert_resource(SkyParamsRes {
        zenith_color: [0.15, 0.3, 0.65],
        horizon_color: [0.55, 0.5, 0.42],
        lower_color: [0.165, 0.15, 0.126],
        sun_direction: [-0.4, 0.8, -0.45],
        sun_color: [1.0, 0.95, 0.8],
        sun_size: 0.9995,
        sun_intensity: 4.0,
        sun_angular_radius: 0.020,
        is_exterior: true,
        cloud_tile_scale: 0.0,
        cloud_texture_index: 0,
        sun_texture_index: 0,
        cloud_tile_scale_1: 0.0,
        cloud_texture_index_1: 0,
        cloud_tile_scale_2: 0.0,
        cloud_texture_index_2: 0,
        cloud_tile_scale_3: 0.0,
        cloud_texture_index_3: 0,
        current_dalc_cube: None,
    });
    world.insert_resource(GameTimeRes::new(10.5, 30.0));
    let emitter = world.spawn();
    world.insert(
        emitter,
        GlobalTransform::new(Vec3::new(1.0, 2.0, 3.0), Quat::IDENTITY, 1.0),
    );
    world.insert(emitter, LightSource::default());

    let lines = cmd.execute(&world, "").lines;
    let joined = lines.join("\n");
    // Ambient + sun_intensity are the two key numbers the Markarth
    // probe needs to read — pin both so output drift breaks the test.
    assert!(
        joined.contains("ambient            = [0.150, 0.140, 0.120]"),
        "ambient must print 3-decimal float triple: {}",
        joined
    );
    assert!(
        joined.contains("sun_intensity      = 4.000"),
        "sun_intensity must print 3-decimal float: {}",
        joined
    );
    // GameTime wall-clock conversion — 10.5 should print "10:30 AM".
    assert!(
        joined.contains("10:30 AM"),
        "GameTimeRes hour=10.5 should print '10:30 AM': {}",
        joined
    );
    // is_exterior true + sun_texture_index 0 should annotate
    // "procedural disc fallback" so a missing CLMT FNAM is obvious.
    assert!(
        joined.contains("procedural disc fallback"),
        "sun_texture_index=0 must annotate procedural fallback: {}",
        joined
    );
    assert!(
        joined.contains("LightSource emitters: 1")
            && joined.contains(&format!("entity={emitter}"))
            && joined.contains("kind=Point")
            && joined.contains("source=nif/synthetic"),
        "live emitter provenance row missing: {}",
        joined
    );
}

/// `look_at_yaw_pitch` must produce a (yaw, pitch) pair such that
/// composing the fly-camera quaternion (`Q_y(yaw) * Q_x(pitch)`)
/// applied to `-Z` yields the unit direction from `from` to `to`.
/// Tests pick directions on the six cardinal axes — full coverage
/// of the yaw/pitch sign convention.
fn forward_from(yaw: f32, pitch: f32) -> Vec3 {
    let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
    rot * (-Vec3::Z)
}

fn assert_forward_matches(from: Vec3, to: Vec3) {
    let (yaw, pitch) = look_at_yaw_pitch(from, to);
    let want = (to - from).normalize();
    let got = forward_from(yaw, pitch);
    assert!(
        (got - want).length() < 1e-3,
        "look_at_yaw_pitch({from:?} -> {to:?}) yielded forward {got:?}, want {want:?}",
    );
}

#[test]
fn look_at_minus_z_is_identity_rotation() {
    // Default fly-camera forward is -Z; looking at -Z from origin
    // must produce yaw=0, pitch=0.
    let (yaw, pitch) = look_at_yaw_pitch(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0));
    assert!(yaw.abs() < 1e-3, "yaw should be 0, got {yaw}");
    assert!(pitch.abs() < 1e-3, "pitch should be 0, got {pitch}");
}

#[test]
fn look_at_cardinal_axes_round_trip_through_quat() {
    assert_forward_matches(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0));
    assert_forward_matches(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0));
    assert_forward_matches(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0));
    assert_forward_matches(Vec3::ZERO, Vec3::new(-1.0, 0.0, 0.0));
    assert_forward_matches(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.001));
    assert_forward_matches(Vec3::ZERO, Vec3::new(0.0, -1.0, 0.001));
}

#[test]
fn look_at_offset_origin_round_trips() {
    // Camera at (10, 5, 200), target at (0, 0, 0): forward should
    // point toward origin and through-quat must reproduce that
    // direction.
    assert_forward_matches(Vec3::new(10.0, 5.0, 200.0), Vec3::ZERO);
    // The cam.tp default framing: 200 back + 50 up.
    let target = Vec3::new(100.0, 0.0, 0.0);
    let camera = target + Vec3::new(0.0, 50.0, 200.0);
    assert_forward_matches(camera, target);
}

#[test]
fn prid_sets_selected_ref_resource() {
    // Spawn an entity with Transform, run `prid <id>`, verify the
    // SelectedRef resource is updated. Output line should name the
    // entity. No-arg `prid` after the set should report the same.
    let mut world = World::new();
    world.insert_resource(SelectedRef::default());
    world.insert_resource(StringPool::new());

    let target = world.spawn();
    world.insert(
        target,
        Transform::from_translation(Vec3::new(1.0, 2.0, 3.0)),
    );

    let cmd = PridCommand;
    let out = cmd.execute(&world, &target.to_string()).lines.join("\n");
    assert!(
        out.contains(&format!("selected: entity {}", target)),
        "expected 'selected: entity {target}' in output: {out}"
    );

    // Resource state should now hold Some(target).
    let sel = world.resource::<SelectedRef>();
    assert_eq!(sel.0, Some(target));
    drop(sel);

    // `prid` with no args prints the current selection.
    let out2 = cmd.execute(&world, "").lines.join("\n");
    assert!(
        out2.contains(&format!("selected: entity {}", target)),
        "no-arg prid should print current selection: {out2}"
    );
}

#[test]
fn prid_rejects_entity_without_transform_or_global_transform() {
    // Entities that exist in the slot table but have no Transform
    // AND no GlobalTransform are conservatively rejected — usually
    // a sign of a typo or a hierarchy-orphan that wouldn't show
    // up in `entities`. The error should mention the id.
    let mut world = World::new();
    world.insert_resource(SelectedRef::default());
    world.insert_resource(StringPool::new());

    let orphan = world.spawn();
    // No Transform / GlobalTransform inserted.

    let cmd = PridCommand;
    let out = cmd.execute(&world, &orphan.to_string()).lines.join("\n");
    assert!(
        out.contains("no Transform/GlobalTransform"),
        "expected rejection message naming missing components: {out}"
    );

    // Resource must be untouched.
    let sel = world.resource::<SelectedRef>();
    assert!(
        sel.0.is_none(),
        "SelectedRef should remain None on rejected prid"
    );
}

#[test]
fn cam_tp_no_args_uses_selected_ref() {
    // Set up: ActiveCamera with a Transform, a target with a
    // GlobalTransform, SelectedRef pointing at the target. `cam.tp`
    // with no args should treat the SelectedRef as if it were the
    // explicit argument and move the camera.
    let mut world = World::new();
    world.insert_resource(StringPool::new());

    let camera = world.spawn();
    world.insert(camera, Transform::from_translation(Vec3::ZERO));
    world.insert_resource(byroredux_core::ecs::ActiveCamera(camera));

    let target = world.spawn();
    let target_pos = Vec3::new(100.0, 0.0, 0.0);
    world.insert(
        target,
        GlobalTransform::new(target_pos, Quat::IDENTITY, 1.0),
    );

    world.insert_resource(SelectedRef(Some(target)));
    // InputState must exist because cam.tp tries to update it.
    world.insert_resource(InputState::default());

    let cmd = CamTpCommand;
    let out = cmd.execute(&world, "").lines.join("\n");
    assert!(
        out.contains(&format!("entity {target}")),
        "cam.tp w/o args should target SelectedRef ({target}): {out}"
    );

    // The camera transform should have moved away from origin.
    let tq = world.query::<Transform>().unwrap();
    let cam_t = tq.get(camera).unwrap();
    assert_ne!(cam_t.translation, Vec3::ZERO);
}

#[test]
fn cam_tp_no_args_no_selection_reports_usage() {
    // If SelectedRef is empty AND no arg is given, point the user
    // at both forms — direct (`cam.tp <id>`) and prid-first.
    let mut world = World::new();
    let camera = world.spawn();
    world.insert(camera, Transform::from_translation(Vec3::ZERO));
    world.insert_resource(byroredux_core::ecs::ActiveCamera(camera));
    world.insert_resource(SelectedRef::default());
    world.insert_resource(InputState::default());

    let cmd = CamTpCommand;
    let out = cmd.execute(&world, "").lines.join("\n");
    assert!(
        out.contains("usage:") && out.contains("prid"),
        "no-selection / no-arg should hint at prid workflow: {out}"
    );
}

#[test]
fn combat_approach_positions_and_aims_the_real_player_without_attacking() {
    let mut world = World::new();
    world.insert_resource(InputState::default());
    world.insert_resource(crate::combat::CombatState::default());

    let camera = world.spawn();
    world.insert(camera, Transform::from_translation(Vec3::ZERO));
    world.insert_resource(ActiveCamera(camera));

    let player = world.spawn();
    world.insert(player, Transform::from_translation(Vec3::ZERO));
    world.insert(player, byroredux_physics::CharacterController::HUMAN);
    world.insert_resource(crate::systems::PlayerEntity(Some(player)));

    let target = world.spawn();
    let target_pos = Vec3::new(100.0, 20.0, 300.0);
    world.insert(
        target,
        GlobalTransform::new(target_pos, Quat::IDENTITY, 1.0),
    );
    world.insert(
        target,
        byroredux_core::ecs::components::ActorVitals {
            health: 0x0000_03E8,
        },
    );

    let out = CombatApproachCommand
        .execute(&world, &target.to_string())
        .lines
        .join("\n");
    assert!(out.contains(&format!("entity {target}")), "{out}");
    assert!(
        out.contains("physics_synced=false"),
        "fixture has no Rapier body: {out}"
    );

    let transforms = world.query::<Transform>().unwrap();
    assert_eq!(
        transforms.get(player).unwrap().translation,
        target_pos - Vec3::Z * 120.0 + Vec3::Y * 64.0
    );
    assert_eq!(
        transforms.get(camera).unwrap().translation,
        target_pos - Vec3::Z * 120.0 + Vec3::Y * 116.0
    );
    drop(transforms);
    let input = world.resource::<InputState>();
    assert!(input.yaw.is_finite() && input.pitch.is_finite());
    let combat = world.resource::<crate::combat::CombatState>();
    assert_eq!(combat.attacks_started, 0);
    assert_eq!(combat.hits_landed, 0);
}

#[test]
fn look_at_degenerate_zero_distance_returns_zero() {
    // Target equals source — no meaningful direction; return zero
    // instead of producing NaN or an arbitrary unit vector.
    let (yaw, pitch) = look_at_yaw_pitch(Vec3::new(5.0, 5.0, 5.0), Vec3::new(5.0, 5.0, 5.0));
    assert_eq!(yaw, 0.0);
    assert_eq!(pitch, 0.0);
}

/// `mat.set` mutates a scalar field and a vec3 field in place — the core
/// of the Cornell-box live material-sweep workflow. The render path reads
/// `Material` fresh each frame, so an in-place edit is all that's needed.
#[test]
fn mat_set_mutates_scalar_and_vec3() {
    let mut world = World::new();
    world.insert_resource(StringPool::new());
    let e = world.spawn();
    world.insert(e, Material::default());

    let cmd = MatSetCommand;
    let out = cmd
        .execute(&world, &format!("{e} roughness 0.25"))
        .lines
        .join("\n");
    assert!(out.contains("roughness = 0.2500"), "got: {out}");
    assert_eq!(world.get::<Material>(e).unwrap().roughness, 0.25);

    // `color` is the alias for diffuse_color, 3 values.
    cmd.execute(&world, &format!("{e} color 0.1 0.2 0.3"));
    assert_eq!(
        world.get::<Material>(e).unwrap().diffuse_color,
        [0.1, 0.2, 0.3]
    );

    // material_kind takes an integer arm.
    cmd.execute(&world, &format!("{e} material_kind 100"));
    assert_eq!(world.get::<Material>(e).unwrap().material_kind, 100);
}

/// Regression for #2249 (REN-D21-03): `ior` (and its `distortion_strength`
/// alias) must be reachable via `mat.set` — `MATERIAL_KIND_FIRE_REFRACTION`
/// overloads this field as its authored distortion strength, and before
/// this fix `mat.set` had no way to reach it at all, so every
/// fire-refraction gap had to be found by static code reading instead of
/// the Cornell harness.
#[test]
fn mat_set_reaches_ior_field() {
    let mut world = World::new();
    world.insert_resource(StringPool::new());
    let e = world.spawn();
    world.insert(e, Material::default());
    let cmd = MatSetCommand;

    let out = cmd
        .execute(&world, &format!("{e} ior 0.6"))
        .lines
        .join("\n");
    assert!(out.contains("ior = 0.6000"), "got: {out}");
    assert_eq!(world.get::<Material>(e).unwrap().ior, 0.6);

    cmd.execute(&world, &format!("{e} distortion_strength 0.9"));
    assert_eq!(world.get::<Material>(e).unwrap().ior, 0.9);
}

/// `mat.set` rejects unknown fields, wrong value arity, and missing
/// entities without mutating anything.
#[test]
fn mat_set_validates_input() {
    let mut world = World::new();
    world.insert_resource(StringPool::new());
    let e = world.spawn();
    world.insert(e, Material::default());
    let cmd = MatSetCommand;

    let unknown = cmd
        .execute(&world, &format!("{e} bogus 1.0"))
        .lines
        .join("\n");
    assert!(unknown.contains("unknown field"), "got: {unknown}");

    let arity = cmd
        .execute(&world, &format!("{e} color 0.5"))
        .lines
        .join("\n");
    assert!(arity.contains("expected 3"), "got: {arity}");

    let missing = cmd.execute(&world, "999999 roughness 0.5").lines.join("\n");
    assert!(missing.contains("no Material"), "got: {missing}");

    // None of the bad inputs touched the component.
    let m = world.get::<Material>(e).unwrap();
    assert_eq!(m.roughness, Material::default().roughness);
    assert_eq!(m.diffuse_color, Material::default().diffuse_color);
}

/// `mat.list` tabulates every entity carrying a Material, sorted by id,
/// with the resolved name.
#[test]
fn mat_list_tabulates_materials() {
    let mut world = World::new();
    let mut pool = StringPool::new();
    let probe = pool.intern("probe_a");
    world.insert_resource(pool);

    let e = world.spawn();
    world.insert(e, Material::default());
    world.insert(e, Name(probe));

    let out = MatListCommand.execute(&world, "").lines.join("\n");
    assert!(
        out.contains(&e.to_string()),
        "row for entity missing: {out}"
    );
    assert!(out.contains("probe_a"), "name missing: {out}");
    assert!(out.contains("kind"), "header missing: {out}");
}

#[test]
fn mat_dump_reports_texture_path_provenance_and_binding_contract() {
    use byroredux_nif::import::MaterialTextureSet;

    let mut world = World::new();
    world.insert_resource(StringPool::new());
    let entity = world.spawn();
    let mut material = Material::default();
    material.effect_shader_flags = byroredux_renderer::vulkan::material::material_flag::PBR_BSDF
        | byroredux_renderer::vulkan::material::material_flag::MODEL_SPACE_NORMALS;
    world.insert(entity, material);

    let mut paths = MaterialTextureSet::default();
    paths.base_color = Some(r"textures\architecture\wall_d.dds".to_string());
    paths.normal = Some(r"textures\architecture\wall_n.dds".to_string());
    paths.environment = Some(r"textures\cubemaps\interior.dds".to_string());
    let mut sources = MaterialTextureSet::default();
    sources.base_color = MaterialTextureSource::MeshMaterial;
    sources.normal = MaterialTextureSource::DerivedNormal;
    sources.environment = MaterialTextureSource::TxstOverride;
    world.insert(
        entity,
        MaterialTextureDebugInfo {
            paths,
            sources,
            clamp_mode: 2,
        },
    );
    let mut handles = MaterialTextureSet::default();
    handles.base_color = 17;
    handles.normal = 18;
    handles.environment = 19;
    world.insert(
        entity,
        MaterialTextureHandles {
            textures: handles,
            normal_has_alpha: true,
            parallax_height_scale: 0.04,
            parallax_max_passes: 4.0,
        },
    );

    let out = MatDumpCommand
        .execute(&world, &entity.to_string())
        .lines
        .join("\n");
    assert!(out.contains("flags=0x000000a0"), "flag word missing: {out}");
    assert!(out.contains("lobe=disney-pbr"), "lobe missing: {out}");
    assert!(
        out.contains("base_color") && out.contains("mesh-material") && out.contains(r"wall_d.dds"),
        "base slot provenance missing: {out}"
    );
    assert!(
        out.contains("derived-normal") && out.contains(r"wall_n.dds"),
        "derived normal provenance missing: {out}"
    );
    assert!(
        out.contains("environment")
            && out.contains("set=0 binding=1")
            && out.contains("txst-override")
            && out.contains("cube"),
        "cubemap binding/provenance missing: {out}"
    );
}

// ── `world.owners` — EX-08 ownership soak surface (#2374) ──────────

/// Drive the command the way the soak harness does, through `execute`
/// rather than the tracker API, so the operator-facing contract (arg
/// parsing, missing-baseline guard, resource wiring) is covered rather
/// than only the accounting rules underneath it.
fn owners_world() -> World {
    let mut world = World::new();
    world.insert_resource(byroredux_core::ecs::OwnershipTelemetry::default());
    world.insert_resource(OwnershipTracker::new());
    world
}

#[test]
fn world_owners_lists_every_class_with_its_policy() {
    let world = owners_world();
    let out = WorldOwnersCommand.execute(&world, "");
    let text = out.lines.join("\n");
    for class in byroredux_core::ecs::OwnershipSnapshot::default().classes() {
        assert!(text.contains(class.name), "missing class {}", class.name);
    }
    // Both policies must be legible in the default listing — an operator
    // reading a surplus needs to know whether it is a leak or a documented
    // retain before escalating.
    assert!(text.contains("exact"));
    assert!(text.contains("bounded"));
}

#[test]
fn world_owners_cycle_without_baseline_is_refused() {
    // Recording cycles against no baseline would silently produce a report
    // with nothing to compare against, which reads as a pass.
    let world = owners_world();
    let out = WorldOwnersCommand.execute(&world, "cycle");
    assert!(
        out.lines.join("\n").contains("no baseline"),
        "{:?}",
        out.lines
    );
    assert_eq!(world.resource::<OwnershipTracker>().cycles().len(), 0);
}

#[test]
fn world_owners_baseline_then_cycles_reaches_a_verdict() {
    let world = owners_world();
    WorldOwnersCommand.execute(&world, "baseline");
    assert!(world.resource::<OwnershipTracker>().baseline().is_some());

    for expected in 1..=3 {
        let out = WorldOwnersCommand.execute(&world, "cycle");
        assert!(
            out.lines[0].contains(&format!("cycle {} recorded", expected)),
            "{:?}",
            out.lines
        );
    }
    // A static world reclaims trivially, so the verdict must be PASS — the
    // gate has to stay quiet when nothing leaks or it will be ignored.
    let report = WorldOwnersCommand
        .execute(&world, "report")
        .lines
        .join("\n");
    assert!(report.contains("ownership: PASS"), "{report}");
}

#[test]
fn world_owners_baseline_restarts_a_stale_run() {
    // Re-baselining mid-session must discard the previous run's cycles,
    // otherwise a second soak inherits the first one's history and its
    // monotonic-growth series spans two unrelated runs.
    let world = owners_world();
    WorldOwnersCommand.execute(&world, "baseline");
    WorldOwnersCommand.execute(&world, "cycle");
    WorldOwnersCommand.execute(&world, "cycle");
    assert_eq!(world.resource::<OwnershipTracker>().cycles().len(), 2);

    WorldOwnersCommand.execute(&world, "baseline");
    assert_eq!(world.resource::<OwnershipTracker>().cycles().len(), 0);
    assert!(world.resource::<OwnershipTracker>().baseline().is_some());
}

#[test]
fn world_owners_reset_clears_the_baseline_too() {
    let world = owners_world();
    WorldOwnersCommand.execute(&world, "baseline");
    WorldOwnersCommand.execute(&world, "cycle");
    WorldOwnersCommand.execute(&world, "reset");
    let tracker = world.resource::<OwnershipTracker>();
    assert!(tracker.baseline().is_none());
    assert_eq!(tracker.cycles().len(), 0);
}

#[test]
fn world_owners_rejects_an_unknown_subcommand() {
    // A typo must not silently fall through to the snapshot listing — the
    // harness would then record a "pass" for a step that never ran.
    let world = owners_world();
    let out = WorldOwnersCommand.execute(&world, "basline");
    assert!(out.lines.join("\n").contains("unknown subcommand"));
}

#[test]
fn world_owners_detects_a_leaked_ecs_owner_end_to_end() {
    // The full EX-08 shape through the command surface: baseline clean,
    // then leave entities resident and record a cycle. `transform_rows` is
    // an exact-return class, so the surplus must surface as a LEAK.
    let mut world = owners_world();
    WorldOwnersCommand.execute(&world, "baseline");

    for _ in 0..5 {
        let e = world.spawn();
        world.insert(e, Transform::default());
    }
    WorldOwnersCommand.execute(&world, "cycle");

    let report = WorldOwnersCommand
        .execute(&world, "report")
        .lines
        .join("\n");
    assert!(report.contains("LEAK"), "{report}");
    assert!(
        report.contains("ownership: FAIL NOT-RECLAIMED transform_rows"),
        "{report}"
    );
}

// ── `r.health` — EX-05 pre-tonemap image health (#2736) ───────────

#[test]
fn r_health_reports_clean_when_nothing_non_finite_has_appeared() {
    let mut world = World::new();
    world.insert_resource(byroredux_core::ecs::ImageHealth::default());
    let out = RenderHealthCommand.execute(&world, "");
    let text = out.lines.join("\n");
    assert!(text.contains("CLEAN"), "{text}");
    // Both horizons must be visible — an operator triaging a NaN needs to know
    // whether it is happening now or happened earlier in the run.
    assert!(text.contains("last frame"));
    assert!(text.contains("since startup"));
}

#[test]
fn r_health_flags_a_historical_nan_even_when_the_current_frame_is_clean() {
    // The reason the running total exists: a NaN is usually transient, present
    // only while a bad material or degenerate light is on screen. Reporting
    // only the current frame would let the gate pass a run that produced one.
    let mut world = World::new();
    world.insert_resource(byroredux_core::ecs::ImageHealth {
        last_non_finite_rgb: 0,
        last_non_finite_alpha: 0,
        total_non_finite_rgb: 91,
        total_non_finite_alpha: 0,
    });
    let text = RenderHealthCommand.execute(&world, "").lines.join("\n");
    assert!(text.contains("NON-FINITE PIXELS DETECTED"), "{text}");
    assert!(text.contains("rgb=91"), "{text}");
}

#[test]
fn r_health_without_the_resource_says_so_rather_than_panicking() {
    let world = World::new();
    let text = RenderHealthCommand.execute(&world, "").lines.join("\n");
    assert!(text.contains("not present"), "{text}");
}

// ── `rt.integrity` — RT publication / TLAS / cluster oracle ────────

#[test]
fn rt_integrity_prints_the_shared_machine_line() {
    let mut world = World::new();
    world.insert_resource(byroredux_core::ecs::RtIntegrityStats {
        frame: 9,
        sampled: true,
        rt_supported: true,
        rt_flag: true,
        tlas_build_succeeded: true,
        tlas_eligible: 12,
        tlas_emitted: 12,
        cluster_sampled: true,
        cluster_max_lights: 37,
        ..Default::default()
    });
    let text = RtIntegrityCommand.execute(&world, "").lines.join("\n");
    assert!(text.starts_with("rt-integrity: frame=9"), "{text}");
    assert!(text.contains("tlas_eligible=12 tlas_emitted=12"), "{text}");
    assert!(text.contains("cluster_max=37 verdict=PASS"), "{text}");
}

#[test]
fn rt_integrity_without_resource_says_so() {
    let world = World::new();
    let text = RtIntegrityCommand.execute(&world, "").lines.join("\n");
    assert!(text.contains("not present"), "{text}");
}

/// Regression for #2876 (and the #518 defect class it repeats). The spawn
/// collider census was public, re-exported, and reachable from exactly one
/// boot-time call site inside `setup_scene`'s door-teleport branch — so the
/// "why is there no floor here" diagnostic could not be run at the moment
/// the operator actually needs it, which is after falling through a floor
/// somewhere else in the cell. `PhysicsWorld`'s entire query surface had zero
/// console exposure alongside it.
///
/// Pin both commands into the live registry: a diagnostic with no console
/// entry point is a defect in this repo, per #518's closure.
#[test]
fn physics_query_surface_is_reachable_from_the_console() {
    let registry = build_command_registry();
    let names: Vec<&str> = registry.list().into_iter().map(|(name, _)| name).collect();
    for expected in ["phys.census", "phys.stats"] {
        assert!(
            names.contains(&expected),
            "`{expected}` must be registered — PhysicsWorld's query surface is \
             unreachable from byro-dbg without it (#2876/#518). Registered: {names:?}"
        );
    }
}

/// The census must degrade to a clear message rather than panicking when the
/// world carries no `PhysicsWorld` — the console runs against whatever state
/// the engine is in, including before the first cell load.
#[test]
fn phys_commands_report_a_missing_physics_world_instead_of_panicking() {
    let world = World::new();
    let census = PhysCensusCommand.execute(&world, "").lines.join("\n");
    assert!(census.contains("no PhysicsWorld"), "got: {census}");
    let stats = PhysStatsCommand.execute(&world, "").lines.join("\n");
    assert!(stats.contains("no PhysicsWorld"), "got: {stats}");
}

/// `phys.stats` must surface the quiesced state explicitly. `awake_dynamic
/// == 0 && !pending_wake` is exactly the static-scene fast path's condition,
/// so an operator reading "nothing is moving" needs to know whether `step`
/// ran at all before concluding anything about physics.
#[test]
fn phys_stats_names_the_quiesced_fast_path_on_an_empty_world() {
    let mut world = World::new();
    world.insert_resource(byroredux_physics::PhysicsWorld::new());
    // A fresh PhysicsWorld arms `pending_wake`; drain it so the world is
    // genuinely quiesced.
    world
        .resource_mut::<byroredux_physics::PhysicsWorld>()
        .step(byroredux_physics::PHYSICS_DT);

    let output = PhysStatsCommand.execute(&world, "").lines.join("\n");
    assert!(output.contains("bodies=0"), "got: {output}");
    assert!(output.contains("quiesced"), "got: {output}");
    assert!(
        output.contains("static colliders: NONE"),
        "an empty world has no fixed geometry, and the report must say so \
         rather than printing empty bounds; got: {output}"
    );
}

/// Argument handling: a bare `phys.census` needs a reference point, and a
/// malformed coordinate must be rejected rather than silently censusing 0,0.
#[test]
fn phys_census_rejects_malformed_arguments() {
    let mut world = World::new();
    world.insert_resource(byroredux_physics::PhysicsWorld::new());

    let no_origin = PhysCensusCommand.execute(&world, "").lines.join("\n");
    assert!(
        no_origin.contains("no player or active camera"),
        "got: {no_origin}"
    );

    let bad = PhysCensusCommand
        .execute(&world, "banana 0")
        .lines
        .join("\n");
    assert!(bad.contains("must be a finite number"), "got: {bad}");

    let lone = PhysCensusCommand.execute(&world, "12").lines.join("\n");
    assert!(lone.contains("expected"), "got: {lone}");

    let negative = PhysCensusCommand.execute(&world, "0 0 -5").lines.join("\n");
    assert!(
        negative.contains("radius must be positive"),
        "got: {negative}"
    );
}

/// Regression for #3423. `combat.approach` aims the gameplay camera, but the
/// swing is an ordinary camera ray, so a ring candidate with walkable floor
/// and legal range can still be occluded by a bystander — on FNV
/// `GSProspectorSaloonInterior` the P2 melee gate's swing landed on
/// `gssettlercm` (entity 927) instead of the fixture's `gstrudy` (1088).
/// Candidate selection now rejects a position whose ray does not resolve to
/// the intended actor.
#[test]
fn combat_approach_line_of_sight_rejects_an_occluded_ring_candidate() {
    use byroredux_core::ecs::components::collision::{CollisionShape, RigidBodyData};

    let mut world = World::new();
    world.register::<Transform>();
    world.register::<GlobalTransform>();
    world.register::<CollisionShape>();
    world.register::<RigidBodyData>();
    world.register::<byroredux_physics::RapierHandles>();
    world.insert_resource(byroredux_physics::PhysicsWorld::new());

    let mut actor = |pos: Vec3| {
        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(pos));
        world.insert(entity, GlobalTransform::new(pos, Quat::IDENTITY, 1.0));
        world.insert(
            entity,
            CollisionShape::Cuboid {
                half_extents: Vec3::splat(16.0),
            },
        );
        world.insert(entity, RigidBodyData::STATIC);
        entity
    };

    let camera_pos = Vec3::ZERO;
    let target = actor(Vec3::new(0.0, 0.0, -300.0));
    let blocker = actor(Vec3::new(0.0, 0.0, -150.0));

    let player = world.spawn();
    world.insert(player, Transform::from_translation(camera_pos));

    byroredux_physics::physics_sync_system(&world, 0.0);

    let aim_pos = Vec3::new(0.0, 0.0, -300.0);
    assert!(
        !super::view::combat_approach_line_of_sight_reaches(
            &world, player, target, camera_pos, aim_pos
        ),
        "a bystander between the eye and the target must disqualify the candidate — \
         otherwise the swing lands on the bystander (#3423)"
    );

    // The same world, aiming at the nearer actor: nothing stands between the
    // eye and `blocker`, so that candidate is accepted. Asserting both
    // directions against one physics world keeps the gate honest — it rejects
    // occlusion specifically, rather than rejecting every candidate.
    assert!(
        super::view::combat_approach_line_of_sight_reaches(
            &world,
            player,
            blocker,
            camera_pos,
            Vec3::new(0.0, 0.0, -150.0),
        ),
        "an unobstructed ray to the intended actor must be accepted"
    );
}

/// A cell with no `PhysicsWorld` has nothing to occlude with, so the check
/// must not reject every candidate — this is the path the resource-light
/// command tests take.
#[test]
fn combat_approach_line_of_sight_is_permissive_without_physics() {
    let mut world = World::new();
    let player = world.spawn();
    let target = world.spawn();
    assert!(super::view::combat_approach_line_of_sight_reaches(
        &world,
        player,
        target,
        Vec3::ZERO,
        Vec3::new(0.0, 0.0, -100.0),
    ));
}

/// Lock-order pins for the console commands that acquire a second guard
/// underneath a first (#3648, #3650).
///
/// Both defects are invisible to `cargo test`: the guards are all reads, the
/// commands run in the exclusive `DebugDrainSystem`, and the tracker's global
/// graph is a process-wide `LazyLock` read from `BYRO_LOCK_ORDER_CHECK` once
/// at startup — so it cannot be armed for one test without arming it for the
/// whole binary. What the fixes change is which guards are *live* across a
/// call, and that is only visible in source.
mod lock_order_pin_tests {
    const ASSETS_RS: &str = include_str!("commands/assets.rs");
    const SCENE_RS: &str = include_str!("commands/scene.rs");
    const QUEST_RS: &str = include_str!("commands/quest.rs");

    /// Source of one `execute` body, located by the command's `name()`
    /// literal so a moved `impl` block still resolves.
    fn execute_body_after(src: &'static str, command_name: &str) -> &'static str {
        let needle = format!("\"{command_name}\"");
        let anchor = src
            .find(&needle)
            .unwrap_or_else(|| panic!("no command named {needle} — it moved or was renamed"));
        let rest = &src[anchor..];
        let start = rest
            .find("fn execute(")
            .unwrap_or_else(|| panic!("{needle} has no execute()"));
        let body = &rest[start..];
        let end = body.find("\n    }\n").unwrap_or(body.len());
        &body[..end]
    }

    /// #3648 — `skin.dump` must snapshot `SkinnedMesh` and drop the guard.
    /// `format_skin_dump` acquires `GlobalTransform` (plus `Name` and
    /// `StringPool`) per bone, so a live `SkinnedMesh` guard across that call
    /// is `SkinnedMesh -> GlobalTransform`, the inverse of the canonical
    /// order that `make_world_bound_propagation_system` records every frame.
    #[test]
    fn skin_dump_snapshots_the_skinned_mesh_before_formatting() {
        let body = execute_body_after(ASSETS_RS, "skin.dump");
        assert!(
            body.contains("Option<SkinnedMesh>"),
            "skin.dump must bind an owned `Option<SkinnedMesh>` snapshot, not \
             the `ComponentRef` returned by `world.get` — the guard would \
             otherwise live across `format_skin_dump`'s GlobalTransform reads \
             (#3648, the console half of #2388)",
        );
    }

    /// #3648 SIBLING — the two `Material` console commands hold their guard
    /// across `resolve_entity_name`, which reaches `Name` then `StringPool`.
    /// `studio_host::snapshot` records the opposing `StringPool -> Material`.
    #[test]
    fn material_commands_snapshot_before_resolving_names() {
        for (command, needle) in [
            ("mat.list", "Vec<(EntityId, Material)>"),
            ("mat.dump", "Option<Material>"),
        ] {
            let body = execute_body_after(SCENE_RS, command);
            assert!(
                body.contains(needle),
                "{command} must snapshot its Material into `{needle}` and drop \
                 the storage guard before calling `resolve_entity_name` \
                 (#3648 SIBLING)",
            );
        }
    }

    /// #3650 — `scene.show` must drop the `SceneRegistry` guard before
    /// reading `ScenePlayer`. `actor_quest_trigger_is_in_sequence` records
    /// `ScenePlayer -> SceneRegistry` every frame a trigger volume is
    /// entered, so the reverse closes a cycle.
    #[test]
    fn scene_show_drops_the_registry_before_reading_the_player() {
        let body = execute_body_after(QUEST_RS, "scene.show");
        let registry = body
            .find("try_resource::<SceneRegistry>()")
            .expect("scene.show must read the SceneRegistry");
        let player = body
            .find("get::<ScenePlayer>(")
            .expect("scene.show must read the ScenePlayer");
        assert!(registry < player, "scene.show reads ScenePlayer first");
        // The registry acquisition must sit inside a scope that closes before
        // the ScenePlayer read — a snapshot block, not a function-long guard.
        let between = &body[registry..player];
        assert!(
            between.contains("definition_arc") && between.contains("\n        };"),
            "scene.show must snapshot via `definition_arc` inside a block that \
             CLOSES before `world.get::<ScenePlayer>` — holding the registry \
             across that read records `SceneRegistry -> ScenePlayer`, the \
             reverse of trigger.rs's direction (#3650)",
        );
    }
}
