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
