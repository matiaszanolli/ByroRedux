//! Per-cell reference loading: walk PlacedRefs, expand PKIN/SCOL
//! containers, parse NIFs/SPTs through the registry cache, and dispatch
//! to `spawn_placed_instances` for actual entity creation.
//!
//! The bulk of cell load time lives here — parsing NIFs (cache miss
//! path), expanding container placements, resolving base records,
//! and committing the per-cell NifImportRegistry deltas.

use byroredux_core::ecs::components::water::{WaterCurrentVolume, WaterFlow, WaterVolume};
use byroredux_core::ecs::components::FormIdComponent;
use byroredux_core::ecs::{EntityId, GlobalTransform, LightSource, Transform, World};
use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::esm;
use byroredux_renderer::VulkanContext;
use std::collections::HashMap;
use std::sync::Arc;

use crate::asset_provider::{MaterialProvider, TextureProvider};
use crate::components::VisibleWhenDistant;
use crate::npc_spawn::{NpcSpawnJob, NpcSpawnProgress};

use super::euler::euler_zup_to_quat_yup_refr;
use super::load_order::{self, plugin_for_form_id, LoadOrder};
use super::nif_import_registry::{canonical_model_path_key, CachedNifImport, NifImportRegistry};
use super::refr::{
    build_refr_texture_overlay, expand_pkin_placements, expand_scol_placements, RefrTextureOverlay,
};
use super::spawn::{light_radius_or_default, spawn_placed_instances};
use super::FrameTimeBudget;

mod attach;
mod import;

use attach::{attach_container_inventory, attach_script_for_refr, trigger_volume_from_primitive};
// Re-exported (not just `use`d) so the sibling `spawn` module can share this
// helper instead of duplicating its body (D22-3 / #2121).
pub(crate) use attach::attach_light_flicker_if_needed;
pub(super) use import::parse_and_import_nif_pub;
// Consumed only by the sibling `attach_points_spawn_tests` (#[cfg(test)]);
// gate the re-export so it isn't an unused import in the non-test build.
#[cfg(test)]
pub(crate) use attach::{attach_points_component, child_attach_connections_component};
use import::{parse_and_import_nif, parse_and_import_spt};

pub(super) struct RefLoadResult {
    pub(super) entity_count: usize,
    /// The cell's chosen spawn point — the first door's own placement if the
    /// cell has one (a guaranteed walkable threshold), else the bounding-box
    /// centroid of every placed REFR, else world origin for an empty cell.
    /// See the `door_pos` local in [`load_references`] for the precedence
    /// rationale.
    pub(super) center: Vec3,
}

/// Owned continuation for the placed-reference phase of a cell load.
///
/// Every field that used to live on `load_references`' stack lives here so
/// exterior streaming can return to the main loop between REFRs without
/// duplicating the interior/reference pipeline.
/// One SCOL/PKIN REFR's expanded child placements plus the shared texture
/// overlay that applies to all of them: `(child_form_id, position,
/// rotation, scale)` per child. Named so `ReferenceLoadJob`'s resume cache
/// doesn't trip `clippy::type_complexity` (#2409 / TD1-006).
pub(super) type SynthChildPlan = (Vec<(u32, Vec3, Quat, f32)>, Option<RefrTextureOverlay>);

pub(super) struct ReferenceLoadJob {
    next_ref: usize,
    /// Next synthetic child inside the current SCOL/PKIN-expanded REFR.
    next_synth: usize,
    /// Cached SCOL/PKIN child-placement expansion + shared texture overlay
    /// for the REFR currently at `next_ref`. `None` until first computed
    /// for this REFR, and cleared again once `next_ref` advances. A budget
    /// yield partway through a REFR's `synth_refs` resumes from this cache
    /// instead of re-walking `scol.parts`/`.placements` and recomposing
    /// every child transform from scratch (#2277 / PERF-D7-03).
    current_ref_synth: Option<SynthChildPlan>,
    /// Sub-REFR continuation for an actor whose NIF bundle spans frames.
    active_npc: Option<NpcSpawnJob>,
    cache_hits_at_entry: u64,
    cache_misses_at_entry: u64,
    cache_size_at_entry: usize,
    door_pos: Option<Vec3>,
    enable_skipped: u32,
    absorbed_skipped: u32,
    absorbed_interactive_retained: u32,
    npc_pending: u32,
    npc_pending_sample: Vec<u32>,
    idle_pool: Vec<u32>,
    accum: RefLoadAccum,
}

pub(super) enum ReferenceLoadProgress {
    Pending(Box<ReferenceLoadJob>),
    Complete(RefLoadResult),
}

