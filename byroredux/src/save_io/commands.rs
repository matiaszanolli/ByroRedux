//! The `save` / `save.info` / `load` console commands and the
//! binary-side validation gates they run (#5585, split from
//! `save_io.rs`).
//!
//! `validate_form_ids` / `validate_cinematic_entity_refs` supplement
//! `byroredux_save::validate_world` with checks only the binary can
//! run (it owns the `FormIdPool` and the `byroredux-scripting` types);
//! both the save path and the live-load drain call them.

use super::*;

/// `save [slot]` — validate, snapshot, and atomically write the world.
/// Binary-side supplement to [`validate_world`]: every `FormIdComponent`'s
/// session-local `FormId` handle must resolve to its stable `FormIdPair`
/// through the live [`FormIdPool`].
///
/// The snapshot serializer resolves `FormId → FormIdPair` at save time and
/// **silently drops** any handle that doesn't resolve — the entity reloads
/// without its `FormIdComponent`, a lost cross-session reference (see
/// `byroredux_save::registry`). `validate_world`'s docstring defers this
/// cross-plugin check to the binary because the binary owns the
/// `FormIdPool`; running it before the write turns that silent drop into a
/// loud abort, the same defense-in-depth the core gates give. SAVE-D4-01.
pub(crate) fn validate_form_ids(world: &World) -> Vec<ValidationError> {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::FormIdPool;

    let mut errors = Vec::new();
    let Some(q) = world.query::<FormIdComponent>() else {
        return errors;
    };
    let pool = world.try_resource::<FormIdPool>();
    for (entity, comp) in q.iter() {
        let resolves = pool.as_ref().is_some_and(|p| p.resolve(comp.0).is_some());
        if !resolves {
            let detail = match pool.as_ref() {
                Some(_) => format!("FormId handle {:?} doesn't resolve in FormIdPool", comp.0),
                None => "carries a FormIdComponent but the world has no FormIdPool".to_string(),
            };
            errors.push(ValidationError {
                entity,
                kind: ValidationKind::FormId,
                detail,
            });
        }
    }
    errors
}

/// Binary-side supplement to [`validate_world`], mirroring
/// [`validate_form_ids`]'s pattern: `validate_world`'s doc comment scopes
/// `crates/save`'s checks to `byroredux-core` types only, deferring
/// anything needing another crate's components to the binary — the same
/// reason `FormId` resolution lives here instead of in `crates/save`.
///
/// #2535 / SAVE-D4-02 — covers the two entity references that live in
/// `byroredux_scripting::cinematic` and so cannot be reached from
/// `crates/save`: `HorseTetherState.horse: EntityId` and
/// `ActorCinematicState.vehicle: Option<EntityId>`. A save with either
/// pointing at an id `>= next_entity` (e.g. a tethered horse that
/// despawned mid-session while the tether component survived) previously
/// passed `validate_world` cleanly with no diagnostic anywhere in the
/// pipeline.
///
/// #4144 — this used to enumerate `validate_world`'s own checks here
/// ("every one of its four reference-class checks", with a gloss on each)
/// as the argument for why these two fields were missed. That
/// enumeration was written when there genuinely were four (`90ae915c`)
/// and was never updated: `validate_world` runs seven checks today, and
/// the claim that `validate_animation` covers "only `AnimationPlayer`"
/// went stale seven lines from the fix that changed it (#3791 added
/// `AnimationStack` and `Seated.animation_restore.clip_handle`).
///
/// The enumeration is deliberately not repaired, only removed. Restating
/// a sibling crate's internals is what keeps rotting; the reason these
/// fields need a binary-side check is *crate reachability*, which is
/// stable, and the live list is one jump away in
/// [`byroredux_save::validate_world`].
pub(crate) fn validate_cinematic_entity_refs(world: &World) -> Vec<ValidationError> {
    use byroredux_scripting::cinematic::{ActorCinematicState, HorseTetherState};

    let mut errors = Vec::new();
    let next_entity = world.next_entity_id();

    if let Some(q) = world.query::<HorseTetherState>() {
        for (entity, tether) in q.iter() {
            validate_entity_reference(
                entity,
                "HorseTetherState.horse",
                tether.horse,
                next_entity,
                &mut errors,
            );
        }
    }

    if let Some(q) = world.query::<ActorCinematicState>() {
        for (entity, state) in q.iter() {
            if let Some(vehicle) = state.vehicle {
                validate_entity_reference(
                    entity,
                    "ActorCinematicState.vehicle",
                    vehicle,
                    next_entity,
                    &mut errors,
                );
            }
        }
    }

    errors
}

