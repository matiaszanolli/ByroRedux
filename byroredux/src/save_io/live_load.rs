//! Live-load drain: reload the saved session, restore resources,
//! overlay deltas (#5585, split from `save_io.rs`).
//!
//! [`execute_pending_save_loads`] runs between frames with
//! `&mut World + &mut VulkanContext` and is the whole load side of the
//! M45.1 contract; `PRE_RELOAD_RESOURCES` lives here beside it so the
//! scan-from-fn-to-EOF pinning tests keep their window on one file.

use super::*;

/// Registered resource names safe to restore from a snapshot **before**
/// the cell/exterior reload that follows in [`execute_pending_save_loads`],
/// as opposed to only after it (the normal, safe default).
///
/// A resource belongs here only if some spawn-time code path consults it
/// while the reload is actively building the new cell/exterior — today
/// that's `ReferenceEnableState` (`cell_loader::spawn::placement_is_disabled`,
/// #3789/#3278) and `ReferenceLockState`
/// (`cell_loader::spawn::scripted_lock_override`, #4136) — both are
/// FormID-keyed ledgers the spawn path reads while the reload is
/// actively building the new cell, so a post-reload restore would
/// arrive after the placements they govern were already stamped. The same holds
/// for `ScriptedFactionChanges` (#5919): `stamp_quest_reference` replays it onto
/// each NPC as the reload re-spawns the cell, and nothing re-applies it to
/// actors already stamped (`FactionRanks` is re-derived, not saved), so a ledger
/// installed afterwards would leave every resident NPC on its record factions.
/// Everything else — most importantly `ItemInstancePool` —
/// must wait for the post-reload wholesale [`byroredux_save::restore_resources`]
/// call, because the reload's own teardown (`unload_cell` →
/// `release_victim_item_instances`) still needs the **live** value of
/// those resources to do its job. Restoring one of them here swaps in the
/// saved value first, so the teardown mutates the save being loaded
/// instead of the live session — see #4135, which is exactly this
/// mistake for `ItemInstancePool`, and is the reason this is an explicit
/// allowlist (mirroring [`MUTABLE_DELTA_COLUMNS`]'s pattern) rather than
/// "everything except a denylist".
pub(super) const PRE_RELOAD_RESOURCES: &[&str] = &[
    "ReferenceEnableState",
    "ReferenceLockState",
    "ReferenceScriptState",
    "ScriptedFactionChanges",
    // #6237 — `cell_loader::spawn::scripted_motion_type` reads it as each
    // collider is built.
    "ReferenceMotionTypeState",
];

/// The live values of the [`PRE_RELOAD_RESOURCES`] ledgers, taken before the
/// pre-reload restore overwrites them with the save's (#6081).
///
/// The two `reload_*_session` helpers abort on a failed non-destructive
/// preflight (ESM moved, cell renamed, mod set changed) with "keeping the
/// current session"; by then the restore had already installed the *other
/// timeline's* ledgers, and the live session's own `Disable()` / lock /
/// once-only / faction entries were gone for good, governing the next cell
/// respawn and persisted by the next quicksave. An aborted load reinstates
/// these instead. One field per `PRE_RELOAD_RESOURCES` entry (a source
/// guard in `live_reload_tests.rs` keeps the two in step).
pub(super) struct LiveReloadLedgers {
    enable: Option<byroredux_scripting::ReferenceEnableState>,
    lock: Option<byroredux_scripting::ReferenceLockState>,
    script: Option<byroredux_scripting::ReferenceScriptState>,
    factions: Option<byroredux_scripting::ScriptedFactionChanges>,
    motion: Option<byroredux_scripting::ReferenceMotionTypeState>,
}

impl LiveReloadLedgers {
    pub(super) fn capture(world: &World) -> Self {
        fn live<R: byroredux_core::ecs::Resource + Clone>(world: &World) -> Option<R> {
            world.try_resource::<R>().map(|guard| (*guard).clone())
        }
        Self {
            enable: live(world),
            lock: live(world),
            script: live(world),
            factions: live(world),
            motion: live(world),
        }
    }

    /// Put the captured values back. A ledger that was absent from the live
    /// world is removed again rather than left holding the save's value.
    pub(super) fn reinstate(self, world: &mut World) {
        fn put<R: byroredux_core::ecs::Resource>(world: &mut World, value: Option<R>) {
            match value {
                Some(value) => {
                    world.insert_resource(value);
                }
                None => {
                    world.remove_resource::<R>();
                }
            }
        }
        put(world, self.enable);
        put(world, self.lock);
        put(world, self.script);
        put(world, self.factions);
        put(world, self.motion);
    }
}

fn notify_player(world: &World, message: impl Into<String>) {
    crate::notifications::push(world, message);
}