impl ReferenceLoadJob {
    /// Release clip handles registered by cache-miss REFRs that never reached
    /// the end-of-cell cache commit because streaming cancelled the cell.
    pub(super) fn cancel(self, world: &World) {
        if self.accum.pending_clip_handles.is_empty() {
            return;
        }
        let mut clip_reg = world.resource_mut::<byroredux_core::animation::AnimationClipRegistry>();
        for handle in self.accum.pending_clip_handles.into_values() {
            clip_reg.release(handle);
        }
    }
}

/// #1495 / REN2-10 — RT absolute-space f32 precision ceiling, in world
/// units. TLAS instance transforms, skinned BLAS vertices, and the ray
/// origins reconstructed in `triangle.frag` all live in ABSOLUTE world
/// space (the TLAS is absolute by design; the raster path is rebased to
/// render-origin-relative via `#markarth-precision`, but RT is not). At
/// `|world| ≈ 2^20 = 1_048_576` the f32 ULP is `2^-3 = 0.125 u`, which
/// reaches the upper RT bias/tMin margin (~0.15 u) — shadow / reflection
/// rays start self-intersecting or leaking. Headroom thins earlier
/// (~0.5 M, where the 0.0156 u ULP loses its 2–3× cushion over the tight
/// 0.05 u margin), so this ceiling is the hard upper bound, not the
/// onset. Vanilla worldspaces top out far below it (Skyrim Tamriel
/// ≈ ±233 k), so a cell past it is a future mega-worldspace that would
/// silently degrade RT. See docs/engine/shader-pipeline.md "Coordinate
/// Spaces & Precision".
pub(crate) const RT_ABSOLUTE_PRECISION_CEILING: f32 = 1_048_576.0; // 2^20

/// Whether a precombined CSG bake may replace this REFR entirely.
///
/// Only render-only static geometry is safe to erase from the ECS. FURN,
/// CONT, ACTI, TERM, and MSTT records carry interaction, inventory, scripts,
/// furniture markers, or runtime motion that the flattened CSG cannot
/// preserve. XPRI can list those records (Switchboard has 141 of them), so
/// treating every XPRI entry as disposable drops both their visuals and their
/// gameplay identity. SCOL is also render-only once expanded into its baked
/// static geometry.
fn precombine_can_replace_record(record_type: Option<byroredux_plugin::RecordType>) -> bool {
    matches!(
        record_type,
        Some(byroredux_plugin::RecordType::STAT | byroredux_plugin::RecordType::SCOL)
    )
}

#[cfg(test)]
mod precombine_absorption_tests {
    use super::precombine_can_replace_record;
    use byroredux_plugin::RecordType;

    #[test]
    fn only_render_only_records_are_fully_replaced_by_csg() {
        assert!(precombine_can_replace_record(Some(RecordType::STAT)));
        assert!(precombine_can_replace_record(Some(RecordType::SCOL)));

        for interactive in [
            RecordType::MSTT,
            RecordType::FURN,
            RecordType::CONT,
            RecordType::ACTI,
            RecordType::TERM,
            RecordType::DOOR,
        ] {
            assert!(
                !precombine_can_replace_record(Some(interactive)),
                "{} must retain its individual runtime identity",
                interactive.as_str(),
            );
        }
        assert!(!precombine_can_replace_record(None));
    }
}

/// Returns the cell's largest absolute world-coordinate magnitude when
/// it reaches [`RT_ABSOLUTE_PRECISION_CEILING`], else `None`. `None` for
/// an empty cell (bounds still `±INF`). Pure helper so the cell-load
/// guard is unit-testable without the full loader. See #1495.
fn worldspace_extent_over_rt_ceiling(bounds_min: Vec3, bounds_max: Vec3) -> Option<f32> {
    if !bounds_min.x.is_finite() {
        return None; // empty cell — no placements accumulated into bounds
    }
    let extent = bounds_min.abs().max(bounds_max.abs()).max_element();
    (extent >= RT_ABSOLUTE_PRECISION_CEILING).then_some(extent)
}

/// Shared reference-loading pipeline: resolve base forms, load NIFs, spawn entities.
///
/// `load_order` holds the global plugin basenames (lowercase) — used
/// only to enrich the loud-fail diagnostic when a REFR's
/// `base_form_id` doesn't resolve. Pass `&[]` for legacy single-plugin
/// callers; the cell loader entry points (`load_cell_with_masters`,
/// `load_exterior_cells_with_masters`) thread the real load order.
/// See M46.0 / #561.
#[tracing::instrument(
    name = "load_references",
    skip_all,
    fields(ref_count = refs.len(), actor_count = record_index.npcs.len() + record_index.creatures.len(), race_count = races.len(), game = ?game, label = label),
)]
#[allow(clippy::too_many_arguments)]
pub(super) fn load_references(
    refs: &[esm::cell::PlacedRef],
    index: &esm::cell::EsmCellIndex,
    record_index: &byroredux_plugin::esm::records::EsmIndex,
    races: &HashMap<u32, byroredux_plugin::esm::records::RaceRecord>,
    game: byroredux_plugin::esm::reader::GameKind,
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
    label: &str,
    load_order: &LoadOrder,
    // #1188 — FO4+ PreCombined absorbed REFR form IDs. Skip placement
    // for any REFR in this set: the CK's bake tool already folded
    // its geometry into a `meshes\precombined\<cell>_<hash>_oc.nif`
    // file that the precombined spawn step (later in this load) will
    // bring in. Spawning here would produce double geometry +
    // z-fighting on every wall / floor / ceiling.
    absorbed_refs: &std::collections::HashSet<u32>,
) -> RefLoadResult {
    let mut budget = FrameTimeBudget::unlimited();
    match load_references_budgeted(
        refs,
        index,
        record_index,
        races,
        game,
        world,
        ctx,
        tex_provider,
        mat_provider,
        label,
        load_order,
        absorbed_refs,
        None,
        &mut budget,
    ) {
        ReferenceLoadProgress::Complete(result) => result,
        ReferenceLoadProgress::Pending(_) => {
            unreachable!("an unlimited reference-load budget cannot yield")
        }
    }
}