pub struct SaveCommand;

/// Execute a quicksave after the scheduler has quiesced. Player input must use
/// [`queue_player_save_action`] instead of calling this lock-wide operation.
pub(crate) fn quicksave(world: &World) -> CommandOutput {
    SaveCommand.execute(world, "")
}

impl ConsoleCommand for SaveCommand {
    fn name(&self) -> &str {
        "save"
    }
    fn description(&self) -> &str {
        "save [slot] — validate + snapshot the world to a slot (default: next ring slot)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        // `registry` (SaveRegistry) stays held through `save_world`/`encode`
        // below, alongside the ~26 component-storage + ~7 resource read
        // locks `save_world`/`validate_world`/`validate_form_ids` take —
        // the widest single-hold edge fan-out in the process. Production
        // callers are constrained to quiescent lanes: remote operator command
        // dispatch runs in the exclusive `DebugDrainSystem`; native-overlay
        // command dispatch and the player-action drain both run after
        // `Scheduler::run` has joined every parallel system. No live system
        // can therefore form the other half of an ABBA cycle against this
        // hold. New input/SDK integrations must enqueue through
        // `queue_player_save_action`; moving execution onto a live scheduler
        // lane needs this ordering re-derived. #3113 / #2154.
        let Some(registry) = world.try_resource::<SaveRegistry>() else {
            return CommandOutput::error("save registry not installed");
        };
        let Some(state) = world.try_resource::<SaveState>() else {
            return CommandOutput::error("save directory not installed");
        };
        // #4138 — refuse mid-transition. An interior transition clears
        // `CurrentCellContext` at teardown and reinstalls it only when the
        // budgeted apply job reaches `Complete`, so a save taken in that
        // window snapshots a world with neither cell nor exterior context.
        // None of the referential-integrity gates below look at context
        // presence, so such a save committed happily, consumed a ring slot
        // (evicting whatever it displaced) and reported success — while
        // being unloadable from the moment it was written.
        //
        // This is the third leg of a pattern the codebase already applies
        // twice on the load side: `LoadCommand::execute` refuses a snapshot
        // carrying neither context, and `step_save_loads` refuses to start
        // a load while `interior_transition.is_some()`. Only the save side
        // was missing.
        //
        // Worded distinctly from the loose-NIF rejection on purpose: the
        // ambiguity between "this save has no location" and "this session
        // has no location" is precisely what made the original failure so
        // hard to attribute.
        if world
            .try_resource::<crate::cell_loader::CellTransitionInFlight>()
            .is_some_and(|flag| flag.0)
        {
            return CommandOutput::error(
                "save REFUSED: a cell transition is still loading — the world has no \
                 cell or exterior context until it finishes, so this save would be \
                 written successfully and then be permanently unloadable. Try again \
                 once the destination has finished loading.",
            );
        }
        // #5253 — the flag above is derived state, synced once per frame at
        // the head of `step_player_save_actions`, and is correct only for
        // the drain that immediately follows it. The other production
        // ingress reads it across a gap: the remote console dispatches
        // inside `DebugDrainSystem` (before that frame's sync) and the
        // native-overlay console dispatches from `render_one_frame` (after
        // `step_cell_transition`). A cover-less transition —
        // `loading_screen.begin` failing for lack of artwork — tears both
        // contexts down inside `step_cell_transition`, so for the rest of
        // that frame (and into the next frame's scheduler) the flag still
        // reads `false` while the world already carries neither context.
        // Read the invariant itself instead of the flag: with a cell index
        // installed, a context-less world is exactly the unloadable-
        // snapshot state above, whatever the flag claims. The index is
        // installed at cell-load moments only (interior
        // `load_cell_with_masters`, the exterior `assemble_exterior_streaming`
        // funnel) and never removed by teardown, so an index without a
        // context cannot be a legitimate loose-NIF / pre-cell boot state.
        if world
            .try_resource::<crate::cell_loader::LoadedCellIndex>()
            .is_some()
            && world
                .try_resource::<crate::cell_loader::CurrentCellContext>()
                .is_none()
            && world
                .try_resource::<crate::cell_loader::CurrentExteriorContext>()
                .is_none()
        {
            return CommandOutput::error(
                "save REFUSED: the session has a loaded cell index but neither an \
                 interior nor an exterior cell context — a cell transition is \
                 mid-flight (or its loading cover failed to start), so this save \
                 would be written successfully and then be permanently unloadable. \
                 Try again once the destination has finished loading.",
            );
        }
        // #4372 — honor `Game.SetInChargen(abDisableSaving = true)`. MQ101
        // disables saving around the execution-block race menu; before this
        // gate the flag was recorded and read by nothing, so a quicksave went
        // through inside a block Bethesda's own runtime refuses.
        if world
            .try_resource::<byroredux_scripting::CinematicPresentationState>()
            .is_some_and(|presentation| presentation.disable_saving)
        {
            return CommandOutput::error(
                "save REFUSED: the running quest has disabled saving \
                 (Game.SetInChargen) — try again once it re-enables it.",
            );
        }

        // Explicit slot, or the ring's next slot — a quicksave only
        // actually *advances* the ring once validation passes and the
        // write is committed to, below (#2017 / SAVE-D4-NEW-01). `peek`
        // is non-mutating: it reports the slot `advance` would hand out
        // without consuming a rotation, so the round-robin invariant
        // ("next quicksave lands one slot after the last SUCCESSFUL
        // one") holds even across repeated validation-aborted attempts.
        let is_quicksave = args.trim().is_empty();
        let slot = if is_quicksave {
            state.ring.peek()
        } else {
            match args.trim().parse::<u32>() {
                Ok(n) => n,
                Err(_) => return CommandOutput::error(format!("invalid slot '{}'", args.trim())),
            }
        };

        // Pre-save validation — refuse to persist a broken world. Core
        // referential-integrity gates plus the binary-only FormId-pool
        // resolvability check (which needs the `FormIdPool` this crate owns)
        // and the cinematic EntityId-reference check (#2535 — needs
        // `byroredux-scripting` types `crates/save` doesn't depend on).
        let mut issues = validate_world(world);
        issues.extend(validate_form_ids(world));
        issues.extend(validate_cinematic_entity_refs(world));
        if !issues.is_empty() {
            let mut lines = vec![format!(
                "save ABORTED: {} referential-integrity issue(s) — refusing to write a poisoned save:",
                issues.len()
            )];
            for issue in issues.iter().take(20) {
                lines.push(format!(
                    "  [{:?}] entity {}: {}",
                    issue.kind, issue.entity, issue.detail
                ));
            }
            if issues.len() > 20 {
                lines.push(format!("  … and {} more", issues.len() - 20));
            }
            return CommandOutput::lines(lines);
        }

        // Nothing past this point needs access to
        // `SaveState` — only the directory, cheaply copied — so drop the
        // guard now rather than holding it across `save_world`'s ~30-storage
        // snapshot walk (SAVE-D3-02 / #2154).
        let dir = state.dir.clone();
        drop(state);

        let mut snapshot = match save_world(world, &registry) {
            Ok(s) => s,
            Err(e) => return CommandOutput::error(format!("snapshot failed: {e}")),
        };
        let extension_rows = match crate::extensions::capture_extension_state(world, &mut snapshot)
        {
            Ok(count) => count,
            Err(error) => {
                return CommandOutput::error(format!("extension-state snapshot failed: {error:#}"));
            }
        };
        let bytes = match encode(&snapshot, registry.schema_fingerprint()) {
            Ok(b) => b,
            Err(e) => return CommandOutput::error(format!("encode failed: {e}")),
        };
        match disk::write_slot(&dir, slot, &bytes) {
            Ok(path) => {
                // Consume quicksave rotation only after every validation,
                // snapshot/extension-state encode, and disk commit succeeds.
                // A rejected transient extension row must not skip a slot.
                if is_quicksave {
                    world.resource_mut::<SaveState>().ring.advance();
                }
                if let Err(error) = crate::extensions::queue_session_event(
                    world,
                    byroredux_sdk::event::SessionEvent {
                        phase: byroredux_sdk::event::SessionPhase::SaveComplete,
                        slot: Some(slot),
                    },
                ) {
                    log::warn!(
                        "save slot {slot} committed, but its extension lifecycle event was not queued: {error}"
                    );
                }
                CommandOutput::lines(vec![
                    format!("saved slot {slot} → {}", path.display()),
                    format!(
                        "  {} entities-worth of rows across {} component columns, {} resource(s); \
                     {extension_rows} extension row(s)",
                        snapshot.row_count(),
                        snapshot.components.len(),
                        snapshot.resources.len()
                    ),
                    format!(
                        "  {} bytes (next_entity={})",
                        bytes.len(),
                        snapshot.next_entity
                    ),
                ])
            }
            Err(e) => CommandOutput::error(format!("write failed: {e}")),
        }
    }
}

