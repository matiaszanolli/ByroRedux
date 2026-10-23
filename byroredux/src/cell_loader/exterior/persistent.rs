//! The worldspace persistent CELL and its logical actor stubs: the apply job, the
//! persistent window, and the tile helpers (split out of `exterior.rs`, #6271).

use super::*;

/// Resumable main-thread apply for the active worldspace's persistent CELL.
pub(crate) struct PersistentCellApplyJob {
    pub(super) cell_root: EntityId,
    pub(super) cell_form_id: u32,
    pub(super) authored_ref_count: usize,
    /// Start of the entity range the *next* [`Self::stamp_slice`] covers.
    ///
    /// A moving cursor, not a fixed origin (#3379). This job yields once
    /// per cooperative slice and stamps on every yield *and* on
    /// completion; when the field was pinned to its constructor value
    /// every stamp re-covered the whole prefix, so an entity created in
    /// slice `k` of an `N`-slice apply was pushed into
    /// `CellRootIndex[cell_root]` `N - k + 1` times. That index is taken
    /// verbatim as `unload_cell`'s victim list, where a duplicate costs
    /// one *extra* mesh/texture refcount decrement per repeat — the
    /// handle reaches zero while a live holder still draws it, and its
    /// BLAS is retained against a buffer already queued for destruction.
    /// The sibling [`ExteriorCellApplyJob`] never had the bug: it takes a
    /// fresh `world.next_entity_id()` at the top of each work unit.
    ///
    /// #5483 — the cursor is *reseeded* to `world.next_entity_id()` at
    /// the top of every `advance` ([`Self::begin_advance`]), matching
    /// that sibling's discipline. Within one `advance` call only this
    /// job's own loaders spawn, so stamps cover exactly what the job
    /// created. A construction-time seed (the pre-#5483 shape) instead
    /// spanned every inter-frame gap: in `ForegroundFirst` boot the job
    /// sits un-advanced while the bootstrap spawns the arrival cell, the
    /// camera and the player, and the first stamp then claimed — and the
    /// worldspace teardown later despawned — all of them.
    pub(super) stamp_cursor: EntityId,
    pub(super) local_refs: Vec<byroredux_plugin::esm::cell::PlacedRef>,
    pub(super) remote_actor_refs: Vec<byroredux_plugin::esm::cell::PlacedRef>,
    pub(super) references: Option<Box<ReferenceLoadJob>>,
    pub(super) reference_entity_count: Option<usize>,
    pub(super) logical_stub_refs: Vec<byroredux_plugin::esm::cell::PlacedRef>,
    pub(super) next_logical_stub: usize,
    /// #4112 — how many of `logical_stub_refs` actually attached a script.
    /// Surfaced in the summary line below so the fix is observable without a
    /// game-data run: this loop previously produced no output naming the
    /// decision at all, which is why the gap survived a full audit pass.
    pub(super) logical_stub_scripts: usize,
    pub(super) local_actor_3d: usize,
    pub(super) local_actor_count: usize,
}

pub(crate) enum PersistentCellApplyProgress {
    Pending(PersistentCellApplyJob),
    Complete,
}

impl PersistentCellApplyJob {
    pub(crate) fn cell_root(&self) -> EntityId {
        self.cell_root
    }

    /// Stamp every entity created since the previous stamp onto this
    /// job's cell root, then advance the cursor past them (#3379).
    ///
    /// Called on both cooperative-yield paths and on completion. Each
    /// call covers a half-open range disjoint from every other, so
    /// `CellRootIndex[cell_root]` ends up holding each entity exactly
    /// once no matter how many slices the apply takes.
    pub(super) fn stamp_slice(&mut self, world: &mut World) {
        let last = world.next_entity_id();
        stamp_cell_root_range(world, self.cell_root, self.stamp_cursor, last);
        self.stamp_cursor = last;
    }