/// Advance the shared reference loader until the cooperative deadline expires.
///
/// A normal synthetic placement is the atomic progress unit. SCOL/PKIN
/// expansions can yield between children, and NPC children can additionally
/// yield between their skeleton/body/head/armor NIFs. The first unit is always
/// admitted by [`FrameTimeBudget`], preserving deterministic ordering and
/// forward progress when an individual NIF itself exceeds the target.
#[allow(clippy::too_many_arguments)]
pub(super) fn load_references_budgeted(
    refs: &[esm::cell::PlacedRef],
    index: &esm::cell::EsmCellIndex,
    record_index: &byroredux_plugin::esm::records::EsmIndex,
    races: &HashMap<u32, byroredux_plugin::esm::records::RaceRecord>,
    game: byroredux_plugin::esm::reader::GameKind,
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    mut mat_provider: Option<&mut MaterialProvider>,
    label: &str,
    load_order: &LoadOrder,
    absorbed_refs: &std::collections::HashSet<u32>,
    job: Option<Box<ReferenceLoadJob>>,
    budget: &mut FrameTimeBudget,
) -> ReferenceLoadProgress {
    let mut job = if let Some(job) = job {
        job
    } else {
        // CHARAL: build the per-game derived-stat ruleset once (idempotent across
        // cells) so `GetActorValue` can compute actor-general derived stats (Carry
        // Weight, Melee Damage, …) for actors that don't carry the value directly.
        if world
            .try_resource::<byroredux_core::character::CharacterRuleset>()
            .is_none()
        {
            if let Some(rs) = crate::npc_spawn::build_character_ruleset(record_index) {
                world.insert_resource(rs);
            }
        }
        // #3092 — same idempotent-once construction for the Melee Damage
        // AVIF id `byroredux/src/combat.rs`'s `attack_damage` needs to look
        // the CharacterRuleset row above back up at combat time. Absent on
        // any game that authors no `MeleeDamage` AVIF (FO4, TES) — combat
        // just sees no config and swings at the flat baseline, same as
        // before this fix.
        if world
            .try_resource::<byroredux_core::character::MeleeDamageConfig>()
            .is_none()
        {
            if let Some(config) = crate::npc_spawn::build_melee_damage_config(record_index) {
                world.insert_resource(config);
            }
        }
        // Process-lifetime cache of parsed-and-imported NIF scene data
        // (`NifImportRegistry`, #381). Each unique mesh is parsed exactly
        // once across the entire process — subsequent placements of the
        // same model in this cell *and* later cells reuse the shared
        // `Arc` and only pay the per-reference spawn cost (vertex upload,
        // texture resolve, entity insertion). A `None` entry records a
        // mesh that failed to parse — we skip subsequent placements of
        // the same model silently. Per-cell hit/miss accounting (the
        // numbers logged at end-of-cell) is computed against the lifetime
        // counters by snapshotting them at entry.
        let (cache_hits_at_entry, cache_misses_at_entry, cache_size_at_entry) = {
            let reg = world.resource::<NifImportRegistry>();
            (reg.core.hits(), reg.core.misses(), reg.len())
        };
        // First door REFR's own placement in THIS cell (not its XTEL
        // destination) — a strictly better spawn-point candidate than the raw
        // bounding-box centroid below. A door is always placed on a walkable
        // threshold; the centroid of every placed REFR (statics, NPCs, invisible
        // trigger volumes, far-flung markers) has no such guarantee and can land
        // inside a wall, a stairwell void, or genuinely outside the interior
        // shell for L-shaped/multi-wing cells — the reported "spawns at random
        // points, sometimes outside the interior" bug. See the `center` doc
        // comment on `RefLoadResult`/`CellLoadResult`.
        let door_pos: Option<Vec3> = None;
        let enable_skipped = 0u32;
        // #1188 — count REFRs skipped because the CK absorbed them into a
        // precombined `_oc.nif`. Surfaced in the end-of-cell summary so an
        // operator can spot a missing precombined-spawn step (would manifest
        // as "absorbed=N but precombined_spawned=0" pair below).
        let absorbed_skipped = 0u32;
        let absorbed_interactive_retained = 0u32;
        // `npc_pending` was the Phase 0/2 telemetry for pre-baked-FaceGen
        // games waiting on Phase 4's spawn path — kept (unused after
        // Phase 4 wired) so the cell summary's "0 ACHR refs ... pending"
        // line stays a coherent zero rather than disappearing entirely.
        // M41.0 lands every supported game on a real spawn function;
        // if a future game variant doesn't satisfy either predicate,
        // these fall back to the original telemetry shape.
        let npc_pending: u32 = 0;
        let npc_pending_sample: Vec<u32> = Vec::with_capacity(8);

        // M41.0 Phase 2 + M41.5 Phase A — resolve the shared per-cell idle
        // pool once before the REFR loop; it is threaded through every
        // `spawn_npc_entity` call, where each NPC picks + phase-desyncs its
        // own handle. `load_idle_pool` is path-keyed memoised (#790), so
        // re-entry across cell loads is a HashMap hit — neither the BSA
        // extract nor `AnimationClipRegistry::add` runs a second time for
        // the same `kf_path`. Returns an empty pool when the game is on the
        // Havok-animation track (Skyrim+/FO4+) or the KF isn't archived —
        // those NPCs just spawn without an animation player. Gender variation
        // is collapsed: FNV vanilla ships only `_male\idle.kf` and uses it
        // for both genders. The `Gender` argument was dropped from these
        // resolvers in #1117 / TD8-018; re-introduce it when a game variant
        // actually ships separate clips.
        let idle_pool = if game.has_kf_animations() {
            crate::npc_spawn::load_idle_pool(world, tex_provider, game)
        } else {
            Vec::new()
        };

        // M42.1 — resolve the sit-enter clip (handle, duration) once per cell
        // (archive provider available here; `sandbox_seat_system` has none) into
        // the `SandboxSitClip` resource. `None` for Skyrim+/Havok games → those
        // actors are not seated.
        let sit_clip = crate::npc_spawn::load_sit_clip(world, tex_provider, game);
        if let Some(mut r) = world.try_resource_mut::<crate::components::SandboxSitClip>() {
            r.0 = sit_clip;
        }

        // #2147 / ECS-2507-01 — prune reservations whose furniture is gone,
        // rather than clearing the whole set.
        //
        // This ran once per cell, and the exterior grid path calls
        // `load_references` per `(gx, gy)` — 49 wholesale clears on `--radius 3`,
        // and again for every cell streamed in at a boundary crossing while
        // previously-loaded cells and their seated actors are still resident.
        // `Seated` is a one-shot terminal marker, so an actor that already sat
        // never re-asserts its claim: the clear released seats that were still
        // physically occupied, and the next unseated actor within
        // `SEAT_SEARCH_RADIUS` could claim the same `(furniture, marker)`.
        //
        // The old comment justified the clear with "entity ids reset on unload".
        // They don't — `World::despawn` documents that IDs are never reclaimed
        // (#372) and `next_entity` only grows, so a stale entry can never alias a
        // *new* furniture entity. The clear was only bounding set growth, at the
        // cost of the per-marker exclusivity the resource exists to provide.
        //
        prune_seat_reservations(world);

        Box::new(ReferenceLoadJob {
            next_ref: 0,
            next_synth: 0,
            current_ref_synth: None,
            active_npc: None,
            cache_hits_at_entry,
            cache_misses_at_entry,
            cache_size_at_entry,
            door_pos,
            enable_skipped,
            absorbed_skipped,
            absorbed_interactive_retained,
            npc_pending,
            npc_pending_sample,
            idle_pool,
            accum: RefLoadAccum::new(),
        })
    };

    // Per-call NIF-cache accumulators (this_call_hits / misses / pending_new
    // / pending_hits / pending_clip_handles) live on `accum` and are committed
    // to `NifImportRegistry` in a single `resource_mut` borrow after the loop
    // rather than a write lock per REFR (#523 / #635 / #544). See the
    // `RefLoadAccum` field docs for each one's role.
    let cell = CellLoadCtx {
        index,
        record_index,
        game,
        tex_provider,
        load_order,
    };
    while job.next_ref < refs.len() {
        if budget.should_yield() {
            return ReferenceLoadProgress::Pending(job);
        }
        let placed_ref = &refs[job.next_ref];
        // Skip REFRs whose XESP gating would keep them hidden under
        // the parents-assumed-enabled heuristic: inverted XESP children
        // are visible only when the parent is *disabled*, so under the
        // default they stay off. Non-inverted XESP children fall through
        // and render. See #471 (flipped #349's over-hiding predicate)
        // — long-term fix is a two-pass loader that reads the parent
        // REFR's own 0x0800 "initial disabled" flag.
        if let Some(ep) = placed_ref.enable_parent {
            if ep.default_disabled() {
                job.enable_skipped += 1;
                job.next_ref += 1;
                budget.complete_unit();
                continue;
            }
        }

        // #1188 — FO4+ PreCombined absorption skip. The bake tool
        // already folded this REFR's geometry into one of the
        // `meshes\precombined\<cell>_<hash>_oc.nif` files; the
        // precombined-spawn pass will bring those in as single
        // entities. Filtering here prevents double geometry.
        if absorbed_refs.contains(&placed_ref.form_id) {
            let record_type = index
                .statics
                .get(&placed_ref.base_form_id)
                .map(|object| object.record_type);
            if precombine_can_replace_record(record_type) {
                job.absorbed_skipped += 1;
                job.next_ref += 1;
                budget.complete_unit();
                continue;
            }
            job.absorbed_interactive_retained += 1;
        }

        // Convert the outer REFR's placement (Z-up Bethesda → Y-up
        // renderer). For normal REFRs this is the spawn transform; for
        // SCOL REFRs it's the parent transform the child placements
        // compose against. #1617 — route through the coord SoT
        // (`zup_to_yup_pos`) rather than an inline `(x, z, -y)` so a future
        // change to the canonical swap can't silently skip this hot REFR
        // placement path. Bit-identical to the old inline form.
        let outer_pos = Vec3::from_array(byroredux_core::math::coord::zup_to_yup_pos(
            placed_ref.position,
        ));
        let outer_rot = euler_zup_to_quat_yup_refr(
            placed_ref.rotation[0],
            placed_ref.rotation[1],
            placed_ref.rotation[2],
        );
        let outer_scale = placed_ref.scale;

        // A REFR carrying an XTEL payload is a door — remember the FIRST
        // one's own placement as the spawn-point candidate (see the
        // `door_pos` declaration above). Deliberately the first in load
        // order, not "the" entrance — this loader has no notion of which
        // door the player narratively used, so any door in this cell is a
        // guaranteed-walkable improvement over the bounding-box centroid.
        if job.door_pos.is_none() && placed_ref.teleport.is_some() {
            job.door_pos = Some(outer_pos);
        }

        // #2277 / PERF-D7-03 — a budget yield partway through a large
        // SCOL/PKIN's `synth_refs` stashes the already-expanded list (and
        // its shared overlay) on `job.current_ref_synth`; reuse it here
        // instead of re-expanding from scratch on the resumed tick.
        let (synth_refs, refr_overlay) = match job.current_ref_synth.take() {
            Some(cached) => cached,
            None => {
                // Build per-REFR texture overlay once. Shared across every
                // synthetic SCOL child — FO4 REFRs that overlay textures at
                // the SCOL level apply the same swap to every child
                // placement. #584.
                let refr_overlay = {
                    let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
                    build_refr_texture_overlay(
                        placed_ref,
                        index,
                        mat_provider.as_deref_mut(),
                        &mut pool,
                    )
                };

                // Compose REFR expansion from composite-record helpers:
                //   1. PKIN (#589) — Pack-In bundle fans out to one synth per
                //      `CNAM` content at the outer transform.
                //   2. SCOL (#585) — Static Collection fans out to one synth
                //      per `ONAM/DATA` placement when no cached `CM*.NIF`.
                //   3. Default — single synth at the outer transform.
                //
                // First expander that fires wins; `expand_scol_placements`
                // already returns the single-entry default when the base
                // form isn't a SCOL, so the chain closes cleanly.
                let synth_refs = expand_pkin_placements(
                    placed_ref.base_form_id,
                    outer_pos,
                    outer_rot,
                    outer_scale,
                    index,
                )
                .unwrap_or_else(|| {
                    expand_scol_placements(
                        placed_ref.base_form_id,
                        outer_pos,
                        outer_rot,
                        outer_scale,
                        index,
                    )
                });
                (synth_refs, refr_overlay)
            }
        };

        // #2026 / SCR-D7-NEW2-01 — the outer REFR's own VMAD
        // (`placed_ref.script_instance`) is a property of that single
        // REFR, not of each synthetic child a SCOL/PKIN expansion fans
        // it out into. Attach it only to the first child; the remaining
        // N-1 pass `None` so a VMAD-scripted SCOL/PKIN's behavior
        // (including the `OnCellLoadEvent` that follows a successful
        // attach) instantiates once per REFR, not once per decorative
        // piece. Mirrors the texture-overlay sharing above (#584) in
        // spirit — one REFR-level property, applied once — but VMAD
        // attachment is behavioral, not visual, so it goes to a single
        // child instead of being broadcast to all of them.
        let synth_count = synth_refs.len();
        let mut synth_idx = job.next_synth;
        while synth_idx < synth_count {
            if budget.should_yield() {
                job.current_ref_synth = Some((synth_refs, refr_overlay));
                return ReferenceLoadProgress::Pending(job);
            }
            let (child_form_id, ref_pos, ref_rot, ref_scale) = synth_refs[synth_idx];

            // EXAL sub-REFR continuation: an actor is a bundle of top-level
            // NIFs, not one indivisible placement.  Keep its skeleton map and
            // placement root alive across frames, advancing one body/head/
            // armor part at a time.  Synchronous interiors drive this exact
            // job with an unlimited budget through `load_references`.
            // #2567 (OBL-D3-01) — `record_index.actor` covers `NPC_` **and**
            // `CREA`. This site used to read the `npcs` map alone, so every
            // placed creature (Oblivion `ACRE`, and `ACHR`→`CREA` from FO3 on)
            // missed the actor pipeline entirely and fell through to the
            // static-mesh path below — which rendered the creature's MODL, i.e.
            // its bare skeleton, and never animated it.
            if let Some(npc) = record_index.actor(child_form_id) {
                if job.active_npc.is_none() {
                    job.accum.bounds_min = job.accum.bounds_min.min(ref_pos);
                    job.accum.bounds_max = job.accum.bounds_max.max(ref_pos);
                    job.active_npc = if game.has_runtime_facegen_recipe() {
                        Some(NpcSpawnJob::runtime(
                            npc,
                            races.get(&npc.race_form_id),
                            game,
                            ref_pos,
                            ref_rot,
                            ref_scale,
                        ))
                    } else if game.uses_prebaked_facegen() {
                        let plugin =
                            load_order::plugin_for_form_id(child_form_id, load_order).unwrap_or("");
                        Some(NpcSpawnJob::prebaked(
                            npc, game, plugin, ref_pos, ref_rot, ref_scale,
                        ))
                    } else {
                        None
                    };
                }

                let Some(active_npc) = job.active_npc.as_mut() else {
                    job.next_synth = synth_idx + 1;
                    budget.complete_unit();
                    synth_idx += 1;
                    continue;
                };
                match active_npc.advance(
                    world,
                    ctx,
                    tex_provider,
                    mat_provider.as_deref_mut(),
                    &job.idle_pool,
                    record_index,
                    budget,
                ) {
                    NpcSpawnProgress::Pending => {
                        job.current_ref_synth = Some((synth_refs, refr_overlay));
                        return ReferenceLoadProgress::Pending(job);
                    }
                    NpcSpawnProgress::Complete(result) => {
                        job.accum.npc_spawn_wall += result.work_wall;
                        if let Some(root) = result.root {
                            // Actor jobs build their own placement root rather
                            // than routing through `spawn_placed_instances`, so
                            // stamp the canonical ACHR identity and the three
                            // fields Skyrim quest-alias resolution consumes.
                            if synth_idx == 0 {
                                stamp_quest_reference(world, root, placed_ref, load_order);
                                if let Some(mut identities) =
                                    world.query_mut::<byroredux_scripting::SceneAliasCandidate>()
                                {
                                    if let Some(identity) = identities.get_mut(root) {
                                        // A placed ACHR may reference an LVLN. Papyrus
                                        // GetActorBase observes the resolved NPC_, which is
                                        // the synthesized child selected above.
                                        identity.base_form_id = child_form_id;
                                    }
                                }
                            }
                            // #2662 — actor jobs bypass `spawn_synth_child`
                            // entirely, so without this they never reached
                            // `attach_script_for_refr`: the `npcs` arm of
                            // `base_record_script_instance` was unreachable
                            // from the live attach path and the placed
                            // `ACHR`'s own VMAD was never consumed. Use the
                            // same per-synth-child gate the static path uses,
                            // so the REFR-own VMAD binds to child 0 only and
                            // the additive REFR-then-base merge is identical.
                            // #3016 — the call itself stays ungated: this
                            // already matched the shared policy documented
                            // on `attach_quest_reference_script`.
                            attach_quest_reference_script(
                                world,
                                root,
                                child_form_id,
                                record_index,
                                refr_script_instance_for_synth_child(
                                    synth_idx,
                                    placed_ref.script_instance.as_ref(),
                                ),
                                &mut job.accum,
                            );
                            job.accum.npc_spawned += 1;
                            if job.accum.npc_spawned_sample.len() < 8
                                && !job.accum.npc_spawned_sample.contains(&child_form_id)
                            {
                                job.accum.npc_spawned_sample.push(child_form_id);
                            }
                            job.accum.entity_count += 1;
                        }
                        job.active_npc = None;
                        job.next_synth = synth_idx + 1;
                    }
                }
                synth_idx += 1;
                continue;
            }

            let refr_script_instance = refr_script_instance_for_synth_child(
                synth_idx,
                placed_ref.script_instance.as_ref(),
            );
            spawn_synth_child(
                &mut job.accum,
                world,
                ctx,
                &cell,
                mat_provider.as_deref_mut(),
                placed_ref,
                &refr_overlay,
                child_form_id,
                ref_pos,
                ref_rot,
                ref_scale,
                refr_script_instance,
                synth_idx == 0,
            );
            job.next_synth = synth_idx + 1;
            budget.complete_unit();
            synth_idx += 1;
        }
        job.next_ref += 1;
        job.next_synth = 0;
        job.current_ref_synth = None;
        if synth_count == 0 {
            budget.complete_unit();
        }
    }
    complete_reference_load(job, game, world, ctx, label, load_order)
}