/// What a `reload_*_session` helper reports back to
/// [`execute_pending_save_loads`]'s shared tail (delta restore/apply,
/// validation, pose restore) once the world is repopulated.
struct ReloadOutcome {
    /// Human-readable location, for the shared tail's log lines —
    /// `"cell 'FooInterior01'"` or `"worldspace 'tamriel' @ (3,-2)"`.
    location_label: String,
    /// What got (re)populated — `"42 entities"` for an interior cell,
    /// `"9 cells streaming"` for an exterior worldspace. Deliberately not a
    /// single `entity_count: usize`: exterior streaming doesn't track a
    /// per-cell entity count the way `load_cell_with_masters`'s
    /// `CellLoadResult` does, and counting live entities under every
    /// just-loaded `CellRoot` post-hoc for a log line isn't worth the
    /// query — "N cells" is the honest number this path actually has.
    count_label: String,
}

/// Reload the saved interior cell (SAVE-D6-02 preflight → teardown →
/// `load_cell_with_masters`). Returns `None` (having already logged why)
/// on any failure — the caller's job is just to bail without running the
/// shared restore/apply tail.
fn reload_interior_session(
    world: &mut World,
    ctx: &mut byroredux_renderer::VulkanContext,
    streaming: &mut Option<crate::streaming::WorldStreamingState>,
    args: &[String],
    cell_ctx: &crate::cell_loader::CurrentCellContext,
) -> Option<ReloadOutcome> {
    // SAVE-D6-02 — pre-flight the reload BEFORE the destructive teardown.
    // `unload_current_interior` + `drain_streaming_state` are irreversible;
    // if the reload then fails (missing/corrupt ESM, renamed/absent cell
    // editor id) the old `Err => return` left the engine in an empty world
    // with the player stranded in the void. Both of those failure modes
    // surface in `validate_cell_loadable` (parse + cell lookup, the same
    // non-destructive prefix `load_cell_with_masters` runs first), so we can
    // catch them here and KEEP the current cell instead. The on-disk save is
    // untouched either way; this just preserves the live session.
    if let Err(e) = crate::cell_loader::validate_cell_loadable(
        &cell_ctx.masters,
        &cell_ctx.esm_path,
        &cell_ctx.cell_editor_id,
    ) {
        let message = format!(
            "save load ABORTED — cannot reload cell '{}'; keeping the current cell (on-disk save intact): {e:#}",
            cell_ctx.cell_editor_id
        );
        log::error!("{message}");
        notify_player(world, message);
        return None;
    }

    // #5056 — the preflight passed, so this session is genuinely being
    // replaced: drop the live cinematic/tether rows BEFORE the teardown
    // so the convoy (cart, horse, riders) despawns with the cell instead
    // of being retained as ghosts that duplicate the reload's fresh
    // REFR/ACHR spawns and keep the player glued to a dead vehicle. Must
    // stay after the preflight: an aborted load keeps the live session,
    // cinematic state included.
    crate::cell_loader::purge_cinematic_retention_state(world, ctx);

    // Tear down whatever's loaded, then reload the saved cell fresh.
    if streaming.is_some() {
        crate::streaming_helpers::drain_streaming_state(world, ctx, streaming);
    }
    crate::cell_loader::unload_current_interior(world, ctx);

    let tex_provider = crate::asset_provider::build_texture_provider(args);
    let mut mat_provider = crate::asset_provider::build_material_provider(args);
    let result = crate::cell_loader::load_cell_with_masters(
        &cell_ctx.masters,
        &cell_ctx.esm_path,
        &cell_ctx.cell_editor_id,
        world,
        ctx,
        &tex_provider,
        Some(&mut mat_provider),
    );
    match result {
        Ok(r) => {
            // Always called (not gated on `Some`) so a cell with no
            // `XCLL`/resolvable `LTMP` still gets the engine-default
            // interior fallback rather than a stale carry-over from
            // whatever cell was loaded before the load-apply (FNV-D1-01).
            crate::cell_loader::apply_interior_cell_lighting(
                world,
                r.lighting.as_ref(),
                r.show_sky,
                r.image_space,
            );
            ctx.signal_temporal_discontinuity(
                crate::streaming_helpers::SVGF_TAA_STREAMING_RECOVERY_FRAMES,
            );
            world.insert_resource(crate::cell_loader::LoadedPluginSet {
                masters: cell_ctx.masters.clone(),
                esm_path: cell_ctx.esm_path.clone(),
            });
            Some(ReloadOutcome {
                location_label: format!("cell '{}'", cell_ctx.cell_editor_id),
                count_label: format!("{} entities", r.entity_count),
            })
        }
        Err(e) => {
            let message = format!(
                "save load: failed to reload cell '{}': {e:#}",
                cell_ctx.cell_editor_id
            );
            log::error!("{message}");
            notify_player(world, message);
            None
        }
    }
}