/// `save.info <slot>` — decode + verify a slot and report its contents,
/// without mutating the live world.
pub struct SaveInfoCommand;

impl ConsoleCommand for SaveInfoCommand {
    fn name(&self) -> &str {
        "save.info"
    }
    fn description(&self) -> &str {
        "save.info <slot> — verify (magic/version/CRC/schema) + summarise a save slot"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(registry) = world.try_resource::<SaveRegistry>() else {
            return CommandOutput::error("save registry not installed");
        };
        let Some(state) = world.try_resource::<SaveState>() else {
            return CommandOutput::error("save directory not installed");
        };
        let slot = match args.trim().parse::<u32>() {
            Ok(n) => n,
            Err(_) => {
                let slots = disk::list_slots(&state.dir);
                return CommandOutput::lines(vec![
                    "usage: save.info <slot>".to_string(),
                    format!("available slots: {slots:?}"),
                ]);
            }
        };

        let bytes = match disk::read_slot(&state.dir, slot) {
            Ok(b) => b,
            Err(e) => return CommandOutput::error(format!("read slot {slot}: {e}")),
        };
        match byroredux_save::decode(&bytes, registry.schema_fingerprint()) {
            Ok(snap) => {
                let mut lines = vec![
                    format!("slot {slot}: VALID ({} bytes)", bytes.len()),
                    format!(
                        "  next_entity={}, {} strings, {} rows",
                        snap.next_entity,
                        snap.strings.len(),
                        snap.row_count()
                    ),
                ];
                // #3500 — mirror LoadCommand's three-arm classification
                // (cell / exterior / neither) so an operator inspecting an
                // exterior save is told the truth `load` already knows:
                // it is live-loadable, not a loose save.
                match (
                    snapshot_cell_context(&snap),
                    snapshot_exterior_context(&snap),
                ) {
                    (Some(ctx), _) => lines.push(format!(
                        "  cell: {} (esm {}, {} master(s))",
                        ctx.cell_editor_id,
                        ctx.esm_path,
                        ctx.masters.len()
                    )),
                    (None, Some(ctx)) => lines.push(format!(
                        "  exterior: worldspace '{}' @ ({},{}) (esm {}, {} master(s), \
                         radius {}/{})",
                        ctx.worldspace_key,
                        ctx.grid.0,
                        ctx.grid.1,
                        ctx.esm_path,
                        ctx.masters.len(),
                        ctx.radius_load,
                        ctx.radius_unload,
                    )),
                    (None, None) => lines.push("  cell: <none — loose save>".to_string()),
                }
                if let Some(pose) = snapshot_player_pose(&snap) {
                    lines.push(format!(
                        "  player: ({:.1}, {:.1}, {:.1}) yaw={:.2} pitch={:.2} ({})",
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
                    ));
                }
                for (name, col) in &snap.components {
                    let rows = col.as_array().map_or(0, |a| a.len());
                    lines.push(format!("  {name}: {rows} rows"));
                }
                for name in snap.resources.keys() {
                    lines.push(format!("  resource {name}"));
                }
                CommandOutput::lines(lines)
            }
            Err(e) => CommandOutput::error(format!("slot {slot} INVALID: {e}")),
        }
    }
}