/// Minimum number of queued DDS textures worth a synchronous submit/fence
/// round trip at a cooperative yield. Phase and cell completion still force a
/// flush regardless of count. FO4 boundary traces showed 145 post-bootstrap
/// flushes for only ~550 new textures (typically 3–17 per batch); accumulating
/// up to this threshold preserves bounded staging memory while avoiding those
/// tiny serial submissions.
const YIELDED_TEXTURE_UPLOAD_BATCH_MIN: usize = 64;

fn should_flush_pending_cell_textures(pending_uploads: usize, force: bool) -> bool {
    pending_uploads > 0 && (force || pending_uploads >= YIELDED_TEXTURE_UPLOAD_BATCH_MIN)
}

/// Flush textures accumulated by a completed reference phase or cell.
pub(super) fn flush_pending_cell_textures(ctx: &mut VulkanContext) {
    flush_pending_cell_textures_inner(ctx, true);
}

/// Flush a yielded slice only after enough DDS work has accumulated to
/// amortize one command-buffer submit and fence wait.
pub(super) fn flush_pending_cell_textures_on_yield(ctx: &mut VulkanContext) {
    flush_pending_cell_textures_inner(ctx, false);
}

fn flush_pending_cell_textures_inner(ctx: &mut VulkanContext, force: bool) {
    let pending_uploads = ctx.texture_registry.pending_dds_upload_count();
    if !should_flush_pending_cell_textures(pending_uploads, force) {
        return;
    }
    let started = std::time::Instant::now();
    match ctx.texture_registry.flush_pending_uploads(
        &ctx.device,
        ctx.allocator
            .as_ref()
            .expect("VulkanContext.allocator initialised before cell load"),
        &ctx.graphics_queue,
        ctx.transfer_pool,
        &ctx.transfer_fence,
    ) {
        Ok(n) => log::info!(
            "  Cell texture upload batch: {n}/{pending_uploads} DDS textures uploaded in {:.2} ms",
            started.elapsed().as_secs_f64() * 1000.0,
        ),
        Err(e) => log::warn!("Cell texture upload batch failed ({pending_uploads} pending): {e}"),
    }
}