/// Reload the saved exterior worldspace/grid (EX-09/17 item 4). Same
/// preflight-before-teardown posture as [`reload_interior_session`]:
/// `build_exterior_world_context` is the exterior equivalent of
/// `validate_cell_loadable` (it's the same ESM-parse-and-resolve work,
/// just with no separate validate-only variant), so it runs first and the
/// already-built context is threaded straight into
/// `scene::assemble_exterior_streaming` on success rather than paying a
/// second parse the way the interior path's separate validate+load calls
/// do.
///
/// In-flight-streaming-worker handling (the design decision the plan
/// flagged): whatever the current session has in flight is unconditionally
/// drained/cancelled, not waited-on or resumed — the same posture
/// `drain_streaming_state` already uses at every other exterior teardown
/// boundary (cell transitions, this same function's interior branch). A
/// discarded in-flight cell payload just means that cell isn't in `World`
/// yet; the fresh `WorldStreamingState` rebuilt below re-requests it from
/// scratch around the saved grid, so nothing is lost, only re-fetched.
/// Whether it's safe to run the saved-delta overlay (`build_form_id_remap`
/// and `apply_deltas` in [`execute_pending_save_loads`]) after a `FullRadius`
/// exterior bootstrap — #3499. `exterior_reload_bootstrap_mode()`'s whole
/// point is guaranteeing every saved cell is resident first, via
/// `bootstrap_waiting`'s `!pending.is_empty()` loop condition; but that
/// loop's one non-`pending`-driven exit (the streaming worker thread dying
/// mid-bootstrap) can return with cells still pending. Applying the overlay
/// in that state would silently drop every saved row belonging to a cell
/// that never arrived.
fn exterior_reload_overlay_is_safe(pending_after_bootstrap: usize) -> bool {
    pending_after_bootstrap == 0
}

fn reload_exterior_session(
    world: &mut World,
    ctx: &mut byroredux_renderer::VulkanContext,
    streaming: &mut Option<crate::streaming::WorldStreamingState>,
    args: &[String],
    ext_ctx: &crate::cell_loader::CurrentExteriorContext,
) -> Option<ReloadOutcome> {
    let wctx = match crate::cell_loader::build_exterior_world_context(
        &ext_ctx.masters,
        &ext_ctx.esm_path,
        ext_ctx.grid.0,
        ext_ctx.grid.1,
        ext_ctx.radius_load,
        Some(&ext_ctx.worldspace_key),
    ) {
        Ok(wctx) => wctx,
        Err(e) => {
            let message = format!(
                "save load ABORTED — cannot rebuild worldspace '{}'; keeping the current session (on-disk save intact): {e:#}",
                ext_ctx.worldspace_key
            );
            log::error!("{message}");
            notify_player(world, message);
            return None;
        }
    };

    // EX-14/15 item C2 (#2369) — deliberately NOT wired in here, despite
    // `docs/engine/stream-boundary-state-continuity.md` §6 naming this
    // reload path as one of three candidate sites. Skipping the
    // persistent-CELL rebuild would mean skipping its ONLY restore path
    // too: a save load's correctness model is "always rebuild fresh from
    // ESM, then restore" — the full per-component registry round-trip for
    // most components, plus `MUTABLE_DELTA_COLUMNS`'s targeted overlay
    // (`Transform`, `WanderState`, `TravelState`, `Traveled`, etc., all
    // FormID-keyed) for the rest. A preserved LIVE root would never pass
    // through either restore step, so its entities would silently keep
    // whatever state the CURRENT session left them in instead of the
    // state actually recorded in the save file being loaded — a real
    // save-fidelity regression, not an optimization, the first time this
    // session's persistent-cell state has drifted from the loaded save's
    // (any save older than "the same instant", i.e. every real load).
    // Item C2's identity-skip is scoped to the genuine still-in-session,
    // still-live worldspace crossing (`step_cell_transition`'s Exterior
    // arm) where there is no save file whose recorded state must win.

    // #5056 — same pre-teardown purge as the interior reload above, after
    // this path's own preflight (`build_exterior_world_context`) passed.
    crate::cell_loader::purge_cinematic_retention_state(world, ctx);

    // Tear down whatever's loaded, then rebuild the saved worldspace fresh.
    if streaming.is_some() {
        crate::streaming_helpers::drain_streaming_state(world, ctx, streaming);
    }
    crate::cell_loader::unload_current_interior(world, ctx);

    let tex_provider = crate::asset_provider::build_texture_provider(args);
    let mat_provider = crate::asset_provider::build_material_provider(args);
    let (state, _cam_center) = crate::scene::assemble_exterior_streaming(
        world,
        ctx,
        wctx,
        tex_provider,
        mat_provider,
        ext_ctx.grid,
        ext_ctx.radius_load,
        exterior_reload_bootstrap_mode(),
        // See the comment above `wctx`'s teardown for why item C2's
        // identity-skip is deliberately not used on the save-load path.
        None,
    );
    let location_label = format!(
        "worldspace '{}' @ ({},{})",
        ext_ctx.worldspace_key, ext_ctx.grid.0, ext_ctx.grid.1
    );
    let count_label = format!(
        "{} cells streaming ({} pending)",
        state.loaded.len(),
        state.pending.len()
    );
    world.insert_resource(crate::cell_loader::LoadedPluginSet {
        masters: ext_ctx.masters.clone(),
        esm_path: ext_ctx.esm_path.clone(),
    });
    // Re-stamp the identity mirror — `assemble_exterior_streaming` doesn't
    // (only `begin_exterior_streaming` does, and this path can't reuse that
    // without paying the second ESM parse it exists to avoid), and
    // `drain_streaming_state` above just cleared the stale one.
    world.insert_resource(crate::cell_loader::CurrentExteriorContext {
        worldspace_key: ext_ctx.worldspace_key.clone(),
        esm_path: ext_ctx.esm_path.clone(),
        masters: ext_ctx.masters.clone(),
        grid: ext_ctx.grid,
        radius_load: state.radius_load,
        radius_unload: state.radius_unload,
    });
    // #3499 — `exterior_reload_bootstrap_mode()` is `FullRadius` specifically
    // so this function's caller can safely run `build_form_id_remap` +
    // `apply_deltas` against a fully-resident world: `bootstrap_waiting`
    // loops until `state.pending` drains. But `stream_initial_radius`'s wait
    // loop has one exit that isn't `pending`-driven — the streaming worker
    // thread disconnecting mid-bootstrap — and on that path `pending` is
    // still non-empty here. Applying the delta overlay anyway would
    // silently drop every saved row belonging to a cell that never
    // arrived: the exact #3280 mechanism, on a narrower trigger. The fresh
    // ESM-only world is committed either way (nothing left to roll back —
    // the old session was already drained above), so abort only the
    // overlay: return `None` so the caller's `let Some(ReloadOutcome {..})
    // = outcome else { return; }` skips `build_form_id_remap`/`apply_deltas`
    // and everything after, the same posture `validate_snapshot_types` /
    // `validate_cell_loadable` already take for their own failure modes.
    let pending_after_bootstrap = state.pending.len();
    *streaming = Some(state);
    ctx.signal_temporal_discontinuity(crate::streaming_helpers::SVGF_TAA_STREAMING_RECOVERY_FRAMES);
    if !exterior_reload_overlay_is_safe(pending_after_bootstrap) {
        let message = format!(
            "save load: worldspace '{}' reloaded from ESM, but the streaming worker \
             disconnected mid-bootstrap with {pending_after_bootstrap} cell(s) still pending — \
             skipping the saved-delta overlay rather than silently dropping rows for cells that \
             never arrived. The session is now on fresh ESM state for this worldspace with NO \
             save-state overlay applied; reload the save again once the streaming worker issue \
             is resolved.",
            ext_ctx.worldspace_key
        );
        log::error!("{message}");
        notify_player(world, message);
        return None;
    }
    Some(ReloadOutcome {
        location_label,
        count_label,
    })
}

