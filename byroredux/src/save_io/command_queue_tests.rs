//! Extracted from `save_io.rs`'s inline `mod tests` (#2407 / TD1-004).
//! Production code there is ~1030 LOC; the test bulk alone pushed the
//! file past 3000. Split by topic, contents unchanged.

use super::*;
use byroredux_core::ecs::components::Transform;
use byroredux_core::form_id::FormIdPool;
use byroredux_core::math::Vec3;
use byroredux_core::string::StringPool;

/// `save` then `load` (commands) round-trip through disk: the save
/// captures the live `CurrentCellContext`, and `load` decodes it back
/// and queues a snapshot whose cell context matches. Exercises the
/// command plumbing end-to-end minus the GPU drain.
#[test]
fn save_then_load_command_queues_with_cell_context() {
    use crate::cell_loader::CurrentCellContext;

    let mut world = World::new();
    world.insert_resource(StringPool::new());
    world.insert_resource(FormIdPool::new());
    world.insert_resource(build_save_registry());
    let dir = std::env::temp_dir().join(format!("byro_m451_cmd_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    world.insert_resource(SaveState::new(dir.clone(), 4));
    world.insert_resource(PendingSaveLoadSlot::default());
    world.insert_resource(CurrentCellContext {
        cell_editor_id: "GSDocMitchellHouse".to_string(),
        esm_path: "FalloutNV.esm".to_string(),
        masters: vec![],
    });

    let e = world.spawn();
    world.insert(e, Transform::from_translation(Vec3::new(7.0, 8.0, 9.0)));

    // save → slot 0
    let out = SaveCommand.execute(&world, "0");
    assert!(
        out.lines.iter().any(|l| l.contains("saved slot 0")),
        "save output: {:?}",
        out.lines
    );

    // load → should queue a snapshot carrying the cell context
    let out = LoadCommand.execute(&world, "0");
    assert!(
        out.lines.iter().any(|l| l.contains("GSDocMitchellHouse")),
        "load output: {:?}",
        out.lines
    );
    let pending = world.resource::<PendingSaveLoadSlot>();
    let snap = pending.snapshot.as_ref().expect("snapshot queued");
    let ctx = snapshot_cell_context(snap).expect("cell context survived round-trip");
    assert_eq!(ctx.cell_editor_id, "GSDocMitchellHouse");
    assert_eq!(ctx.esm_path, "FalloutNV.esm");
    assert_eq!(pending.slot, 0, "queued slot number recorded");

    let _ = std::fs::remove_dir_all(&dir);
}

/// EX-09/17 item 4 — exterior counterpart of
/// `save_then_load_command_queues_with_cell_context`: a save taken mid-
/// exterior-streaming carries `CurrentExteriorContext` instead of
/// `CurrentCellContext`, and `LoadCommand` must accept it rather than
/// hitting the "live load needs an interior cell" rejection this test
/// would have tripped before EX-09/17 item 4 landed.
#[test]
fn save_then_load_command_queues_with_exterior_context() {
    use crate::cell_loader::CurrentExteriorContext;

    let mut world = World::new();
    world.insert_resource(StringPool::new());
    world.insert_resource(FormIdPool::new());
    world.insert_resource(build_save_registry());
    let dir = std::env::temp_dir().join(format!("byro_m451_ext_cmd_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    world.insert_resource(SaveState::new(dir.clone(), 4));
    world.insert_resource(PendingSaveLoadSlot::default());
    world.insert_resource(CurrentExteriorContext {
        worldspace_key: "tamriel".to_string(),
        esm_path: "Oblivion.esm".to_string(),
        masters: vec![],
        grid: (3, -2),
        radius_load: 2,
        radius_unload: 3,
    });

    let e = world.spawn();
    world.insert(e, Transform::from_translation(Vec3::new(7.0, 8.0, 9.0)));

    // save → slot 0
    let out = SaveCommand.execute(&world, "0");
    assert!(
        out.lines.iter().any(|l| l.contains("saved slot 0")),
        "save output: {:?}",
        out.lines
    );

    // load → should queue a snapshot carrying the exterior context, not
    // get rejected for lacking an interior `CurrentCellContext`.
    let out = LoadCommand.execute(&world, "0");
    assert!(
        out.lines.iter().any(|l| l.contains("tamriel")),
        "load output: {:?}",
        out.lines
    );
    let pending = world.resource::<PendingSaveLoadSlot>();
    let snap = pending.snapshot.as_ref().expect("snapshot queued");
    assert!(
        snapshot_cell_context(snap).is_none(),
        "an exterior save must not carry an interior cell context"
    );
    let ctx = snapshot_exterior_context(snap).expect("exterior context survived round-trip");
    assert_eq!(ctx.worldspace_key, "tamriel");
    assert_eq!(ctx.esm_path, "Oblivion.esm");
    assert_eq!(ctx.grid, (3, -2));
    assert_eq!(pending.slot, 0, "queued slot number recorded");

    let _ = std::fs::remove_dir_all(&dir);
}

/// #3280 — unlike interactive world entry, synchronous live-load cannot
/// return after only the arrival cell: the shared tail immediately builds a
/// FormId remap and applies deltas exactly once. Pin the full-radius contract
/// that makes peripheral-cell entities present for that scan.
#[test]
fn exterior_live_load_waits_for_full_radius_before_delta_overlay() {
    assert_eq!(
        exterior_reload_bootstrap_mode(),
        crate::scene::ExteriorBootstrapMode::FullRadius
    );
}

/// A save taken outside both interior and exterior modes (loose-NIF /
/// `--mesh`) carries neither context resource — `load` must reject it
/// with a clear message instead of silently queueing an undreadable
/// snapshot for `execute_pending_save_loads` to fail on later.
#[test]
fn load_command_rejects_a_save_with_no_cell_or_exterior_context() {
    let mut world = World::new();
    world.insert_resource(StringPool::new());
    world.insert_resource(FormIdPool::new());
    world.insert_resource(build_save_registry());
    let dir = std::env::temp_dir().join(format!("byro_no_ctx_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    world.insert_resource(SaveState::new(dir.clone(), 4));
    world.insert_resource(PendingSaveLoadSlot::default());

    let e = world.spawn();
    world.insert(e, Transform::from_translation(Vec3::new(1.0, 1.0, 1.0)));

    let out = SaveCommand.execute(&world, "0");
    assert!(
        out.lines.iter().any(|l| l.contains("saved slot 0")),
        "save output: {:?}",
        out.lines
    );

    let out = LoadCommand.execute(&world, "0");
    assert!(
        out.lines.iter().any(|l| l.starts_with("Error:")),
        "a loose save must be rejected, not queued: {:?}",
        out.lines
    );
    assert!(world.resource::<PendingSaveLoadSlot>().snapshot.is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

/// #1848 / SAVE-05 — two `load` commands in the same frame (before
/// `execute_pending_save_loads` drains) resolve last-writer-wins, and
/// the superseded request is *reported* rather than silently dropped.
/// Pre-fix `pending.0 = Some(snapshot)` overwrote unconditionally with
/// no signal on either the log or the command output, so an operator
/// double-typing `load` in one frame saw only a confirmation for a
/// request that never ran.
///
/// The two slots are saved from different `CurrentCellContext`s so the
/// surviving snapshot is identifiable by its cell EDID, not just by
/// the recorded slot number.
#[test]
fn second_load_before_drain_supersedes_and_reports() {
    use crate::cell_loader::CurrentCellContext;

    let mut world = World::new();
    world.insert_resource(StringPool::new());
    world.insert_resource(FormIdPool::new());
    world.insert_resource(build_save_registry());
    let dir = std::env::temp_dir().join(format!("byro_supersede_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    world.insert_resource(SaveState::new(dir.clone(), 4));
    world.insert_resource(PendingSaveLoadSlot::default());
    world.insert_resource(CurrentCellContext {
        cell_editor_id: "FirstCell".to_string(),
        esm_path: "FalloutNV.esm".to_string(),
        masters: vec![],
    });

    let e = world.spawn();
    world.insert(e, Transform::from_translation(Vec3::new(1.0, 2.0, 3.0)));
    SaveCommand.execute(&world, "0");

    // Second save from a different cell so the two snapshots differ.
    world.resource_mut::<CurrentCellContext>().cell_editor_id = "SecondCell".to_string();
    SaveCommand.execute(&world, "1");

    // Two loads, no drain in between.
    let first = LoadCommand.execute(&world, "0");
    assert!(
        !first.lines.iter().any(|l| l.contains("superseded")),
        "first load must not report a supersede: {:?}",
        first.lines
    );
    let second = LoadCommand.execute(&world, "1");
    assert!(
        second
            .lines
            .iter()
            .any(|l| l.contains("slot 0 superseded by slot 1")),
        "second load must name both requests: {:?}",
        second.lines
    );

    // Last writer won, and the slot number tracks it.
    let pending = world.resource::<PendingSaveLoadSlot>();
    assert_eq!(pending.slot, 1);
    let snap = pending.snapshot.as_ref().expect("snapshot queued");
    let ctx = snapshot_cell_context(snap).expect("cell context present");
    assert_eq!(
        ctx.cell_editor_id, "SecondCell",
        "the surviving snapshot is the second request's"
    );
    drop(pending);

    // The drain `.take()`s, so a third frame with no `load` is a no-op
    // — idempotency of the single surviving request is unchanged.
    {
        let mut slot = world.resource_mut::<PendingSaveLoadSlot>();
        assert!(slot.snapshot.take().is_some());
    }
    assert!(world.resource::<PendingSaveLoadSlot>().snapshot.is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

/// #2017 / SAVE-D4-NEW-01 — a quicksave (blank-slot `save`) whose
/// pre-save validation aborts must NOT consume a ring rotation. Pre-fix
/// `state.ring.advance()` ran before the validation gate, so a failed
/// attempt permanently burned a slot with nothing written to back it,
/// breaking "next quicksave lands one slot after the last SUCCESSFUL
/// one". Drives one aborted attempt (world carries an unresolvable
/// `FormIdComponent`, mirroring `unresolvable_form_id_is_rejected`)
/// followed by one successful attempt, and checks the ring cursor only
/// moved on the successful write.
#[test]
fn quicksave_ring_cursor_does_not_advance_on_validation_abort() {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, LocalFormId, PluginId};

    let mut world = World::new();
    world.insert_resource(StringPool::new());
    world.insert_resource(FormIdPool::new());
    world.insert_resource(build_save_registry());
    let dir = std::env::temp_dir().join(format!("byro_ring_abort_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    world.insert_resource(SaveState::new(dir.clone(), 4));

    // A stray FormId handle minted in a throwaway pool — the world's
    // own (empty) pool can't resolve it, so `validate_form_ids` fails
    // and `SaveCommand::execute` must abort without writing.
    let stray = {
        let mut tmp = FormIdPool::new();
        tmp.intern(FormIdPair {
            plugin: PluginId::from_filename("Test.esm"),
            local: LocalFormId(0x07),
        })
    };
    let bad_entity = world.spawn();
    world.insert(bad_entity, FormIdComponent(stray));

    assert_eq!(
        world.resource::<SaveState>().ring.peek(),
        0,
        "fresh ring starts at slot 0"
    );

    // Attempt 1: quicksave, world is invalid → must abort.
    let out = SaveCommand.execute(&world, "");
    assert!(
        out.lines.iter().any(|l| l.contains("ABORTED")),
        "invalid world must abort the save: {:?}",
        out.lines
    );
    assert_eq!(
        world.resource::<SaveState>().ring.peek(),
        0,
        "an aborted quicksave must NOT advance the ring cursor"
    );

    // Fix the world (drop the stray-handle entity) and retry.
    world.despawn(bad_entity);
    let out = SaveCommand.execute(&world, "");
    assert!(
        out.lines.iter().any(|l| l.contains("saved slot 0")),
        "valid world must save to the still-unconsumed slot 0: {:?}",
        out.lines
    );
    assert_eq!(
        world.resource::<SaveState>().ring.peek(),
        1,
        "a successful quicksave must advance the ring cursor exactly once"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

fn quicksave_test_world(dir: PathBuf) -> World {
    use crate::cell_loader::CurrentCellContext;
    let mut world = World::new();
    world.insert_resource(StringPool::new());
    world.insert_resource(FormIdPool::new());
    world.insert_resource(build_save_registry());
    world.insert_resource(SaveState::new(dir, 4));
    world.insert_resource(PendingSaveLoadSlot::default());
    world.insert_resource(CurrentCellContext {
        cell_editor_id: "TestCell".to_string(),
        esm_path: "Test.esm".to_string(),
        masters: vec![],
    });
    world
}

#[test]
fn quicksave_shares_the_console_save_command_output_contract() {
    let base = std::env::temp_dir().join(format!("byro_quicksave_parity_{}", std::process::id()));
    let quick_dir = base.join("quick");
    let command_dir = base.join("command");
    let _ = std::fs::remove_dir_all(&base);
    let quick = quicksave_test_world(quick_dir.clone());
    let command = quicksave_test_world(command_dir.clone());

    let quick_lines = quicksave(&quick).lines;
    let command_lines = SaveCommand.execute(&command, "").lines;
    assert_eq!(quick_lines.len(), command_lines.len());
    for (quick, command) in quick_lines.iter().zip(&command_lines) {
        assert_eq!(
            quick.replace(&quick_dir.display().to_string(), "<dir>"),
            command.replace(&command_dir.display().to_string(), "<dir>")
        );
    }
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn player_save_actions_wait_for_the_quiescent_fifo_drain() {
    let dir =
        std::env::temp_dir().join(format!("byro_deferred_player_save_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut world = quicksave_test_world(dir.clone());
    world.insert_resource(PendingPlayerSaveActions::default());

    queue_player_save_action(&world, PlayerSaveAction::Quicksave).unwrap();
    queue_player_save_action(&world, PlayerSaveAction::Quickload).unwrap();
    assert!(
        disk::list_slots(&dir).is_empty(),
        "input adapters must not execute the wide save lock surface"
    );

    let outputs = execute_pending_player_save_actions(&world);
    assert_eq!(outputs.len(), 2);
    assert_eq!(outputs[0].0, PlayerSaveAction::Quicksave);
    assert!(!command_output_is_failure(&outputs[0].1));
    assert_eq!(outputs[1].0, PlayerSaveAction::Quickload);
    assert!(!command_output_is_failure(&outputs[1].1));
    assert_eq!(disk::list_slots(&dir), vec![0]);
    assert_eq!(world.resource::<PendingSaveLoadSlot>().slot, 0);
    assert!(execute_pending_player_save_actions(&world).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn quickload_empty_errors_and_corrupt_newest_falls_back() {
    let dir = std::env::temp_dir().join(format!("byro_quickload_fallback_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let world = quicksave_test_world(dir.clone());

    let empty = quickload_latest(&world);
    assert!(command_output_is_failure(&empty));
    assert!(empty.lines.join(" ").contains("no save slots available"));

    let saved = SaveCommand.execute(&world, "0");
    assert!(!command_output_is_failure(&saved));
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(disk::slot_path(&dir, 1), b"corrupt newest save").unwrap();

    let output = quickload_latest(&world);
    assert!(!command_output_is_failure(&output));
    assert!(output
        .lines
        .iter()
        .any(|line| line.contains("skipped invalid quickload slot 1")));
    assert!(output
        .lines
        .iter()
        .any(|line| line.contains("falling back to valid slot 0")));
    assert_eq!(world.resource::<PendingSaveLoadSlot>().slot, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn startup_load_parser_queues_valid_slot_and_surfaces_invalid_value() {
    let dir = std::env::temp_dir().join(format!("byro_startup_load_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let world = quicksave_test_world(dir.clone());
    assert!(!command_output_is_failure(
        &SaveCommand.execute(&world, "3")
    ));

    let invalid = queue_startup_load(&world, "not-a-slot");
    assert!(command_output_is_failure(&invalid));
    assert!(invalid
        .lines
        .join(" ")
        .contains("--load requires a numeric"));

    let valid = queue_startup_load(&world, "3");
    assert!(!command_output_is_failure(&valid));
    assert_eq!(world.resource::<PendingSaveLoadSlot>().slot, 3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn validation_aborted_quicksave_is_classified_for_player_feedback() {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, LocalFormId, PluginId};

    let dir = std::env::temp_dir().join(format!("byro_abort_feedback_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut world = quicksave_test_world(dir.clone());
    let stray = {
        let mut pool = FormIdPool::new();
        pool.intern(FormIdPair {
            plugin: PluginId::from_filename("Missing.esm"),
            local: LocalFormId(1),
        })
    };
    let entity = world.spawn();
    world.insert(entity, FormIdComponent(stray));

    let output = quicksave(&world);
    assert!(command_output_is_failure(&output));
    assert!(
        output
            .lines
            .first()
            .is_some_and(|line| line.starts_with("save ABORTED")),
        "the exact first line routed to the HUD toast must name the abort"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