    /// Reseed [`Self::stamp_cursor`] to *now*, at the top of every
    /// `advance` (#5483).
    ///
    /// Between two `advance` calls the job is parked in the streaming
    /// state while the rest of the frame runs, and anything may spawn —
    /// the bootstrap's camera and player, a crossing frame's LOD rings,
    /// the appearance loaders. None of those belong to the persistent
    /// CELL, so each `advance` starts a fresh range exactly like
    /// [`ExteriorCellApplyJob`]'s per-work-unit `next_entity_id()`. Only
    /// what this job's own loaders create inside the call is stamped.
    /// Split out of `advance` so the stamp tests can drive it without a
    /// `VulkanContext`.
    pub(super) fn begin_advance(&mut self, world: &World) {
        self.stamp_cursor = world.next_entity_id();
    }

    /// Release the ECS-side state an unfinished persistent apply is
    /// holding, ahead of the drain's `unload_cell(persistent_root)`
    /// (#3377).
    ///
    /// `ReferenceLoadJob` stages clip handles for cache-miss REFRs in
    /// `accum.pending_clip_handles` and commits them to
    /// `NifImportRegistry` only at end-of-cell, so a cancelled cell
    /// never reaches the commit and must release them by hand — the
    /// `#863` discipline [`ExteriorCellApplyJob::cancel`] already
    /// honours. Unlike that sibling this does *not* call `unload_cell`:
    /// `drain_streaming_state` already unloads `persistent_root`
    /// itself, and the partially-spawned entities are reachable from it
    /// because [`Self::stamp_slice`] runs on every yield.
    pub(crate) fn cancel(mut self, world: &World) {
        if let Some(references) = self.references.take() {
            references.cancel(world);
        }
    }

    pub(crate) fn advance(
        mut self,
        wctx: &ExteriorWorldContext,
        world: &mut World,
        ctx: &mut VulkanContext,
        tex_provider: &TextureProvider,
        mat_provider: Option<&mut MaterialProvider>,
        budget: &mut FrameTimeBudget,
    ) -> PersistentCellApplyProgress {
        self.begin_advance(world);
        if self.reference_entity_count.is_none() {
            let result = load_references_budgeted(
                &self.local_refs,
                &wctx.record_index.cells,
                &wctx.record_index,
                &wctx.record_index.races,
                wctx.record_index.game,
                world,
                ctx,
                tex_provider,
                mat_provider,
                &format!("{} persistent", wctx.worldspace_key),
                wctx.load_order.as_ref(),
                &std::collections::HashSet::new(),
                self.references.take(),
                budget,
            );
            match result {
                ReferenceLoadProgress::Pending(references) => {
                    self.references = Some(references);
                    self.stamp_slice(world);
                    flush_pending_cell_textures_on_yield(ctx);
                    return PersistentCellApplyProgress::Pending(self);
                }
                ReferenceLoadProgress::Complete(result) => {
                    self.reference_entity_count = Some(result.entity_count);
                    self.prepare_logical_actor_stubs(wctx, world);
                }
            }
        }

        while self.next_logical_stub < self.logical_stub_refs.len() {
            if budget.should_yield() {
                self.stamp_slice(world);
                flush_pending_cell_textures_on_yield(ctx);
                return PersistentCellApplyProgress::Pending(self);
            }
            let placed = &self.logical_stub_refs[self.next_logical_stub];
            // #2664 (SCR-D7-NEW11-03) — this used to open-code
            // `stamp_quest_reference`'s body and insert nothing else. Two
            // problems, one of them live: the copy would drift from the
            // canonical stamper as `SceneAliasCandidate` grows (the
            // divergence #2541 guards against), and it left the entity with
            // no transform at all, which excludes it from every
            // distance-ranked alias fill (see
            // `spawn_logical_quest_reference`'s docs). `placed.position` was
            // available the whole time. Same conversion as the REFR
            // placement path in `references::load_references_budgeted`.
            //
            // #4112 (SCR-D7-2026-09-11-01) — the identity half above was only
            // half the job: this loop reached no `attach_*` function at all, so
            // a persistent quest actor's own SCRI/VMAD was silently absent for
            // as long as it was stub-only. The paired helper is now the only
            // way to spawn one of these, so the two cannot drift apart again.
            let (stub, attached) = super::super::references::spawn_logical_quest_reference_with_scripts(
                world,
                placed,
                &wctx.load_order,
                &wctx.record_index,
                super::super::transition::position_zup_to_yup(placed.position),
                super::super::transition::rotation_zup_to_yup_quat(placed.rotation),
                placed.scale,
            );
            // #6220 — its home cell is not loaded: its script runs no `GameMode`.
            world.insert(stub, byroredux_scripting::obscript_objects::UnloadedReferenceStub);
            if attached {
                self.logical_stub_scripts += 1;
            }
            self.next_logical_stub += 1;
            budget.complete_unit();
        }

        if !self.logical_stub_refs.is_empty() {
            byroredux_scripting::mark_scene_actor_bindings_dirty(world);
        }
        self.stamp_slice(world);
        flush_pending_cell_textures(ctx);
        log::info!(target: "engine::scene_persistent",
            "Worldspace '{}' persistent CELL {:08X}: {} local refs -> {} entities; {}/{} local actors have 3D, {} logical-only actor identities ({} with scripts) ({} authored refs total)",
            wctx.worldspace_key,
            self.cell_form_id,
            self.local_refs.len(),
            self.reference_entity_count.unwrap_or(0),
            self.local_actor_3d,
            self.local_actor_count,
            self.logical_stub_refs.len(),
            self.logical_stub_scripts,
            self.authored_ref_count,
        );
        // #6219 — the persistent CELL is "entered" every time the worldspace is
        // built, so a return after `iHoursToRespawnCell` owes it a reset like any
        // cell. Retire it now that its references have spawned, as the interior
        // and tile jobs do (#6102): left pending, `mark_cell_reset` rescanned every
        // frame of the stay and reset references stamped under this root later
        // (#6076 tile-streamed persistent actors, re-spawns after eviction).
        byroredux_scripting::obscript_state::cell_load_finished(world, self.cell_root);
        PersistentCellApplyProgress::Complete
    }