/// Live-load delta application is synchronous and runs immediately after the
/// exterior reload returns. The full radius must therefore be resident before
/// `build_form_id_remap` scans the world; foreground-first would permanently
/// drop saved rows belonging to still-pending peripheral cells (#3280).
pub(super) fn exterior_reload_bootstrap_mode() -> crate::scene::ExteriorBootstrapMode {
    crate::scene::ExteriorBootstrapMode::FullRadius
}

/// #5034 / #5255 — drop the player's mid-life gear handoff scratch
/// (both the equip-import queue #5034 and the release queue #5028)
/// before a reload's reconcile reads the restored slots.
///
/// Neither component is saved (runtime scratch), but both survive the
/// world replacement on the process-lifetime player body. Left alone:
/// a stale import would attach the pre-load session's gear after the
/// slots it responded to are gone, and — because the gear loader's
/// drain runs *after* `step_save_loads` in the frame — a release queued
/// before the load would despawn a gear root `reconcile_worn_gear` just
/// revealed for the loaded slots, with nothing re-importing it. The
/// restored `Inventory`/`EquipmentSlots` are the authority; the next
/// genuine equip/leaves transfer re-queues whichever half applies.
pub(super) fn clear_player_gear_handoff_scratch(
    world: &mut World,
    player: byroredux_core::ecs::EntityId,
) {
    world.remove::<crate::npc_spawn::PendingGearImport>(player);
    world.remove::<crate::npc_spawn::PendingGearRelease>(player);
}

/// #5435 — forget the outgoing session's CLOC location. The cursor is
/// session-local and unsaved; left alone across an in-process load it made
/// the loaded game's first CLOC carry the PRE-load session's LCTN as `L1`
/// (a different cell), or fire nothing at all (same cell), while a
/// fresh-process load fired with `L1 = None`. Resetting it makes every load
/// behave like a fresh boot: the first post-load frame raises one CLOC
/// whose `L1` is empty and `L2` is the loaded location — arriving in a
/// location by loading is a location change, and the loaded game was never
/// anywhere before it.
pub(super) fn reset_story_location_cursor(world: &World) {
    if let Some(mut cursor) =
        world.try_resource_mut::<byroredux_scripting::StoryLocationCursor>()
    {
        *cursor = byroredux_scripting::StoryLocationCursor::default();
    }
}