#[cfg(test)]
mod texture_flush_policy_tests {
    use super::{should_flush_pending_cell_textures, YIELDED_TEXTURE_UPLOAD_BATCH_MIN};

    #[test]
    fn yielded_slices_accumulate_until_the_batch_threshold() {
        assert!(!should_flush_pending_cell_textures(0, false));
        assert!(!should_flush_pending_cell_textures(
            YIELDED_TEXTURE_UPLOAD_BATCH_MIN - 1,
            false,
        ));
        assert!(should_flush_pending_cell_textures(
            YIELDED_TEXTURE_UPLOAD_BATCH_MIN,
            false,
        ));
    }

    #[test]
    fn completion_forces_any_nonempty_texture_batch() {
        assert!(!should_flush_pending_cell_textures(0, true));
        assert!(should_flush_pending_cell_textures(1, true));
    }
}

/// Mutable accumulators threaded through the per-REFR spawn loop in
/// [`load_references`]. Bundled so the per-record-kind dispatch could be
/// split into [`spawn_synth_child`] without a 20-argument signature
/// (#2058). `door_pos` / `enable_skipped` / `absorbed_skipped` /
/// `npc_pending*` stay as loop-locals — they are set in the outer REFR
/// loop, never inside the per-child dispatch.
struct RefLoadAccum {
    /// Mesh-bearing entities spawned (return value + summary line).
    entity_count: usize,
    /// Running world-space AABB over every placed REFR; seeds the spawn point.
    bounds_min: Vec3,
    bounds_max: Vec3,
    /// REFR base forms missing from `index.statics` (+ bounded FormID sample).
    stat_miss: u32,
    stat_hit: u32,
    /// NIF/SPT files not found in the BSA archives (+ bounded path sample).
    nif_not_found: u32,
    nif_not_found_sample: Vec<String>,
    stat_miss_sample: Vec<u32>,
    /// NPC actors spawned via either FaceGen dispatch path (+ sample + wall time).
    npc_spawned: u32,
    npc_spawned_sample: Vec<u32>,
    npc_spawn_wall: std::time::Duration,
    /// M47.2 script-attach + trigger-volume telemetry.
    scripts_recognized: u32,
    trigger_volumes: u32,
    /// #1359 — CONT REFRs that received an `Inventory` component.
    containers_attached: u32,
    /// FO4+/FO76/Starfield packed-Havok compatibility coverage, counted per
    /// placement and emitted once in the cell summary.
    packed_collision_fallbacks: u32,
    unresolved_packed_collision: u32,
    /// #523 per-call NIF-cache hit/miss tallies, merged after the loop.
    this_call_hits: u64,
    this_call_misses: u64,
    /// #523 per-call parse/hit shadows + #544 clip handles, committed after the loop.
    pending_new: HashMap<String, Option<Arc<CachedNifImport>>>,
    pending_hits: Vec<String>,
    pending_clip_handles: HashMap<String, u32>,
}