/// `load <slot>` — decode + verify a slot and queue it for the next
/// frame's live-load drain.
///
/// Read-only here (holds `&World`): it decodes, validates the container,
/// checks the snapshot carries a cell context (live load needs a cell to
/// reload), and pushes the snapshot into [`PendingSaveLoadSlot`].
/// [`execute_pending_save_loads`] does the actual cell reload + delta
/// apply between frames.
pub struct LoadCommand;

/// Queue a verified slot for the between-frame live-load drain.
pub fn queue_load_slot(world: &World, slot: u32) -> CommandOutput {
    LoadCommand.execute(world, &slot.to_string())
}

/// Parse and queue the `--load` value through the same command path used by
/// F9, the pause menu, and the debug console.
pub fn queue_startup_load(world: &World, value: &str) -> CommandOutput {
    match value.parse::<u32>() {
        Ok(slot) => queue_load_slot(world, slot),
        Err(_) => CommandOutput::error(format!(
            "--load requires a numeric save slot, got '{value}'"
        )),
    }
}

pub(crate) fn command_output_is_failure(output: &CommandOutput) -> bool {
    output
        .lines
        .iter()
        .any(|line| line.starts_with("Error:") || line.starts_with("save ABORTED"))
}

/// Queue the newest on-disk slot, matching conventional quickload behavior.
pub(crate) fn quickload_latest(world: &World) -> CommandOutput {
    let slots = world
        .try_resource::<SaveState>()
        .map(|state| disk::slots_by_recency(&state.dir))
        .unwrap_or_default();
    if slots.is_empty() {
        return CommandOutput::error("no save slots available");
    }
    let mut rejected = Vec::new();
    for slot in slots {
        let mut output = queue_load_slot(world, slot);
        if !command_output_is_failure(&output) {
            if !rejected.is_empty() {
                rejected.push(format!("falling back to valid slot {slot}"));
                rejected.append(&mut output.lines);
                return CommandOutput::lines(rejected);
            }
            return output;
        }
        rejected.push(format!(
            "skipped invalid quickload slot {slot}: {}",
            output.lines.join(" | ")
        ));
    }
    rejected.push("Error: no decodable save slots available".to_string());
    CommandOutput::lines(rejected)
}