/// #5435 — discard Story Manager markers still pending from the outgoing
/// session. `StoryEvent` is a Pattern-B marker drained by the Update-stage
/// dispatcher, but Late-stage producers (`AHEL` from the dialogue selection,
/// drowning `KILL` from `water_damage_system`) stamp it after that drain, so
/// one can straddle the frame boundary this drain runs on. It carries
/// session-local `EntityId`s that the reload has just invalidated (and may
/// have reused for unrelated entities); dispatching it would fire a story
/// event about the previous session's actors in the loaded one.
pub(super) fn discard_pending_story_events(world: &World) {
    let Some(mut events) = world.query_mut::<byroredux_scripting::story_manager::StoryEvent>()
    else {
        return;
    };
    let stale: Vec<_> = events.iter().map(|(entity, _)| entity).collect();
    for entity in stale {
        events.remove(entity);
    }
}

/// #6239 — discard the value-keyed requests the outgoing session's scripts
/// queued. They hold FormIDs, amounts and names rather than entities, so
/// nothing stops them resolving in the loaded world, and the drain runs
/// between their producer and consumer more than once: a fragment's player
/// `MoveTo` (`PlayerMoveRequest`) is taken by `step_cell_transition` after
/// this drain in the same `about_to_wait`, and XP / skill grants raised by
/// the Late fragment runners registered after `scripted_experience_system` /
/// `scripted_skill_system` wait a frame. Applied in the loaded game they
/// would move the player off the restored pose or grant unearned progress,
/// and the next save would keep it. The rest are the same shape: sleep hours,
/// music, script messages and the message boxes they opened.
pub(super) fn discard_outgoing_session_requests(world: &World) {
    if let Some(mut request) = world.try_resource_mut::<byroredux_scripting::PlayerMoveRequest>() {
        request.0 = None;
    }
    if let Some(mut grants) = world.try_resource_mut::<byroredux_scripting::story_manager::PendingPlayerXp>() {
        grants.0.clear();
    }
    if let Some(mut advances) = world.try_resource_mut::<byroredux_scripting::PendingSkillAdvances>() {
        advances.0.clear();
    }
    if let Some(mut sleep) = world.try_resource_mut::<byroredux_scripting::obscript_more::PendingSleep>() {
        sleep.0 = 0.0;
    }
    if let Some(mut music) = world.try_resource_mut::<byroredux_scripting::obscript_more::PendingMusic>() {
        music.0.clear();
    }
    if let Some(mut messages) = world.try_resource_mut::<byroredux_scripting::PendingScriptMessages>() {
        messages.0.clear();
    }
    if let Some(mut boxes) = world.try_resource_mut::<crate::notifications::MessageBoxQueue>() {
        boxes.0.clear();
    }
}