impl RefLoadAccum {
    fn new() -> Self {
        Self {
            entity_count: 0,
            bounds_min: Vec3::splat(f32::INFINITY),
            bounds_max: Vec3::splat(f32::NEG_INFINITY),
            stat_miss: 0,
            stat_hit: 0,
            nif_not_found: 0,
            nif_not_found_sample: Vec::with_capacity(5),
            stat_miss_sample: Vec::with_capacity(20),
            npc_spawned: 0,
            npc_spawned_sample: Vec::with_capacity(8),
            npc_spawn_wall: std::time::Duration::ZERO,
            scripts_recognized: 0,
            trigger_volumes: 0,
            containers_attached: 0,
            packed_collision_fallbacks: 0,
            unresolved_packed_collision: 0,
            this_call_hits: 0,
            this_call_misses: 0,
            pending_new: HashMap::new(),
            pending_hits: Vec::new(),
            pending_clip_handles: HashMap::new(),
        }
    }
}

/// Read-only per-cell context shared by every [`spawn_synth_child`] call.
/// Destructured verbatim at the top of the helper so the moved dispatch
/// body reads exactly as it did inline (#2058). All fields are `Copy`.
#[derive(Clone, Copy)]
struct CellLoadCtx<'a> {
    index: &'a esm::cell::EsmCellIndex,
    record_index: &'a byroredux_plugin::esm::records::EsmIndex,
    game: byroredux_plugin::esm::reader::GameKind,
    tex_provider: &'a TextureProvider,
    load_order: &'a LoadOrder,
}

