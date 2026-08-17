use super::{
    capsule_center_y_on_surface, select_door_spawn_position, select_initial_player_mode,
    CharacterSpawnPlan, GroundProbe,
};
use crate::systems::PlayerMode;
use byroredux_core::math::Vec3;

#[test]
fn exterior_spawn_skips_persistent_door_in_neighboring_cell() {
    // Regression: interactive `(3,-19)` bootstrap has only that tile's
    // collision ready, while the persistent CELL may already have materialised
    // this row -20 door. It used to win solely because it iterated first.
    let neighboring_persistent_door = Vec3::new(14_740.0, 8_200.0, 78_988.0);
    let foreground_door = Vec3::new(14_500.0, 8_100.0, 75_700.0);

    assert_eq!(
        select_door_spawn_position(
            [neighboring_persistent_door, foreground_door],
            Some((3, -19)),
        ),
        Some(foreground_door),
    );
}

#[test]
fn exterior_spawn_returns_none_when_only_neighboring_doors_exist() {
    let neighboring_persistent_door = Vec3::new(14_740.0, 8_200.0, 78_988.0);

    assert_eq!(
        select_door_spawn_position([neighboring_persistent_door], Some((3, -19))),
        None,
        "the caller must ground against the foreground terrain instead",
    );
}

#[test]
fn interior_spawn_preserves_first_door_behavior() {
    let first = Vec3::new(10.0, 20.0, 30.0);
    let second = Vec3::new(40.0, 50.0, 60.0);

    assert_eq!(
        select_door_spawn_position([first, second], None),
        Some(first)
    );
}

#[test]
fn capsule_spawn_clears_surface_by_full_shape_and_kcc_offset() {
    // Oblivion's default body is capsule_y(46, 18) with a 4 BU KCC offset.
    // Its centre must therefore be 68 BU above the probed floor, not 50 BU.
    assert_eq!(capsule_center_y_on_surface(350.8, 46.0, 18.0, 4.0), 418.8,);
}

#[test]
fn capsule_spawn_height_includes_radius() {
    let without_radius = capsule_center_y_on_surface(10.0, 46.0, 0.0, 4.0);
    let with_radius = capsule_center_y_on_surface(10.0, 46.0, 18.0, 4.0);

    assert_eq!(with_radius - without_radius, 18.0);
}

#[test]
fn empty_exterior_defaults_to_flycam_even_when_peripheral_content_loaded() {
    assert_eq!(
        select_initial_player_mode(false, false, false, true, false, true),
        PlayerMode::FlyCam,
    );
}

#[test]
fn explicit_player_overrides_empty_exterior_guard() {
    assert_eq!(
        select_initial_player_mode(false, true, false, true, false, true),
        PlayerMode::Character,
    );
}

#[test]
fn explicit_fly_still_wins_over_explicit_player() {
    assert_eq!(
        select_initial_player_mode(true, true, false, true, true, true),
        PlayerMode::FlyCam,
    );
}

#[test]
fn content_backed_foreground_defaults_to_character() {
    assert_eq!(
        select_initial_player_mode(false, false, false, true, true, true),
        PlayerMode::Character,
    );
}

// ── EX-04 ground-probe gate (#2375) ──────────────────────────────

#[test]
fn content_backed_foreground_without_walkable_ground_falls_back_to_flycam() {
    // The defect this issue names: a cell can be fully content-backed and
    // still have nothing under the spawn column (FO3 MegatonWorld 0,0). The
    // capsule used to be placed at aabb.max.y + 200 and fall indefinitely.
    assert_eq!(
        select_initial_player_mode(false, false, false, true, true, false),
        PlayerMode::FlyCam,
    );
}

#[test]
fn explicit_player_overrides_the_ground_probe_too() {
    // Acceptance is explicit that --player may override "with a warning".
    assert_eq!(
        select_initial_player_mode(false, true, false, true, true, false),
        PlayerMode::Character,
    );
}

#[test]
fn character_needs_content_foreground_and_ground_together() {
    // All three must hold; any one failing is FlyCam. Pinning the full truth
    // table stops a later refactor from dropping a term silently.
    for (content, foreground, ground) in [
        (true, true, true),
        (true, true, false),
        (true, false, true),
        (false, true, true),
        (false, false, false),
    ] {
        let expected = if content && foreground && ground {
            PlayerMode::Character
        } else {
            PlayerMode::FlyCam
        };
        assert_eq!(
            select_initial_player_mode(false, false, false, content, foreground, ground),
            expected,
            "content={content} foreground={foreground} ground={ground}"
        );
    }
}

// ── GroundProbe (#2375) ──────────────────────────────────────────

#[test]
fn only_a_grounded_probe_is_walkable() {
    assert!(GroundProbe::Grounded {
        x: 1.0,
        z: 2.0,
        surface_y: 10.0,
        spawn_y: 78.0,
        collider_count: 416,
    }
    .is_walkable());
    assert!(!GroundProbe::NoFloorBeneath {
        x: 1.0,
        z: 2.0,
        searched_bu: 5000.0,
        collider_count: 416,
    }
    .is_walkable());
    assert!(!GroundProbe::NoColliders.is_walkable());
}

#[test]
fn probe_telemetry_is_greppable_and_names_the_collider_count() {
    // The smoke matrix greps this line, so its shape is a contract.
    let grounded = GroundProbe::Grounded {
        x: 10.0,
        z: 20.0,
        surface_y: -1234.5,
        spawn_y: -1150.0,
        collider_count: 416,
    };
    let line = grounded.telemetry_line();
    assert!(line.starts_with("spawn-probe: "), "{line}");
    assert!(line.contains("result=grounded"), "{line}");
    assert!(line.contains("colliders=416"), "{line}");

    let missing = GroundProbe::NoFloorBeneath {
        x: 10.0,
        z: 20.0,
        searched_bu: 5000.0,
        collider_count: 19,
    };
    assert!(missing.telemetry_line().contains("result=no-floor"));
    assert!(missing.telemetry_line().contains("colliders=19"));

    // The two failure modes must stay distinguishable: "no colliders at all"
    // is a different diagnosis from "colliders exist, none under the spawn".
    let none = GroundProbe::NoColliders;
    assert!(none.telemetry_line().contains("result=no-colliders"));
    assert_eq!(none.collider_count(), 0);
}

#[test]
fn spawn_plan_reuses_the_probed_column_and_controller() {
    let controller = byroredux_physics::CharacterController::HUMAN;
    let body_pos = Vec3::new(11.0, 78.0, 22.0);
    let plan = CharacterSpawnPlan::new(
        body_pos,
        controller,
        GroundProbe::Grounded {
            x: body_pos.x,
            z: body_pos.z,
            surface_y: 10.0,
            spawn_y: body_pos.y,
            collider_count: 4,
        },
    );

    assert_eq!(plan.body_pos, body_pos);
    assert_eq!(plan.controller.half_height, controller.half_height);
    assert_eq!(plan.controller.radius, controller.radius);
    assert_eq!(plan.controller.eye_height, controller.eye_height);
    match plan.ground_probe {
        GroundProbe::Grounded { x, z, .. } => {
            assert_eq!((x, z), (plan.body_pos.x, plan.body_pos.z));
        }
        other => panic!("expected grounded spawn plan, got {other:?}"),
    }
}