/// Drain a queued live-load: reload the saved cell or exterior worldspace
/// via the existing loaders (full GPU/physics/camera setup), restore saved
/// resources, then overlay the form-id-keyed mutable component deltas.
///
/// Runs once per frame after `step_debug_loads`. No-op when nothing is
/// queued. Mirrors [`crate::debug_load::execute_pending_debug_loads`]'s
/// synchronous loader-in-drain shape.
pub fn execute_pending_save_loads(
    world: &mut World,
    ctx: &mut byroredux_renderer::VulkanContext,
    streaming: &mut Option<crate::streaming::WorldStreamingState>,
) {
    let (snapshot, loaded_slot) = {
        let Some(mut slot) = world.try_resource_mut::<PendingSaveLoadSlot>() else {
            return;
        };
        match slot.snapshot.take() {
            Some(snapshot) => (snapshot, slot.slot),
            None => return,
        }
    };

    let registry = build_save_registry();

    // #3163 — every typed column is decodable from the snapshot alone.
    // Reject before the irreversible cell/streaming teardown so no serde
    // failure can leave a half-overlaid world.
    if let Err(e) = byroredux_save::validate_snapshot_types(&registry, &snapshot) {
        let message = format!(
            "save load ABORTED — snapshot columns failed typed preflight; keeping the current session: {e}"
        );
        log::error!("{message}");
        notify_player(world, message);
        return;
    }
    if let Err(error) = crate::extensions::preflight_extension_state(world, &snapshot) {
        let message = format!(
            "save load ABORTED — extension state failed preflight; keeping the current session: \
             {error:#}"
        );
        log::error!("{message}");
        notify_player(world, message);
        return;
    }

    // Build asset providers from the boot CLI args (same BSAs the engine
    // is running with) — matches the cell-transition path. #2039 /
    // PERF-D7-02: this rebuild discards the same warm BGSM/BGEM/CSG
    // caches `step_cell_transition`'s identical rebuild does — see the
    // caching design note on `App::step_cell_transition` in
    // `app_step.rs` for the shape a shared cache should take.
    let args = crate::cli_args::effective_args();

    // #3789 — restore saved resources BEFORE the reload, not only after it.
    //
    // `ReferenceEnableState` is the FormID-keyed ledger a Papyrus `Disable()`
    // writes to, and since #3278 it has a *spawn-time* consumer:
    // `cell_loader::spawn::placement_is_disabled` consults it per placed
    // REFR, before any mesh, collider or light. The reload below therefore
    // takes every spawn decision against whatever ledger the live session
    // happens to hold, and the saved one used to arrive seventeen lines
    // later — too late to matter, because `apply_deltas` is additive-only by
    // contract and can neither spawn nor despawn.
    //
    // On the most common load in the game — start the engine, load a save —
    // the live ledger is `ReferenceEnableState::default()`, i.e. everything
    // enabled, so every reference the save recorded as disabled came back
    // solid and interactive. Nothing logged it: from the loader's point of
    // view it correctly honoured the ledger it was shown. The symmetric
    // same-session case is a reference disabled *after* the save spawning
    // content-less for the whole of that cell's residency.
    //
    // Restoring here is also the right place for the preflight ordering the
    // block above establishes: a resource-restore failure now aborts BEFORE
    // the irreversible cell/streaming teardown rather than after it, so a
    // bad snapshot can no longer leave a torn-down world behind.
    //
    // #4135 — this MUST be `restore_resources_subset(..., PRE_RELOAD_RESOURCES)`,
    // never the wholesale `restore_resources`. The reload below tears down
    // the current cell/exterior via `unload_cell`, which calls
    // `release_victim_item_instances` against whichever `ItemInstancePool`
    // is installed, to free the *live* session's item-instance slots. A
    // wholesale restore here installs the *saved* pool first, so that
    // release call frees slots in the save being loaded instead — and since
    // both pools are simple monotonically-growing arenas starting at slot
    // 1, a live id very often indexes a genuinely populated saved slot,
    // silently deleting that item's saved state (a named/modded/unique
    // item) moments before `apply_deltas` overlays the very `Inventory` row
    // that references it. Only resources some spawn-time code path
    // consults while the reload is building the new cell belong in
    // `PRE_RELOAD_RESOURCES` — see its own doc comment before adding one.
    //
    // The post-reload call below is kept (and stays the wholesale
    // `restore_resources`, including `ItemInstancePool`): by then
    // `unload_cell` has already run and released against the live pool, so
    // installing the saved one is safe, and it re-asserts every saved value
    // — including the ones just installed above — over anything the reload
    // itself rebuilt (`CurrentCellContext`, `PlayerPose`). `restore_resources`
    // is a straight per-resource overwrite from the snapshot, so restoring
    // `ReferenceEnableState` a second time here is idempotent.
    //
    // #6081 — the restore overwrites the live ledgers, and every abort after
    // it ("keeping the current session") would otherwise leave the other
    // timeline's values in the session the player keeps playing.
    let live_ledgers = LiveReloadLedgers::capture(world);
    if let Err(e) =
        byroredux_save::restore_resources_subset(world, &registry, &snapshot, PRE_RELOAD_RESOURCES)
    {
        live_ledgers.reinstate(world);
        let message = format!(
            "save load ABORTED — resource restore failed before cell reload; \
             keeping the current session: {e}"
        );
        log::error!("{message}");
        notify_player(world, message);
        return;
    }

    // Suppress ordinary eviction capture/respawn restoration during a save
    // load: the outgoing session's parked state must not contaminate the save.
    // Resident rows arrive from the snapshot's component overlay; nonresident
    // rows arrive with the post-reload resource restore below.
    let outcome = crate::cell_loader::reference_state::without_parked_state(world, |world| {
        if let Some(cell_ctx) = snapshot_cell_context(&snapshot) {
            reload_interior_session(world, ctx, streaming, &args, &cell_ctx)
        } else if let Some(ext_ctx) = snapshot_exterior_context(&snapshot) {
            reload_exterior_session(world, ctx, streaming, &args, &ext_ctx)
        } else {
            let message = "save load: snapshot lost its cell/exterior context between queue and drain";
            log::error!("{message}");
            notify_player(world, message);
            None
        }
    });
    let Some(ReloadOutcome {
        location_label,
        count_label,
    }) = outcome
    else {
        // #6081 — the load was refused (preflight) or failed: hand the live
        // session its own ledgers back.
        live_ledgers.reinstate(world);
        return;
    };

    // #5435 — the session was replaced; the CLOC cursor still describes the
    // outgoing one.
    reset_story_location_cursor(world);
    discard_pending_story_events(world);
    // #5864 — and the package transitions the outgoing session's actors
    // reported: running their Begin/Change fragments in the loaded session
    // would fire for actors that never ran those packages.
    byroredux_scripting::discard_package_reports(world);
    // #6083 — the reload's own cell teardown queued `OnCellDetach` /
    // `OnUnload` for the outgoing session's scripted references; Bethesda
    // raises no unload event for a load, and the first post-load frame would
    // run those handlers against the restored script/quest state.
    byroredux_scripting::discard_pending_unload_deliveries(world);
    // #6238 — and the cell resets it raised: each rebuilt cell noted the
    // player entering it against the outgoing session's visits and clock.
    byroredux_scripting::obscript_state::discard_pending_cell_resets(world);
    discard_outgoing_session_requests(world);

    // The cell/session replacement invalidates every transient SDK entity
    // handle even when the restored population happens to reuse the same raw
    // EntityId. Rebind extension rows only through stable FormRef identity;
    // unavailable packages/forms stay retained verbatim for a later load/save.
    if let Err(error) = crate::extensions::restore_extension_state(world, &snapshot) {
        let message =
            format!("save load: extension-state restore failed after preflight: {error:#}");
        log::error!("{message}");
        notify_player(world, message);
        return;
    }

    // Re-assert saved resources over anything the reload rebuilt
    // (`CurrentCellContext`, `PlayerPose`), so inventory instance ids
    // resolve, then overlay the form-id-keyed mutable deltas. The
    // spawn-time consumers were served by the pre-reload restore above
    // (#3789); this second pass is the one that has always been here.
    if let Err(e) = byroredux_save::restore_resources(world, &registry, &snapshot) {
        let message = format!("save load: resource restore failed: {e}");
        log::error!("{message}");
        notify_player(world, message);
        return;
    }
    // #4695 — the reload above ran inside `without_parked_state`, so every
    // spawn-time `restore` call bailed with the store absent, and the saved
    // store only exists again now. Re-run the per-placement restore across
    // the resident population so rows that apply to already-spawned
    // placements are consumed: the pickup tombstone of an item taken in
    // this very cell first among them, but the window equally covers
    // inventory/equipment/weapon/actor-values/dead rows. Rows whose
    // reference is not resident stay parked for the next eviction.
    let restored_rows = crate::cell_loader::reference_state::restore_resident(world);
    if restored_rows > 0 {
        log::debug!(
            "save load: {restored_rows} parked reference row(s) applied to resident placements"
        );
    }
    // #4139 — purge continuations whose locals hold session-local
    // `EntityRef` handles. This column was registered on the claim that
    // "all nested identities are stable manifest strings"; `locals` can in
    // fact hold `ScriptValue::Entity(EntityRef)`, which the SDK itself
    // documents as scoped to a `world_generation`. The rebind that would
    // make such a handle safe lives behind
    // `ExtensionHost::restore_saved_state`, which is skipped entirely when
    // the saved `ExtensionStateSnapshot` is empty — so the generation
    // counter can stay untouched and a stale handle resolve to a
    // *different live entity* rather than fail. See
    // `drop_entity_bound_continuations` for the full mechanism.
    if let Some(mut queue) =
        world.try_resource_mut::<byroredux_scripting::PapyrusProviderContinuationQueue>()
    {
        let dropped = queue.drop_entity_bound_continuations();
        if dropped > 0 {
            log::warn!(
                "save load: dropped {dropped} papyrus provider continuation(s) bound to \
                 session-local entity handles (#4139)"
            );
        }
    }
    // #4815 — the reload picked every NPC's ambient package against the
    // outgoing session's clock; re-seat against the restored one BEFORE the
    // overlay below, so the saved procedure state lands on the saved winner
    // and the first Update tick does not clear it as a package change.
    // #6087 — `ScriptPackage` must be present when the re-seat runs: the
    // ambient selection honours a running script package over the schedule,
    // and re-seating without it stamped the scheduled winner W for an actor
    // the overlay then restored onto the script package — the next minute's
    // tick saw the "change", cleared the freshly overlaid TravelState /
    // Traveled markers and reported a spurious W→script fragment. The
    // column is replacing and idempotent, so the full overlay below reapplies
    // the same value harmlessly.
    let remap = byroredux_save::build_form_id_remap(world, &registry, &snapshot);
    match byroredux_save::apply_deltas(world, &registry, &snapshot, &remap, &["ScriptPackage"]) {
        Ok(applied) if applied > 0 => {
            log::debug!("save load: pre-seated {applied} ScriptPackage(s) before the ambient re-seat")
        }
        Ok(_) => {}
        Err(error) => log::warn!("save load: ScriptPackage pre-seat failed: {error}"),
    }
    crate::npc_spawn::ai_package::reseat_ambient_packages_after_restore(world);
    // #5054 — an exterior save holds resident cells out to the hysteresis
    // ring (`radius_load + 1`) while the reload streams only `radius_load`,
    // so the ring's rows cannot overlay. Park them as eviction would have,
    // so those placements return with their saved state when streamed in.
    let unresolved = byroredux_save::unresolved_form_id_pairs(&registry, &snapshot, &remap);
    let parked_rows = crate::cell_loader::reference_state::park_unresolved_snapshot_rows(
        world,
        &snapshot,
        &unresolved,
    );
    if parked_rows > 0 {
        log::info!(
            "save load: parked {parked_rows} saved row(s) of non-resident references \
             for their next respawn"
        );
    }
    match byroredux_save::apply_deltas(world, &registry, &snapshot, &remap, MUTABLE_DELTA_COLUMNS) {
        Ok(applied) => {
            let dead = crate::combat::reconcile_dead_actor_runtime_state(world);
            // #3488 — the overlay is additive-only, so a saved *absence*
            // (unarmed) cannot clear a live `EquippedWeapon` off the
            // process-lifetime player body, which survives the cell reload
            // untouched. Re-derive it from the `EquipmentSlots` + `Inventory`
            // rows just overlaid, using the same reconciler the runtime
            // equip/unequip path uses. Sibling of the `Dead` reconciler
            // above, per the contract in `byroredux_save::apply_deltas`.
            let player = crate::systems::PlayerEntity::get(world);
            if let Some(player) = player {
                crate::inventory::reconcile_player_equipped_weapon(world, player);
                // #5034 / #5255 — drop the player's mid-life gear handoff
                // scratch before the reconcile diffs the body roots
                // against the just-overlaid slots so worn gear agrees with
                // the restored state (the visual sibling of the weapon
                // re-derive above).
                clear_player_gear_handoff_scratch(world, player);
                crate::npc_spawn::loot_appearance::reconcile_worn_gear(world, player);
                // #5058 — the ledger reset above cannot name the player's
                // stale alias-injected memberships anymore, so strip them
                // before the first refresh reads them as authored.
                crate::inventory::reset_player_factions_to_record(world, player);
                // M47.2 — then the restored scripted memberships on top.
                byroredux_scripting::reapply_scripted_faction_changes(
                    world,
                    player,
                    byroredux_scripting::PLAYER_REFERENCE,
                );
            }
            log::info!(
                "save load: {location_label} reloaded ({count_label}); applied {} saved deltas \
                 across {} form-id-matched entities; reconciled {} dead actors",
                applied,
                remap.len(),
                dead
            );
        }
        Err(e) => {
            // Typed decoding already succeeded in the preflight above, so
            // this arm is an unexpected apply failure. Do not continue into
            // validation or pose restore on a potentially partial overlay.
            let dead = crate::combat::reconcile_dead_actor_runtime_state(world);
            let message = format!(
                "save load: delta apply failed after preflight: {e}; reconciled {dead} dead actors before aborting"
            );
            log::error!("{message}");
            notify_player(world, message);
            return;
        }
    }

    // #6083 — `ActorWatch` still describes the outgoing session's bleedout /
    // combat sets and the player's old location. Re-baseline it against the
    // overlaid world so the first pass delivers nothing across timelines.
    byroredux_scripting::rebaseline_actor_watch(world);

    // #1844 / SAVE-01 — mirror the save path's `validate_world` +
    // `validate_form_ids` pairing (see `SaveCommand::execute` above) as a
    // post-load diagnostic. A save written before a given validation
    // rule existed, or a hand-edited-but-CRC-valid file, would otherwise
    // overlay a referentially broken world with no warning. Diagnostic
    // only — a load can't cleanly fall back to the previous cell.
    let mut issues = validate_world(world);
    issues.extend(validate_form_ids(world));
    issues.extend(validate_cinematic_entity_refs(world));
    log_validation_warnings(&format!("save load: {location_label}"), &issues);

    // M45.1 refinement — put the player back where they saved, on top of
    // the reloaded cell (which spawns the player at the default door).
    if let Some(pose) = snapshot_player_pose(&snapshot) {
        if apply_player_pose(world, &pose) {
            log::info!(
                "save load: restored player pose at ({:.1}, {:.1}, {:.1}) yaw={:.2} pitch={:.2} ({})",
                pose.position[0],
                pose.position[1],
                pose.position[2],
                pose.yaw,
                pose.pitch,
                if pose.character_mode {
                    "character"
                } else {
                    "flycam"
                },
            );
        }
    }
    if let Err(error) = crate::extensions::queue_session_event(
        world,
        byroredux_sdk::event::SessionEvent {
            phase: byroredux_sdk::event::SessionPhase::LoadComplete,
            slot: Some(loaded_slot),
        },
    ) {
        log::warn!(
            "load slot {loaded_slot} completed, but its extension lifecycle event was not queued: {error}"
        );
    }
}

#[cfg(test)]
mod exterior_reload_overlay_tests {
    use super::exterior_reload_overlay_is_safe;

    /// The common case: `stream_initial_radius`'s wait loop drained
    /// `pending` normally, so the overlay is safe to run.
    #[test]
    fn zero_pending_is_safe() {
        assert!(exterior_reload_overlay_is_safe(0));
    }

    /// #3499's regression case: the worker-disconnect `break` can exit the
    /// wait loop with cells still pending — the overlay must be refused,
    /// not silently applied against an incomplete world.
    #[test]
    fn nonzero_pending_is_unsafe() {
        assert!(!exterior_reload_overlay_is_safe(1));
        assert!(!exterior_reload_overlay_is_safe(48));
    }
}