/// #2147 / #2392 — drop seat reservations whose furniture or claiming actor
/// is gone, keeping claims only while the actor remains seated on that same
/// furniture.
///
/// Called once per `load_references`, i.e. once per cell. Extracted so the
/// cross-cell survival property is testable without the archive providers and
/// ESM index the full load path needs.
///
/// Snapshot `Furniture` and `Seated` before taking the resource write. This
/// matches `sandbox_seat_system`'s component-before-resource order, so the
/// pairs cannot form an ABBA cycle under the parallel scheduler.
fn prune_seat_reservations(world: &byroredux_core::ecs::World) {
    let live_furniture: std::collections::HashSet<byroredux_core::ecs::storage::EntityId> = world
        .query::<byroredux_core::ecs::components::Furniture>()
        .map(|q| q.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    let seated_claimants: std::collections::HashMap<
        byroredux_core::ecs::storage::EntityId,
        byroredux_core::ecs::storage::EntityId,
    > = world
        .query::<byroredux_core::ecs::components::Seated>()
        .map(|q| {
            q.iter()
                .map(|(actor, seated)| (actor, seated.furniture))
                .collect()
        })
        .unwrap_or_default();
    if let Some(mut r) = world.try_resource_mut::<crate::components::SeatReservations>() {
        r.0.retain(|(furniture, _), claimant| {
            live_furniture.contains(furniture) && seated_claimants.get(claimant) == Some(furniture)
        });
    }
}

// Completion tail + synthetic-child spawn path (#2409 / TD1-006).
mod complete;
mod synth_child;
use complete::complete_reference_load;
use synth_child::{
    attach_quest_reference_script, refr_script_instance_for_synth_child, spawn_synth_child,
    stamp_quest_reference,
};
// #2664 — the exterior persistent-cell loader spawns the same logical
// identity entity for a 3D-less persistent ACHR, so the one spawner is
// shared rather than copied.
pub(crate) use synth_child::spawn_logical_quest_reference;

// Tests live in sibling files by topic (#2409 / TD1-006).
#[cfg(test)]
mod attach_tests;
#[cfg(test)]
mod import_tests;
#[cfg(test)]
mod seat_reservation_tests;
#[cfg(test)]
mod source_pin_tests;
#[cfg(test)]
mod synth_child_tests;
#[cfg(test)]
mod trigger_volume_tests;