/// Execute queued player actions after the scheduler has joined.
///
/// The queue guard is dropped before either command starts, so it never joins
/// the save registry/component lock set. FIFO order makes a quicksave followed
/// by quickload deterministic within one frame.
pub(crate) fn execute_pending_player_save_actions(
    world: &World,
) -> Vec<(PlayerSaveAction, CommandOutput)> {
    let actions = {
        let Some(mut pending) = world.try_resource_mut::<PendingPlayerSaveActions>() else {
            return Vec::new();
        };
        std::mem::take(&mut pending.0)
    };
    actions
        .into_iter()
        .map(|action| {
            let output = match action {
                PlayerSaveAction::Quicksave => quicksave(world),
                PlayerSaveAction::Quickload => quickload_latest(world),
            };
            (action, output)
        })
        .collect()
}

impl ConsoleCommand for LoadCommand {
    fn name(&self) -> &str {
        "load"
    }
    fn description(&self) -> &str {
        "load <slot> — reload the saved cell and apply saved game-state deltas"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(registry) = world.try_resource::<SaveRegistry>() else {
            return CommandOutput::error("save registry not installed");
        };
        let Some(state) = world.try_resource::<SaveState>() else {
            return CommandOutput::error("save directory not installed");
        };
        let slot = match args.trim().parse::<u32>() {
            Ok(n) => n,
            Err(_) => return CommandOutput::error("usage: load <slot>"),
        };

        let bytes = match disk::read_slot(&state.dir, slot) {
            Ok(b) => b,
            Err(e) => return CommandOutput::error(format!("read slot {slot}: {e}")),
        };
        let snapshot = match byroredux_save::decode(&bytes, registry.schema_fingerprint()) {
            Ok(s) => s,
            Err(e) => return CommandOutput::error(format!("slot {slot} INVALID: {e}")),
        };
        // EX-09/17 item 4 — a live load can reconstruct either kind of
        // session now; only reject a snapshot carrying neither.
        let destination_label = match (
            snapshot_cell_context(&snapshot),
            snapshot_exterior_context(&snapshot),
        ) {
            (Some(cell_ctx), _) => format!("cell {}", cell_ctx.cell_editor_id),
            (None, Some(ext_ctx)) => format!(
                "worldspace '{}' @ ({},{})",
                ext_ctx.worldspace_key, ext_ctx.grid.0, ext_ctx.grid.1
            ),
            (None, None) => {
                return CommandOutput::error(
                    "save has no cell or exterior context (loose save) — live load needs one",
                );
            }
        };

        // Queue for the between-frames drain (needs &mut World + renderer).
        match world.try_resource_mut::<PendingSaveLoadSlot>() {
            Some(mut pending) => {
                // #1848 / SAVE-05 — the pending slot is a single Option, so
                // a second `load` issued before `execute_pending_save_loads`
                // drains replaces the first. Last-writer-wins is the intended
                // semantics; what was wrong is that the discarded request
                // vanished without a trace. Report it on both channels: the
                // engine log (for a `--bench-hold` session) and the command
                // output (for the byro-dbg operator who typed it).
                let superseded = pending.snapshot.is_some().then_some(pending.slot);
                pending.snapshot = Some(snapshot);
                pending.slot = slot;
                let mut lines = Vec::new();
                if let Some(prev) = superseded {
                    let msg = format!(
                        "queued load of slot {prev} superseded by slot {slot} before drain"
                    );
                    log::info!("save load: {msg}");
                    lines.push(msg);
                }
                lines.push(format!(
                    "queued load of slot {slot} → {destination_label} (applies next frame)",
                ));
                CommandOutput::lines(lines)
            }
            None => CommandOutput::error("load slot not installed"),
        }
    }
}