    fn prepare_logical_actor_stubs(&mut self, wctx: &ExteriorWorldContext, world: &World) {
        let live_actor_candidates: std::collections::HashMap<u32, (EntityId, bool)> = world
            .query::<byroredux_scripting::SceneAliasCandidate>()
            .map(|query| {
                query
                    .iter()
                    .map(|(entity, candidate)| {
                        (
                            candidate.reference_form_id,
                            (
                                entity,
                                world.has::<byroredux_core::ecs::GlobalTransform>(entity),
                            ),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let local_actor_refs: Vec<_> = self
            .local_refs
            .iter()
            // #2567 — `is_actor` covers CREA as well as NPC_, so a
            // persistent creature is counted and stubbed like any other actor
            // instead of being invisible to the actor bookkeeping entirely.
            .filter(|placed| wctx.record_index.is_actor(placed.base_form_id))
            .collect();
        self.local_actor_count = local_actor_refs.len();
        self.local_actor_3d = local_actor_refs
            .iter()
            .filter(|placed| {
                live_actor_candidates
                    .get(&placed.form_id)
                    .is_some_and(|(_, loaded_3d)| *loaded_3d)
            })
            .count();
        // #6076 — a remote actor's tile may have streamed in (and spawned
        // it for real) while this job was still applying.
        self.logical_stub_refs.extend(
            self.remote_actor_refs
                .drain(..)
                .filter(|placed| !live_actor_candidates.contains_key(&placed.form_id)),
        );
        self.logical_stub_refs.extend(
            local_actor_refs
                .into_iter()
                .filter(|placed| !live_actor_candidates.contains_key(&placed.form_id))
                .cloned(),
        );
    }
}

/// Bound on how many WNAM hops [`resolve_persistent_cell`] follows before
/// giving up. Vanilla WNAM chains are at most a couple of hops deep; this
/// exists purely as a cycle guard against malformed/mod-authored data, not
/// because real content is expected to approach it.
pub(super) const MAX_WNAM_DEPTH: usize = 8;

/// Resolve the persistent CELL a worldspace should use: its own, if it
/// authors one, otherwise the nearest WNAM ancestor's (EX-14/15 item C2,
/// #2369).
///
/// Before this, `begin_worldspace_persistent_cell` exact-matched only the
/// *current* worldspace's own key. A worldspace with no persistent CELL of
/// its own therefore got none at all — every globally-persistent quest
/// actor/ref that lives ONLY in a parent worldspace's persistent CELL
/// (the structural home Bethesda stores them in) silently failed to spawn
/// the moment the player entered a child worldspace, even though the child
/// is meant to inherit that content by construction (WNAM). Walking the
/// chain fixes that without changing anything for the common case (a
/// worldspace with its own persistent CELL resolves on the first
/// iteration, identical to before).
pub(super) fn resolve_persistent_cell<'a>(
    index: &'a byroredux_plugin::esm::cell::EsmCellIndex,
    worldspace_key: &'a str,
) -> Option<(&'a str, &'a byroredux_plugin::esm::cell::CellData)> {
    let mut current_key = worldspace_key.to_string();
    let mut visited = HashSet::new();
    for _ in 0..MAX_WNAM_DEPTH {
        if let Some(cell) = index.worldspace_persistent_cells.get(&current_key) {
            // `current_key` is a local `String`, so its borrow can't outlive
            // this call — re-look-up through `worldspaces` (owned by
            // `index`, lifetime `'a`) to hand back a borrow with the right
            // lifetime for the diagnostic log line below instead.
            let owner_key = index
                .worldspaces
                .get(&current_key)
                .map(|w| w.editor_id.as_str())
                .unwrap_or(worldspace_key);
            return Some((owner_key, cell));
        }
        if !visited.insert(current_key.clone()) {
            break; // WNAM cycle — malformed data, stop rather than loop.
        }
        let Some(record) = index.worldspaces.get(&current_key) else {
            break;
        };
        let Some(parent_form_id) = record.parent_worldspace else {
            break;
        };
        let Some(parent_key) = index
            .worldspaces
            .values()
            .find(|w| w.form_id == parent_form_id)
            .map(|w| w.editor_id.to_ascii_lowercase())
        else {
            break;
        };
        current_key = parent_key;
    }
    None
}

/// EX-14/15 item C2 (#2369) — pure core of the persistent-CELL identity
/// check: does `new_index`'s `new_worldspace_key` resolve (via
/// [`resolve_persistent_cell`]) to the SAME persistent CELL as
/// `current_form_id`? Split out from [`persistent_root_survives_crossing`]
/// so it's unit-testable the same way `resolve_persistent_cell` is — no
/// `World` required. See `docs/engine/stream-boundary-state-continuity.md`
/// §1 + §7 for why this comparison exists: a worldspace crossing that
/// resolves to the same persistent CELL (a child leaving back to its
/// parent, or between siblings sharing one ancestor's persistent CELL via
/// the WNAM chain) makes the persistent-CELL drain+rebuild pure waste —
/// the *ordinary* grid tiles still always change and must still drain.
pub(crate) fn persistent_cell_identity_unchanged(
    new_index: &byroredux_plugin::esm::cell::EsmCellIndex,
    new_worldspace_key: &str,
    current_form_id: u32,
) -> bool {
    resolve_persistent_cell(new_index, new_worldspace_key)
        .is_some_and(|(_, cell)| cell.form_id == current_form_id)
}

/// Whether `current_persistent_root` (the currently-active persistent-CELL
/// root, if any) should survive a worldspace crossing into `new_wctx`,
/// per EX-14/15 item C2. `None` means the ordinary full drain+rebuild is
/// correct — there's no active root, its [`CellFormId`] is missing (should
/// not happen for a root [`begin_worldspace_persistent_cell`] built, but a
/// missing component fails safe into "rebuild" rather than panicking), the
/// resolved identity differs, or the root's own spawn is still in flight
/// (#3376). `Some(root)` means the caller must
/// detach `root` from the old streaming state's `persistent_root` field
/// *before* calling `drain_streaming_state` (so the drain's unconditional
/// `unload_cell` never sees it) and hand it back in through
/// `assemble_exterior_streaming`'s `preserved_persistent_root` parameter.
pub(crate) fn persistent_root_survives_crossing(
    world: &World,
    current_persistent_root: Option<EntityId>,
    new_wctx: &ExteriorWorldContext,
    persistent_apply_in_flight: bool,
) -> Option<EntityId> {
    // #3376 — identity is necessary but not sufficient: the root must also be
    // *finished*. A `PersistentCellApplyJob` is the resumable continuation
    // that spawns the CELL's `local_refs` / `logical_stub_refs`, and it lives
    // on the streaming state that the imminent `drain_streaming_state` throws
    // away (it cancels both applies — `persistent_apply` since #3377 — and
    // neither continuation survives the moved-out state). Preserving the
    // root would then make
    // `stream_initial_radius`'s `persistent_root.is_none() &&
    // persistent_apply.is_none()` guard false, so no replacement job is ever
    // created and the unspawned tail of that CELL — doors, quest refs, unique
    // actors — is silently absent for the rest of the session.
    //
    // A job in flight is the normal state for the first frames after entering
    // any worldspace (`ExteriorBootstrapMode::ForegroundFirst`, i.e. every
    // launch without `--bench-frames`), so this is the ordinary interactive
    // path, not an exotic one. Falling back to the always-rebuild path costs a
    // teardown and respawn of a root that was incomplete anyway.
    if persistent_apply_in_flight {
        return None;
    }
    let root = current_persistent_root?;
    let current_form_id = world.get::<CellFormId>(root)?.0;
    persistent_cell_identity_unchanged(
        &new_wctx.record_index.cells,
        &new_wctx.worldspace_key,
        current_form_id,
    )
    .then_some(root)
}


/// Begin spawning the active worldspace's persistent CELL once, outside the
/// coordinate-streamed tile set: the refs within `full_3d_radius` of the
/// arrival, plus a logical stub for every actor beyond it. The refs beyond it
/// stream with the tiles they stand in (#6076, [`PersistentCellWindow`]).
///
/// Bethesda stores globally persistent exterior actors here (Skyrim's
/// Hadvar/Ralof ACHRs are the MQ101-critical examples). Giving the CELL its
/// own root keeps those entities alive across ordinary tile unloads while
/// still making the whole set reclaimable on a worldspace transition. The
/// returned job shares the steady-state reference budget so large persistent
/// cells cannot monopolize the startup frame (#2275).
///
/// EX-14/15 item C2 (#2369) — the persistent CELL used is resolved through
/// [`resolve_persistent_cell`], which walks the WNAM parent chain when this
/// worldspace authors no persistent CELL of its own.
pub(crate) fn begin_worldspace_persistent_cell(
    wctx: &ExteriorWorldContext,
    center_grid: (i32, i32),
    full_3d_radius: i32,
    world: &mut World,
) -> Option<PersistentCellApplyJob> {
    super::super::load::ensure_globals_resource(world, &wctx.record_index.globals);
    let index = &wctx.record_index.cells;
    let (owner_key, cell) = resolve_persistent_cell(index, &wctx.worldspace_key)?;
    if !owner_key.eq_ignore_ascii_case(&wctx.worldspace_key) {
        log::info!(
            "Worldspace '{}' authors no persistent CELL of its own — inheriting '{}''s \
             via the WNAM parent chain (EX-14/15 item C2 / #2369)",
            wctx.worldspace_key,
            owner_key,
        );
    }

    let mut local_refs = Vec::new();
    let mut remote_actor_refs = Vec::new();
    for placed in &cell.references {
        if persistent_position_is_local(placed.position, center_grid, full_3d_radius) {
            local_refs.push(placed.clone());
        } else if wctx.record_index.is_actor(placed.base_form_id) {
            remote_actor_refs.push(placed.clone());
        }
    }

    // #6076 — the refs left out above belong to the tiles they stand in.
    world.insert_resource(PersistentCellWindow {
        cell_form_id: cell.form_id,
        center: center_grid,
        radius: full_3d_radius,
    });
    let cell_root = world.spawn();
    register_cell_root(world, cell_root);
    world.insert(cell_root, CellFormId(cell.form_id));
    stamp_exterior_cell(world, cell_root, wctx);
    super::super::note_player_entered_cell(world, cell.form_id, &wctx.record_index);
    // Placeholder only — `advance` reseeds the cursor at its top (#5483),
    // so the constructor value never reaches a stamp. Kept as
    // `next_entity_id()` rather than `cell_root` purely so an accidental
    // future stamp before the first advance claims nothing (an empty
    // range) instead of a foreign one.
    let first_entity = world.next_entity_id();
    Some(PersistentCellApplyJob {
        cell_root,
        cell_form_id: cell.form_id,
        authored_ref_count: cell.references.len(),
        stamp_cursor: first_entity,
        local_refs,
        remote_actor_refs,
        references: None,
        reference_entity_count: None,
        logical_stub_refs: Vec::new(),
        next_logical_stub: 0,
        logical_stub_scripts: 0,
        local_actor_3d: 0,
        local_actor_count: 0,
    })
}

/// #6076 — the grid window the persistent root spawned at worldspace entry
/// ([`begin_worldspace_persistent_cell`]), keyed by the persistent CELL it
/// partitioned. Every persistent ref inside it is the root's for the rest of
/// the worldspace session; one outside it belongs to the streamed tile it
/// stands in ([`persistent_refs_for_tile`]), which spawns it when it loads
/// and releases it when it unloads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PersistentCellWindow {
    pub(super) cell_form_id: u32,
    pub(super) center: (i32, i32),
    pub(super) radius: i32,
}

impl byroredux_core::ecs::Resource for PersistentCellWindow {}

/// #6076 — the persistent refs tile `(gx, gy)` owns: the ones standing in it,
/// when the tile lies outside the window the persistent root spawned. Before
/// this the partition ran once at entry and dropped every non-actor ref beyond
/// it (5,185 of WastelandNV's 6,069 from the Goodsprings arrival, 373 of them
/// teleport doors), so they never appeared as the player moved. Empty when no
/// persistent root has partitioned this worldspace's CELL yet: the root's own
/// job then owns the whole set, exactly as before.
pub(crate) fn persistent_refs_for_tile<'a>(
    wctx: &'a ExteriorWorldContext,
    world: &World,
    gx: i32,
    gy: i32,
) -> Vec<&'a byroredux_plugin::esm::cell::PlacedRef> {
    let Some((_, cell)) = resolve_persistent_cell(&wctx.record_index.cells, &wctx.worldspace_key)
    else {
        return Vec::new();
    };
    let Some(window) = world
        .try_resource::<PersistentCellWindow>()
        .map(|window| *window)
        .filter(|window| window.cell_form_id == cell.form_id)
    else {
        return Vec::new();
    };
    tile_persistent_refs(&cell.references, window, gx, gy)
}

pub(super) fn tile_persistent_refs(
    references: &[byroredux_plugin::esm::cell::PlacedRef],
    window: PersistentCellWindow,
    gx: i32,
    gy: i32,
) -> Vec<&byroredux_plugin::esm::cell::PlacedRef> {
    // The window is whole tiles, so a tile is wholly in or out of it.
    if persistent_tile_is_local((gx, gy), window.center, window.radius) {
        return Vec::new();
    }
    references
        .iter()
        .filter(|placed| persistent_tile_of(placed.position) == (gx, gy))
        .collect()
}

/// #6076 — promote: a persistent actor its tile is about to spawn for real
/// stands as a logical stub (identity, transform, scripts) while the tile is
/// out of range. Park the stub's state and despawn it first, so the real
/// actor takes over its identity and script state instead of running beside
/// it. The stubs carry no mesh or collider, so a plain despawn reclaims them.
pub(super) fn promote_persistent_actor_stubs(
    world: &mut World,
    wctx: &ExteriorWorldContext,
    refs: &[byroredux_plugin::esm::cell::PlacedRef],
) {
    let actors: HashSet<u32> = refs
        .iter()
        .filter(|placed| wctx.record_index.is_actor(placed.base_form_id))
        .map(|placed| placed.form_id)
        .collect();
    if actors.is_empty() {
        return;
    }
    let stubs: Vec<EntityId> = world
        .query::<byroredux_scripting::SceneAliasCandidate>()
        .map(|query| {
            query
                .iter()
                .filter(|(_, candidate)| actors.contains(&candidate.reference_form_id))
                .map(|(entity, _)| entity)
                .collect()
        })
        .unwrap_or_default();
    if stubs.is_empty() {
        return;
    }
    super::super::reference_state::capture(world, &stubs);
    world.despawn_batch(stubs);
    byroredux_scripting::mark_scene_actor_bindings_dirty(world);
}

/// #6076 — demote: give every persistent actor of `tiles` that has no entity
/// left (its tile unloaded or its apply was cancelled) its logical stub back,
/// owned by `persistent_root` like the stubs the persistent job made, so
/// quest aliases keep a rankable identity and its scripts keep running.
/// Idempotent: an actor something already carries is left alone.
pub(crate) fn ensure_persistent_actor_stubs(
    world: &mut World,
    wctx: &ExteriorWorldContext,
    tiles: &[(i32, i32)],
    persistent_root: Option<EntityId>,
) {
    let Some(persistent_root) = persistent_root else {
        return;
    };
    let actors: Vec<byroredux_plugin::esm::cell::PlacedRef> = tiles
        .iter()
        .flat_map(|&(gx, gy)| persistent_refs_for_tile(wctx, world, gx, gy))
        .filter(|placed| wctx.record_index.is_actor(placed.base_form_id))
        .cloned()
        .collect();
    if actors.is_empty() {
        return;
    }
    let carried: HashSet<u32> = world
        .query::<byroredux_scripting::SceneAliasCandidate>()
        .map(|query| query.iter().map(|(_, c)| c.reference_form_id).collect())
        .unwrap_or_default();
    let first = world.next_entity_id();
    let mut spawned = 0;
    for placed in actors.iter().filter(|placed| !carried.contains(&placed.form_id)) {
        super::super::references::spawn_logical_quest_reference_with_scripts(
            world,
            placed,
            &wctx.load_order,
            &wctx.record_index,
            super::super::transition::position_zup_to_yup(placed.position),
            super::super::transition::rotation_zup_to_yup_quat(placed.rotation),
            placed.scale,
        );
        spawned += 1;
    }
    stamp_cell_root_range(world, persistent_root, first, world.next_entity_id());
    if spawned > 0 {
        byroredux_scripting::mark_scene_actor_bindings_dirty(world);
        log::debug!("persistent CELL: {spawned} actor(s) back to logical stubs after their tile left");
    }
}

pub(super) fn persistent_tile_of(position_zup: [f32; 3]) -> (i32, i32) {
    (
        (position_zup[0] / EXTERIOR_CELL_UNITS).floor() as i32,
        (position_zup[1] / EXTERIOR_CELL_UNITS).floor() as i32,
    )
}

pub(super) fn persistent_tile_is_local(tile: (i32, i32), center_grid: (i32, i32), radius: i32) -> bool {
    (tile.0 - center_grid.0).abs().max((tile.1 - center_grid.1).abs()) <= radius.max(0)
}

pub(super) fn persistent_position_is_local(
    position_zup: [f32; 3],
    center_grid: (i32, i32),
    radius: i32,
) -> bool {
    persistent_tile_is_local(persistent_tile_of(position_zup), center_grid, radius)
}
