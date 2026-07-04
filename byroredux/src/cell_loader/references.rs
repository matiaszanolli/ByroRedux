//! Per-cell reference loading: walk PlacedRefs, expand PKIN/SCOL
//! containers, parse NIFs/SPTs through the registry cache, and dispatch
//! to `spawn_placed_instances` for actual entity creation.
//!
//! The bulk of cell load time lives here — parsing NIFs (cache miss
//! path), expanding container placements, resolving base records,
//! and committing the per-cell NifImportRegistry deltas.

use byroredux_core::ecs::{
    BillboardMode, GlobalTransform, LightFlicker, LightSource, Transform, World,
    LIGHT_FLAG_FLICKER, LIGHT_FLAG_FLICKER_SLOW, LIGHT_FLAG_PULSE, LIGHT_FLAG_PULSE_SLOW,
};
use byroredux_core::form_id::{FormIdPair, LocalFormId, PluginId};
use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::esm;
use byroredux_renderer::VulkanContext;
use std::collections::HashMap;
use std::sync::Arc;

use crate::asset_provider::{merge_bgsm_into_mesh, MaterialProvider, TextureProvider};

use super::euler::euler_zup_to_quat_yup_refr;
use super::load_order::{self, plugin_for_form_id};
use super::nif_import_registry::{CachedNifImport, NifImportRegistry};
use super::refr::{build_refr_texture_overlay, expand_pkin_placements, expand_scol_placements};
use super::spawn::{light_radius_or_default, spawn_placed_instances};

pub(super) struct RefLoadResult {
    pub(super) entity_count: usize,
    /// The cell's chosen spawn point — the first door's own placement if the
    /// cell has one (a guaranteed walkable threshold), else the bounding-box
    /// centroid of every placed REFR, else world origin for an empty cell.
    /// See the `door_pos` local in [`load_references`] for the precedence
    /// rationale.
    pub(super) center: Vec3,
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
const RT_ABSOLUTE_PRECISION_CEILING: f32 = 1_048_576.0; // 2^20

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
    fields(ref_count = refs.len(), npc_count = npcs.len(), race_count = races.len(), game = ?game, label = label),
)]
#[allow(clippy::too_many_arguments)]
pub(super) fn load_references(
    refs: &[esm::cell::PlacedRef],
    index: &esm::cell::EsmCellIndex,
    record_index: &byroredux_plugin::esm::records::EsmIndex,
    npcs: &HashMap<u32, byroredux_plugin::esm::records::NpcRecord>,
    races: &HashMap<u32, byroredux_plugin::esm::records::RaceRecord>,
    game: byroredux_plugin::esm::reader::GameKind,
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    mut mat_provider: Option<&mut MaterialProvider>,
    label: &str,
    load_order: &[String],
    // #1188 — FO4+ PreCombined absorbed REFR form IDs. Skip placement
    // for any REFR in this set: the CK's bake tool already folded
    // its geometry into a `meshes\precombined\<cell>_<hash>_oc.nif`
    // file that the precombined spawn step (later in this load) will
    // bring in. Spawning here would produce double geometry +
    // z-fighting on every wall / floor / ceiling.
    absorbed_refs: &std::collections::HashSet<u32>,
) -> RefLoadResult {
    let mut entity_count = 0;
    // CHARAL: build the per-game derived-stat ruleset once (idempotent across
    // cells) so `GetActorValue` can compute actor-general derived stats (Carry
    // Weight, Melee Damage, …) for actors that don't carry the value directly.
    if world
        .try_resource::<byroredux_core::character::CharacterRuleset>()
        .is_none()
    {
        if let Some(rs) = crate::npc_spawn::build_character_ruleset(game, record_index) {
            world.insert_resource(rs);
        }
    }
    // Number of mesh-bearing entities (those that receive a
    // `MeshHandle` insert in `spawn_placed_instances`). Distinct from
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
    let mut bounds_min = Vec3::splat(f32::INFINITY);
    let mut bounds_max = Vec3::splat(f32::NEG_INFINITY);
    // First door REFR's own placement in THIS cell (not its XTEL
    // destination) — a strictly better spawn-point candidate than the raw
    // bounding-box centroid below. A door is always placed on a walkable
    // threshold; the centroid of every placed REFR (statics, NPCs, invisible
    // trigger volumes, far-flung markers) has no such guarantee and can land
    // inside a wall, a stairwell void, or genuinely outside the interior
    // shell for L-shaped/multi-wing cells — the reported "spawns at random
    // points, sometimes outside the interior" bug. See the `center` doc
    // comment on `RefLoadResult`/`CellLoadResult`.
    let mut door_pos: Option<Vec3> = None;

    let mut stat_miss = 0u32;
    let mut stat_hit = 0u32;
    let mut enable_skipped = 0u32;
    // Count NIF / SPT files not found in the BSA archives. Logged at
    // info level (not debug) so missing-mesh failures surface in the
    // default log without needing RUST_LOG=debug. A large number here
    // indicates either wrong --bsa paths or a BSA path-lookup mismatch.
    let mut nif_not_found: u32 = 0;
    let mut nif_not_found_sample: Vec<String> = Vec::with_capacity(5);
    // #1188 — count REFRs skipped because the CK absorbed them into a
    // precombined `_oc.nif`. Surfaced in the end-of-cell summary so an
    // operator can spot a missing precombined-spawn step (would manifest
    // as "absorbed=N but precombined_spawned=0" pair below).
    let mut absorbed_skipped = 0u32;
    // Bounded sample of distinct miss FormIDs so an operator can
    // cross-reference in xEdit without flipping the whole log to
    // debug. Cap at 20 unique IDs; duplicates (same FormID placed
    // repeatedly across a worldspace) get deduped. See #386.
    let mut stat_miss_sample: Vec<u32> = Vec::with_capacity(20);
    // M41.0 Phase 1b — separate counters for the two NPC dispatch
    // paths so the per-cell summary distinguishes "spawned via
    // runtime-FaceGen path" (kf-era games — has a real spawn entity)
    // from "pre-baked-FaceGen pending" (Skyrim+/FO4+ — Phase 4 wires
    // the spawn). Sample stays small (8) since per-cell NPC counts
    // are bounded — Whiterun Bannered Mare carries ~6 actors versus
    // ~1 932 statics, Goodsprings exterior similar.
    let mut npc_spawned: u32 = 0;
    let mut npc_spawned_sample: Vec<u32> = Vec::with_capacity(8);
    // #1798 / D7-NEW-01 — the interior transition path (unlike the
    // exterior streaming path's `MAX_CELLS_SPAWNED_PER_FRAME` budget)
    // has no per-frame or per-NPC spawn budget, and until now no timing
    // existed to even show the cost. `spawn_npc_entity` /
    // `spawn_prebaked_npc_entity` make ~28 synchronous NIF-load call
    // sites per actor, so an NPC-dense interior cell can pay a large,
    // unmeasured stall in one synchronous frame. Accumulate wall time
    // spent in the two NPC-spawn call sites so the cost is visible in
    // the end-of-cell summary before investing in the larger chunked-
    // spawn-budget rewrite the issue's full suggested fix describes.
    let mut npc_spawn_wall = std::time::Duration::ZERO;
    // M47.2 — script-attach telemetry for the per-cell summary: how many
    // REFRs got canonical behavior from the recognizer chain, and how many
    // invisible trigger volumes were spawned. Both surface in the summary
    // so a smoke test can confirm the `.pex` attach + trigger paths fired
    // without inspecting individual entities.
    let mut scripts_recognized: u32 = 0;
    let mut trigger_volumes: u32 = 0;
    // `npc_pending` was the Phase 0/2 telemetry for pre-baked-FaceGen
    // games waiting on Phase 4's spawn path — kept (unused after
    // Phase 4 wired) so the cell summary's "0 ACHR refs ... pending"
    // line stays a coherent zero rather than disappearing entirely.
    // M41.0 lands every supported game on a real spawn function;
    // if a future game variant doesn't satisfy either predicate,
    // these fall back to the original telemetry shape.
    #[allow(unused_mut)]
    let mut npc_pending: u32 = 0;
    #[allow(unused_mut)]
    let mut npc_pending_sample: Vec<u32> = Vec::with_capacity(8);

    // M41.0 Phase 2 — resolve the per-game default idle KF clip once
    // before the REFR loop. The handle is threaded through every
    // `spawn_npc_entity` call so each NPC's `AnimationStack` references
    // the same registry slot. `load_idle_clip` itself is path-keyed
    // memoised (#790), so re-entry across cell loads is a HashMap hit
    // — neither the BSA extract nor `AnimationClipRegistry::add` runs
    // a second time for the same `kf_path`. Returns `None` when the
    // game is on the Havok-animation track (Skyrim+/FO4+) or the KF
    // isn't archived — NPCs in those cases just spawn without an
    // animation player. Gender variation is collapsed: FNV vanilla
    // ships only `_male\idle.kf` and uses it for both genders. The
    // `Gender` argument was dropped from these resolvers in #1117 /
    // TD8-018; re-introduce it when a game variant actually ships
    // separate clips.
    let idle_clip_handle = if game.has_kf_animations() {
        crate::npc_spawn::load_idle_clip(world, tex_provider, game)
    } else {
        None
    };

    // Per-call accumulators — committed to `NifImportRegistry` in a
    // single `resource_mut` borrow after the loop instead of acquiring
    // the write lock on every REFR. Previously every iteration took
    // `world.resource_mut::<NifImportRegistry>()` (write lock + atomic
    // CAS) even on the hot cache-hit path; for Prospector Saloon's
    // ~461 REFRs that was hundreds of write-lock cycles serialising
    // nothing. See #523.
    let mut this_call_hits: u64 = 0;
    let mut this_call_misses: u64 = 0;
    // Parses performed during this call. Merged into the registry at
    // end-of-function. `pending_new.get` shadows the registry read so
    // subsequent iterations of the loop see this call's own parses
    // without re-entering the registry.
    let mut pending_new: HashMap<String, Option<Arc<CachedNifImport>>> = HashMap::new();
    // Cache keys that resolved through a registry hit this call. Bulk-
    // bumped through `NifImportRegistry::touch_keys` at end-of-load so
    // recently-used entries float above the LRU eviction watermark.
    // #635 / FNV-D3-05.
    let mut pending_hits: Vec<String> = Vec::new();
    // Embedded-clip handles registered during this call. Mirrors
    // `pending_new` so the spawn loop can reach a freshly-registered
    // handle through the per-call shadow before the end-of-load
    // batched commit pushes it into `NifImportRegistry.clip_handles`.
    // Each parsed NIF whose `embedded_clip` is `Some` produces one
    // entry (after the conversion + `AnimationClipRegistry::add`
    // round-trip). Subsequent REFRs of the same model — within this
    // load or across cells — reach the same `u32` handle without
    // re-running `convert_nif_clip`. See #544 / #261.
    let mut pending_clip_handles: HashMap<String, u32> = HashMap::new();

    for placed_ref in refs {
        // Skip REFRs whose XESP gating would keep them hidden under
        // the parents-assumed-enabled heuristic: inverted XESP children
        // are visible only when the parent is *disabled*, so under the
        // default they stay off. Non-inverted XESP children fall through
        // and render. See #471 (flipped #349's over-hiding predicate)
        // — long-term fix is a two-pass loader that reads the parent
        // REFR's own 0x0800 "initial disabled" flag.
        if let Some(ep) = placed_ref.enable_parent {
            if ep.default_disabled() {
                enable_skipped += 1;
                continue;
            }
        }

        // #1188 — FO4+ PreCombined absorption skip. The bake tool
        // already folded this REFR's geometry into one of the
        // `meshes\precombined\<cell>_<hash>_oc.nif` files; the
        // precombined-spawn pass will bring those in as single
        // entities. Filtering here prevents double geometry.
        if absorbed_refs.contains(&placed_ref.form_id) {
            absorbed_skipped += 1;
            continue;
        }

        // Convert the outer REFR's placement (Z-up Bethesda → Y-up
        // renderer). For normal REFRs this is the spawn transform; for
        // SCOL REFRs it's the parent transform the child placements
        // compose against. #1617 — route through the coord SoT
        // (`zup_to_yup_pos`) rather than an inline `(x, z, -y)` so a future
        // change to the canonical swap can't silently skip this hot REFR
        // placement path. Bit-identical to the old inline form.
        let outer_pos =
            Vec3::from_array(byroredux_core::math::coord::zup_to_yup_pos(placed_ref.position));
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
        if door_pos.is_none() && placed_ref.teleport.is_some() {
            door_pos = Some(outer_pos);
        }

        // Build per-REFR texture overlay once. Shared across every
        // synthetic SCOL child — FO4 REFRs that overlay textures at the
        // SCOL level apply the same swap to every child placement.
        // #584.
        let refr_overlay = {
            let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
            build_refr_texture_overlay(placed_ref, index, mat_provider.as_deref_mut(), &mut pool)
        };

        // Compose REFR expansion from composite-record helpers:
        //   1. PKIN (#589) — Pack-In bundle fans out to one synth per
        //      `CNAM` content at the outer transform.
        //   2. SCOL (#585) — Static Collection fans out to one synth
        //      per `ONAM/DATA` placement when no cached `CM*.NIF`.
        //   3. Default — single synth at the outer transform.
        //
        // First expander that fires wins; `expand_scol_placements`
        // already returns the single-entry default when the base form
        // isn't a SCOL, so the chain closes cleanly.
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

        for (child_form_id, ref_pos, ref_rot, ref_scale) in synth_refs {
            // M41.0 Phase 1b — NPC dispatch must run BEFORE the
            // statics lookup. NPC_ records are also indexed in
            // `EsmCellIndex.statics` (because they carry a MODL —
            // the body mesh path) by `parse_modl_group` at
            // `crates/plugin/src/esm/cell/mod.rs:703`. If the
            // statics check ran first the static-spawn path would
            // claim every NPC ACHR and the NPC dispatcher would
            // never see them. Pre-fix `TestQAHairM` (31 NPCs / 61
            // refs) reported "61 statics hits, 0 NPCs spawned" — the
            // NPCs were silently rendered as a single non-skinned
            // body mesh per actor instead of going through the
            // skeleton-aware spawn function.
            if let Some(npc) = npcs.get(&child_form_id) {
                bounds_min = bounds_min.min(ref_pos);
                bounds_max = bounds_max.max(ref_pos);
                if game.has_runtime_facegen_recipe() {
                    let race = races.get(&npc.race_form_id);
                    let spawn_t0 = std::time::Instant::now();
                    let spawned = crate::npc_spawn::spawn_npc_entity(
                        world,
                        ctx,
                        npc,
                        race,
                        game,
                        tex_provider,
                        mat_provider.as_deref_mut(),
                        idle_clip_handle,
                        ref_pos,
                        ref_rot,
                        ref_scale,
                        record_index,
                    );
                    npc_spawn_wall += spawn_t0.elapsed();
                    if spawned.is_some() {
                        npc_spawned += 1;
                        if npc_spawned_sample.len() < 8
                            && !npc_spawned_sample.contains(&child_form_id)
                        {
                            npc_spawned_sample.push(child_form_id);
                        }
                        entity_count += 1;
                    }
                } else if game.uses_prebaked_facegen() {
                    // M41.0 Phase 4 — Skyrim / FO4 / FO76 / Starfield
                    // pre-baked-FaceGen dispatch. The NPC's plugin
                    // name resolves from the high byte of its
                    // load-order-global FormID against `load_order`;
                    // when the plugin can't be resolved (corrupt
                    // FormID, missing master), the spawn function
                    // logs and returns the placement root with no
                    // mesh — same observable outcome as a missing
                    // FaceGen NIF, just diagnosable from the log.
                    let plugin =
                        load_order::plugin_for_form_id(child_form_id, load_order).unwrap_or("");
                    let spawn_t0 = std::time::Instant::now();
                    let spawned = crate::npc_spawn::spawn_prebaked_npc_entity(
                        world,
                        ctx,
                        npc,
                        game,
                        tex_provider,
                        mat_provider.as_deref_mut(),
                        plugin,
                        ref_pos,
                        ref_rot,
                        ref_scale,
                        record_index,
                    );
                    npc_spawn_wall += spawn_t0.elapsed();
                    if spawned.is_some() {
                        npc_spawned += 1;
                        if npc_spawned_sample.len() < 8
                            && !npc_spawned_sample.contains(&child_form_id)
                        {
                            npc_spawned_sample.push(child_form_id);
                        }
                        entity_count += 1;
                    }
                }
                continue;
            }

            // M47.2 — invisible trigger volume. A REFR carrying an `XPRM`
            // box/sphere primitive and an attached script is a Bethesda
            // trigger box: no MODL, so the statics path below would skip
            // it (empty / missing mesh). Spawn a transform-only entity,
            // attach its world-space `TriggerVolume`, and run the script
            // attach so the recognizer's `OnTriggerEnter → SetStage`
            // advance lands. `trigger_detection_system` then fires
            // `OnTriggerEnterEvent` when the player crosses in. Gated on
            // *no renderable mesh* so a visible scripted activator (lever
            // with MODL + primitive) still spawns through the normal path
            // — only genuinely invisible triggers take this branch.
            let has_mesh = index
                .statics
                .get(&child_form_id)
                .is_some_and(|s| !s.model_path.is_empty());
            let has_script = record_index.base_record_script(child_form_id).is_some()
                || record_index
                    .base_record_script_instance(child_form_id)
                    .is_some()
                // #1737 — a model-less REFR can be a trigger volume scripted
                // purely by its OWN VMAD (no base-record script at all).
                || placed_ref.script_instance.is_some();
            if !has_mesh && has_script {
                if let Some(prim) = placed_ref.primitive.as_ref() {
                    if let Some(volume) =
                        trigger_volume_from_primitive(prim, ref_pos, ref_rot, ref_scale)
                    {
                        let entity = world.spawn();
                        world.insert(entity, Transform::new(ref_pos, ref_rot, ref_scale));
                        world.insert(entity, GlobalTransform::new(ref_pos, ref_rot, ref_scale));
                        world.insert(entity, volume);
                        if attach_script_for_refr(
                            world,
                            entity,
                            child_form_id,
                            record_index,
                            placed_ref.script_instance.as_ref(),
                        ) {
                            scripts_recognized += 1;
                        }
                        trigger_volumes += 1;
                        bounds_min = bounds_min.min(ref_pos);
                        bounds_max = bounds_max.max(ref_pos);
                        entity_count += 1;
                        continue;
                    }
                }
            }

            let stat = match index.statics.get(&child_form_id) {
                Some(s) => {
                    stat_hit += 1;
                    s
                }
                None => {
                    stat_miss += 1;
                    // Collect a bounded sample so the summary line can
                    // surface actual FormIDs without pulling down a
                    // full RUST_LOG=debug run. Linear dedup is fine
                    // for 20 entries. See #386.
                    if stat_miss_sample.len() < 20 && !stat_miss_sample.contains(&child_form_id) {
                        stat_miss_sample.push(child_form_id);
                    }
                    log::debug!("REFR base {:08X} not in statics table", child_form_id);
                    continue;
                }
            };

            // Update bounds from the (possibly SCOL-composed) placement.
            bounds_min = bounds_min.min(ref_pos);
            bounds_max = bounds_max.max(ref_pos);

            // Spawn light-only entities (LIGH with no mesh).
            if stat.model_path.is_empty() {
                if let Some(ref ld) = stat.light_data {
                    let entity = world.spawn();
                    world.insert(entity, Transform::new(ref_pos, ref_rot, ref_scale));
                    world.insert(entity, GlobalTransform::new(ref_pos, ref_rot, ref_scale));
                    world.insert(
                        entity,
                        LightSource {
                            radius: light_radius_or_default(ld.radius),
                            color: ld.color,
                            flags: ld.flags,
                            falloff_exponent: ld.falloff_exponent,
                            ..Default::default()
                        },
                    );
                    attach_light_flicker_if_needed(world, entity, ld, ref_pos);
                    entity_count += 1;
                }
                continue;
            }

            // Skip non-renderable meshes: editor markers, effect
            // sprites, fog. Still spawn the ESM light entity if this
            // LIGH record carries one — the effect mesh is visual-only
            // but the point light is real.
            let model_lower = stat.model_path.to_ascii_lowercase();

            // Extract the filename (after the last \ or /) for prefix matching.
            let filename = model_lower
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or(&model_lower);

            if filename.starts_with("marker")
                || filename.starts_with("xmarker")
                || filename.starts_with("defaultsetmarker")
                || filename.starts_with("doormarker")
                || filename.starts_with("northmarker")
                || filename.starts_with("prisonmarker")
                || filename.starts_with("travelmarker")
                || filename.starts_with("roommarker")
                || filename.starts_with("vatsmarker")
            {
                continue;
            }

            if model_lower.contains("fxlightrays")
                || model_lower.contains("fxlight")
                || model_lower.contains("fxfog")
            {
                if let Some(ref ld) = stat.light_data {
                    let entity = world.spawn();
                    world.insert(entity, Transform::from_translation(ref_pos));
                    world.insert(entity, GlobalTransform::new(ref_pos, Quat::IDENTITY, 1.0));
                    world.insert(
                        entity,
                        LightSource {
                            radius: light_radius_or_default(ld.radius),
                            color: ld.color,
                            flags: ld.flags,
                            falloff_exponent: ld.falloff_exponent,
                            ..Default::default()
                        },
                    );
                    entity_count += 1;
                }
                continue;
            }

            let model_path =
                if model_lower.starts_with("meshes\\") || model_lower.starts_with("meshes/") {
                    stat.model_path.clone()
                } else {
                    format!("meshes\\{}", stat.model_path)
                };

            // Fetch parsed+imported NIF from the process-lifetime
            // registry, or load+parse once. Three-tier lookup (#523):
            //   1. `pending_new` — this call's own parses, zero lock
            //      cost.
            //   2. Registry read-lock — a shared borrow that doesn't
            //      serialise against concurrent readers.
            //   3. Parse outside any lock, insert into `pending_new`;
            //      the merge into the registry happens in a single
            //      write lock after the loop.
            //
            // Previously this block took `resource_mut` (write lock)
            // on every iteration even on the hit path; see #523 / #381
            // for the wider cache history.
            let cache_key = model_path.to_ascii_lowercase();
            let cached = if let Some(entry) = pending_new.get(&cache_key).cloned() {
                this_call_hits += 1;
                entry
            } else {
                let reg_entry = {
                    let reg = world.resource::<NifImportRegistry>();
                    reg.get(&cache_key).cloned()
                };
                match reg_entry {
                    Some(entry) => {
                        this_call_hits += 1;
                        // Mark for LRU touch at the end-of-load batched
                        // commit so frequently-revisited meshes don't
                        // get evicted under `BYRO_NIF_CACHE_MAX`. The
                        // batched flush keeps the read path on a shared
                        // lock — preserves the #523 invariant.
                        pending_hits.push(cache_key.clone());
                        entry
                    }
                    None => {
                        // Slow-path: parse outside any registry borrow.
                        // Take the StringPool write lock only for the
                        // parse + intern + BGSM merge — the read lock
                        // on `NifImportRegistry` was released at the
                        // close of the `reg_entry` scope above, so the
                        // two locks never overlap. See #609.
                        //
                        // SpeedTree extension switch (Phase 1.5).
                        // Pre-Skyrim TREE records point MODL at a
                        // `.spt` SpeedTree binary instead of a NIF —
                        // dispatch to the SPT crate's parser/importer
                        // when we see that extension. The TREE record
                        // (carrying ICON / OBND / etc.) is looked up
                        // from `record_index.trees` keyed by the same
                        // form id the cell loader resolved against
                        // `index.statics`. See SpeedTree plan 1.5.
                        let is_spt = model_path
                            .as_str()
                            .rsplit('.')
                            .next()
                            .map(|ext| ext.eq_ignore_ascii_case("spt"))
                            .unwrap_or(false);
                        let parsed = match tex_provider.extract_mesh(&model_path) {
                            Some(d) => {
                                let mut pool =
                                    world.resource_mut::<byroredux_core::string::StringPool>();
                                if is_spt {
                                    let tree_record = record_index.trees.get(&child_form_id);
                                    parse_and_import_spt(&d, &model_path, tree_record, &mut pool)
                                } else {
                                    parse_and_import_nif(
                                        &d,
                                        &model_path,
                                        mat_provider.as_deref_mut(),
                                        &mut pool,
                                        Some(tex_provider),
                                    )
                                }
                            }
                            None => {
                                log::debug!(
                                    "{} not found in BSA: '{}'",
                                    if is_spt { "SPT" } else { "NIF" },
                                    model_path,
                                );
                                nif_not_found += 1;
                                if nif_not_found_sample.len() < 5 {
                                    nif_not_found_sample.push(model_path.clone());
                                }
                                None
                            }
                        };
                        this_call_misses += 1;
                        // #544 — register the embedded animation clip
                        // exactly once per parsed NIF, before stashing
                        // into `pending_new`. Subsequent REFRs of this
                        // model reach the handle through the per-call
                        // shadow (`pending_clip_handles`) or, on later
                        // cell loads, through `NifImportRegistry::
                        // clip_handle_for` after the end-of-load
                        // commit. The conversion runs at most once per
                        // unique model across the process — matches
                        // the loose-NIF path's one-clip-per-NIF
                        // invariant from #261.
                        if let Some(ref cached) = parsed {
                            if let Some(nif_clip) = cached.embedded_clip.as_ref() {
                                let handle = {
                                    let mut pool =
                                        world.resource_mut::<byroredux_core::string::StringPool>();
                                    let clip =
                                        crate::anim_convert::convert_nif_clip(nif_clip, &mut pool);
                                    drop(pool);
                                    let mut clip_reg = world.resource_mut::<
                                        byroredux_core::animation::AnimationClipRegistry,
                                    >();
                                    clip_reg.add(clip)
                                };
                                pending_clip_handles.insert(cache_key.clone(), handle);
                            }
                        }
                        pending_new.insert(cache_key.clone(), parsed.clone());
                        parsed
                    }
                }
            };
            let Some(cached) = cached else { continue };

            // #544 — embedded animation-clip handle for this REFR's
            // model. Three-tier lookup mirrors the cache:
            //   1. `pending_clip_handles` — registered earlier in this
            //      call's slow path.
            //   2. `NifImportRegistry::clip_handle_for` — registered
            //      by an earlier cell load. Read-only / shared lock.
            //   3. `None` — the cached NIF authored no controllers.
            // Subsequent REFRs of the same model in this same load
            // hit case (1) and never touch the registry write path.
            let clip_handle = pending_clip_handles.get(&cache_key).copied().or_else(|| {
                world
                    .resource::<NifImportRegistry>()
                    .clip_handle_for(&cache_key)
            });

            // #1212 / D1-NEW-01 — build the placement FormIdPair so the
            // spawn site can attach a `FormIdComponent` on the placement
            // root. Plugin lookup uses `placed_ref.form_id` against the
            // load-order map (master + DLC + mod chain post-#445 remap).
            // Unresolved plugin → "Engine.esm" placeholder so the
            // intern still succeeds; the placement form-id itself is
            // the unique key callers consume via `find_by_form_id`.
            let placement_pair = {
                let plugin_name =
                    plugin_for_form_id(placed_ref.form_id, load_order).unwrap_or("Engine.esm");
                FormIdPair {
                    plugin: PluginId::from_filename(plugin_name),
                    local: LocalFormId(placed_ref.form_id),
                }
            };
            let (placement_root, count) = spawn_placed_instances(
                world,
                ctx,
                &cached,
                tex_provider,
                ref_pos,
                ref_rot,
                ref_scale,
                stat.light_data.as_ref(),
                refr_overlay.as_ref(),
                clip_handle,
                stat.record_type.render_layer(),
                Some(cache_key.as_str()),
                Some(placement_pair),
                placed_ref.teleport,
            );
            entity_count += count;

            // M47.0 Phase 3b — attach script state to the placement
            // root. `child_form_id` is the leaf base record (SCOL /
            // PKIN children each get their own; non-expanded REFRs
            // pass placed_ref.base_form_id verbatim). Index → SCPT →
            // editor_id → ScriptRegistry → spawner; misses fall
            // through silently per Phase 2's "unregistered scripts are
            // common" contract. See docs/engine/m47-0-design.md.
            if attach_script_for_refr(
                world,
                placement_root,
                child_form_id,
                record_index,
                placed_ref.script_instance.as_ref(),
            ) {
                scripts_recognized += 1;
            }
        }
    }

    let bbox_center = (bounds_min + bounds_max) * 0.5;
    let dims = bounds_max - bounds_min;
    // Spawn-point precedence: first door in this cell (walkable threshold,
    // guaranteed) > bounding-box centroid (best-effort, can land inside
    // geometry or outside the shell) > world origin (empty cell — no
    // placements accumulated into bounds at all, matching vanilla `coc`'s
    // local-origin fallback when nothing else applies).
    let center = door_pos.unwrap_or(if bbox_center.x.is_finite() {
        bbox_center
    } else {
        Vec3::ZERO
    });
    // #1495 / REN2-10 — fail loud in debug if this cell's geometry sits
    // beyond the RT absolute-space f32 precision ceiling (see
    // `RT_ABSOLUTE_PRECISION_CEILING`). Never fires on vanilla content;
    // catches a future mega-worldspace before its rays silently degrade.
    debug_assert!(
        worldspace_extent_over_rt_ceiling(bounds_min, bounds_max).is_none(),
        "cell '{}' worldspace extent {:.0} u reaches the RT absolute-space \
         f32 precision ceiling ({:.0} u): ray bias/tMin margins fall below \
         the f32 ULP at this magnitude. See #1495 / \
         docs/engine/shader-pipeline.md.",
        label,
        worldspace_extent_over_rt_ceiling(bounds_min, bounds_max).unwrap_or(0.0),
        RT_ABSOLUTE_PRECISION_CEILING,
    );
    // Commit the accumulated counters + pending entries in a single
    // write lock. Stats snapshot happens in the same scope so the log
    // line below reflects post-commit numbers. See #523. `insert`
    // drives `parsed_count` / `failed_count` and runs LRU eviction; we
    // touch hit keys first so they bump above the LRU watermark before
    // any new inserts fight them for cache space (#635 / FNV-D3-05).
    let (this_cell_hits, this_cell_misses, this_cell_unique, lifetime_hit_rate, freed_clip_handles) = {
        let mut reg = world.resource_mut::<NifImportRegistry>();
        let mut freed: Vec<u32> = Vec::new();
        reg.accumulate_hits(this_call_hits);
        reg.accumulate_misses(this_call_misses);
        reg.touch_keys(pending_hits.iter().map(String::as_str));
        for (key, entry) in pending_new {
            // #863 — accumulate LRU-evicted clip handles from each
            // insert; the AnimationClipRegistry release happens after
            // we drop the NifImportRegistry write lock.
            freed.extend(reg.insert(key, entry));
        }
        // #544 — commit per-call clip handles into the process-lifetime
        // registry. Future cell loads of the same NIF reach the
        // memoised handle through `clip_handle_for` without
        // re-converting the channel arrays.
        for (key, handle) in pending_clip_handles {
            reg.set_clip_handle(key, handle);
        }
        let new_entries = reg.len().saturating_sub(cache_size_at_entry);
        (
            reg.core.hits().saturating_sub(cache_hits_at_entry),
            reg.core.misses().saturating_sub(cache_misses_at_entry),
            new_entries,
            reg.hit_rate_pct(),
            freed,
        )
    };
    // Release the clip-registry slots of every cache victim that
    // surfaced an evicted clip handle (#863). Drains the keyframe
    // arrays without invalidating live `clip_handle: u32` consumers.
    if !freed_clip_handles.is_empty() {
        let mut clip_reg = world.resource_mut::<byroredux_core::animation::AnimationClipRegistry>();
        for h in freed_clip_handles {
            clip_reg.release(h);
        }
    }
    log::info!(
        "'{}' loaded: {} entities, {} new unique meshes parsed, NIF cache hits/misses {}/{} this cell ({:.1}% lifetime hit rate), {} statics hits, {} statics misses",
        label,
        entity_count,
        this_cell_unique,
        this_cell_hits,
        this_cell_misses,
        lifetime_hit_rate,
        stat_hit,
        stat_miss,
    );
    log::info!(
        "  Bounds: min=[{:.0},{:.0},{:.0}] max=[{:.0},{:.0},{:.0}] size=[{:.0},{:.0},{:.0}] spawn=[{:.0},{:.0},{:.0}]{}",
        bounds_min.x, bounds_min.y, bounds_min.z,
        bounds_max.x, bounds_max.y, bounds_max.z,
        dims.x, dims.y, dims.z,
        center.x, center.y, center.z,
        if door_pos.is_some() { " (door)" } else { " (bbox centroid fallback)" },
    );
    if scripts_recognized > 0 || trigger_volumes > 0 {
        // M47.2 — the recognizer chain attached canonical ECS behavior to
        // `scripts_recognized` REFRs (`.pex` decompile / SCPT registry),
        // and `trigger_volumes` invisible trigger boxes were spawned with
        // a TriggerVolume. The smoke test asserts on this line to confirm
        // the compiled-script + trigger paths fired on real game data.
        log::info!(
            "  M47.2 scripts: {} REFRs recognized, {} trigger volumes spawned",
            scripts_recognized,
            trigger_volumes,
        );
    }
    if npc_spawned > 0 {
        // M41.0 Phase 1b + Phase 4 — NPC actors landed. The
        // dispatcher routes through the runtime-FaceGen path
        // (kf-era games — applies FGGS/FGGA morphs to the race base
        // head) or the pre-baked-FaceGen path (Skyrim+ — loads the
        // per-NPC pre-deformed NIF) per `GameKind`. Both end at the
        // same placement_root + skeleton + skinned mesh shape for
        // visual QA purposes.
        let sample_str = npc_spawned_sample
            .iter()
            .map(|id| format!("{:08X}", id))
            .collect::<Vec<_>>()
            .join(", ");
        let trunc = if (npc_spawned_sample.len() as u32) < npc_spawned {
            format!(
                ", … +{} more",
                npc_spawned - npc_spawned_sample.len() as u32
            )
        } else {
            String::new()
        };
        let path_label = if game.has_runtime_facegen_recipe() {
            "runtime-FaceGen"
        } else {
            "pre-baked-FaceGen"
        };
        log::info!(
            "  {} NPCs spawned via {} path (sample: {}{}), {:.1}ms wall in spawn calls",
            npc_spawned,
            path_label,
            sample_str,
            trunc,
            npc_spawn_wall.as_secs_f64() * 1000.0,
        );
    }
    if npc_pending > 0 {
        // M41.0 Phase 4 — Skyrim/FO4/FO76/Starfield NPCs sit on the
        // pre-baked-FaceGen path; their dispatch lands when Phase 4
        // wires the per-NPC NIF + face-tint resolution.
        let sample_str = npc_pending_sample
            .iter()
            .map(|id| format!("{:08X}", id))
            .collect::<Vec<_>>()
            .join(", ");
        let trunc = if (npc_pending_sample.len() as u32) < npc_pending {
            format!(
                ", … +{} more",
                npc_pending - npc_pending_sample.len() as u32
            )
        } else {
            String::new()
        };
        log::info!(
            "  {} ACHR refs resolve to NPC_ (M41.0 Phase 4 pre-baked-FaceGen dispatch pending; sample: {}{})",
            npc_pending,
            sample_str,
            trunc,
        );
    }
    if stat_miss > 0 {
        // Log the bounded sample at info level so the miss types are
        // diagnosable without flipping to debug. Common causes:
        // leveled-list targets (LVLI/LVLN/LVLC — parsed elsewhere, not
        // in `index.statics`), master-ESM-only forms, and mod-added
        // records without a loaded master. See #386 for the roadmap
        // toward leveled-list resolution.
        let sample_str = stat_miss_sample
            .iter()
            .map(|id| {
                let plugin = plugin_for_form_id(*id, load_order).unwrap_or("???");
                format!("{:08X} (from '{}')", id, plugin)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let truncation_marker = if (stat_miss_sample.len() as u32) < stat_miss {
            format!(", … +{} more", stat_miss - stat_miss_sample.len() as u32)
        } else {
            String::new()
        };
        // #561 — when load_order has more than one plugin, also break
        // down misses by plugin so the user can tell whether a missing
        // master is the cause (top byte points at a plugin in the
        // load order whose statics table is missing the FormID =
        // unresolved cross-plugin override) vs. a leveled-list /
        // dynamic-form target (top byte points at a loaded plugin
        // whose statics table genuinely doesn't carry the form).
        let plugin_breakdown = if load_order.len() > 1 {
            let mut by_plugin: std::collections::HashMap<&str, u32> =
                std::collections::HashMap::new();
            for id in &stat_miss_sample {
                let plugin = plugin_for_form_id(*id, load_order).unwrap_or("???");
                *by_plugin.entry(plugin).or_insert(0) += 1;
            }
            let mut rows: Vec<_> = by_plugin.into_iter().collect();
            rows.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
            let s = rows
                .iter()
                .map(|(p, n)| format!("{}={}", p, n))
                .collect::<Vec<_>>()
                .join(", ");
            format!(" — by plugin (in sample): {}", s)
        } else {
            String::new()
        };
        // F5 2026-05-27: the message is intentionally STATics-only
        // because the REFR-spawn path only looks up `index.statics`.
        // Most of the "missing" hits are ACTI quest-trigger volumes
        // (FNV `*Trigger` activators), or engine-defined references
        // like FO3's player-placement FormID `0x00000021`. Once the
        // cell loader walks `index.activators` / `index.containers` /
        // `index.doors` / `index.npcs` for non-STAT REFRs (proper
        // categorised spawn at M30+ script-execution time), this
        // counter drops naturally. Today the warning reflects
        // "REFRs we didn't spawn a mesh for," not a parser bug.
        log::warn!(
            "  {} base forms not in STATics dispatch (often ACTI triggers or \
             engine-defined refs — see F5 in docs/audits/FALLOUT_SYMPTOMS_*; \
             sample: {}{}){}",
            stat_miss,
            sample_str,
            truncation_marker,
            plugin_breakdown,
        );
    }
    if enable_skipped > 0 {
        log::info!(
            "  {} REFRs skipped via XESP enable-parent gating (#349)",
            enable_skipped,
        );
    }
    if absorbed_skipped > 0 {
        log::info!(
            "  {} REFRs skipped via FO4 PreCombined absorption — geometry \
             served by precombined-spawn pass (#1188)",
            absorbed_skipped,
        );
    }
    if nif_not_found > 0 {
        let sample = nif_not_found_sample.join(", ");
        let trunc = if nif_not_found > nif_not_found_sample.len() as u32 {
            format!(
                ", … +{} more",
                nif_not_found - nif_not_found_sample.len() as u32
            )
        } else {
            String::new()
        };
        log::info!(
            "  {} unique model paths not found in BSA archives \
             (wrong --bsa? check paths: {}{})",
            nif_not_found,
            sample,
            trunc,
        );
    }

    // #881 / CELL-PERF-03 — drain queued DDS uploads with ONE
    // batched submit + ONE fence-wait. Pre-fix every fresh DDS
    // texture in this cell paid its own `with_one_time_commands`
    // (submit + fence-wait), accumulating ~50–100 ms of stall on
    // worldspace edge crossings. The cell-load completion gate is
    // the right place: every REFR has been spawned with its
    // bindless handle attached (descriptor temporarily redirected
    // to the fallback), and the next draw must see real images.
    let pending_uploads = ctx.texture_registry.pending_dds_upload_count();
    if pending_uploads > 0 {
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
                "  Cell texture upload batch: {n}/{pending_uploads} DDS textures uploaded",
            ),
            Err(e) => {
                log::warn!("Cell texture upload batch failed ({pending_uploads} pending): {e}",)
            }
        }
    }

    RefLoadResult {
        entity_count,
        center,
    }
}

/// Parse + import a NIF scene once. Returns `None` on parse failure
/// or when the scene has zero useful geometry. All per-block parse
/// warnings and the truncation message (if any) are emitted exactly
/// once per unique NIF at this step — subsequent placements read
/// from the cache without re-parsing. See runtime-spam incident from
/// the `AnvilHeinrichOakenHallsHouse` trace.
/// Public re-export of `parse_and_import_nif` for the precombined-mesh
/// loader (#1188). `pub(super)` so only sibling modules in
/// `cell_loader` can reach it.
pub(super) fn parse_and_import_nif_pub(
    nif_data: &[u8],
    label: &str,
    mat_provider: Option<&mut MaterialProvider>,
    pool: &mut byroredux_core::string::StringPool,
    mesh_resolver: Option<&dyn byroredux_nif::import::MeshResolver>,
) -> Option<Arc<CachedNifImport>> {
    parse_and_import_nif(nif_data, label, mat_provider, pool, mesh_resolver)
}

fn parse_and_import_nif(
    nif_data: &[u8],
    label: &str,
    mat_provider: Option<&mut MaterialProvider>,
    pool: &mut byroredux_core::string::StringPool,
    mesh_resolver: Option<&dyn byroredux_nif::import::MeshResolver>,
) -> Option<Arc<CachedNifImport>> {
    let scene = match byroredux_nif::parse_nif(nif_data) {
        Ok(s) => {
            log::debug!("Parsed NIF '{}': {} blocks", label, s.len());
            if s.truncated {
                log::warn!(
                    "  NIF '{}' parsed with truncation — downstream import will \
                     work from the partial block list",
                    label
                );
            }
            s
        }
        Err(e) => {
            log::warn!("Failed to parse NIF '{}': {}", label, e);
            return None;
        }
    };

    // BSXFlags bit 5 — semantics differ across game eras:
    //   * Oblivion / FO3 / FNV (BSVER < FALLOUT4): bit 5 = `EditorMarker`.
    //     The NIF is an invisible CK pin (XMarker, PrisonMarker, etc.)
    //     and must not render.
    //   * Skyrim / FO4 / FO76 / Starfield (BSVER >= FALLOUT4):
    //     bit 5 = `MultiBoundNode` (Bethesda re-purposed it). A hint
    //     that the NIF carries an authored BSMultiBound for culling.
    //     Filtering on it drops legitimate architecture — F4 in the
    //     2026-05-26 sweep was caused by `hitfloorsolidfull01.nif`
    //     (FO4 Institute floor, BSXFlags = 0xA2, bit 5 set) and 14
    //     siblings being wrongly classified as editor markers.
    //
    // For FO4+ we rely on the name-based check in `walk/mod.rs:1430`
    // (`is_editor_marker`) which catches names matching `EditorMarker*`,
    // `marker_*`, `MarkerX`, `marker:*`, `MapMarker` — every shipping
    // FO4 editor-marker NIF authored a name in that family.
    let bsx = byroredux_nif::import::extract_bsx_flags(&scene);
    // NifScene doesn't retain the header, so re-parse the header
    // (~60 bytes) to read `bs_version`. Cheap relative to the full
    // scene parse we already did.
    let bsver = byroredux_nif::header::NifHeader::parse(nif_data)
        .map(|(h, _)| h.user_version_2)
        .unwrap_or(0);
    let bsx_editor_marker = bsx & 0x20 != 0 && bsver < byroredux_nif::version::bsver::FALLOUT4;
    if bsx_editor_marker {
        log::debug!(
            "Skipping editor marker NIF '{}' (BSXFlags 0x{:X}, BSVER {})",
            label,
            bsx,
            bsver,
        );
        return None;
    }
    // Root-node NiAVObject.flags — surfaced for the placement-root
    // SceneFlags row. See #1235 / LC-D1-NEW-01.
    let root_flags = byroredux_nif::import::extract_root_flags(&scene);

    let (mut meshes, collisions) =
        byroredux_nif::import::import_nif_with_collision_and_resolver(&scene, pool, mesh_resolver);
    // FO4+ external material resolution (#493). Walk once at cache-fill
    // time so every REFR sharing this NIF sees the merged texture paths.
    // NIF fields take precedence; only empty slots are filled from the
    // resolved BGSM/BGEM chain.
    if let Some(provider) = mat_provider {
        for mesh in &mut meshes {
            merge_bgsm_into_mesh(mesh, provider, pool);
        }
    }
    let lights = byroredux_nif::import::import_nif_lights(&scene);
    let particle_emitters = byroredux_nif::import::import_nif_particle_emitters(&scene);
    let embedded_clip = byroredux_nif::anim::import_embedded_animations(&scene);
    // Cell-load path doesn't yet attach `Name` components or a
    // per-placement subtree root to spawned mesh entities, so the
    // AnimationStack's name-keyed subtree lookup can't anchor onto the
    // flat-spawn hierarchy. Clips extracted here are captured on the
    // cache entry for a follow-up wiring pass (add placement-root
    // entities + parent meshes under them, then attach a scoped
    // AnimationPlayer per placement). See #261. The loose-NIF
    // `load_nif_bytes` path already consumes embedded clips end-to-end.
    if let Some(ref clip) = embedded_clip {
        log::debug!(
            "NIF '{}' has {} embedded controllers ({} float + {} color + {} bool) \
             — captured on cache; cell-loader spawn wiring is a follow-up",
            label,
            clip.float_channels.len() + clip.color_channels.len() + clip.bool_channels.len(),
            clip.float_channels.len(),
            clip.color_channels.len(),
            clip.bool_channels.len(),
        );
    }
    // #1215 / D2 FIND-1 — surface zero-contribution imports loudly. A
    // NIF that parses cleanly but yields no meshes / collisions / lights /
    // emitters / clips is almost always either a CSG-deferred precombined
    // `_oc.nif` (Shared variant, geometry in companion `.csg` blob —
    // #1188) or a malformed scene. Pre-#1215 these were silently
    // returned as empty `CachedNifImport` entries and the operator
    // hit a "props in a void" symptom downstream with no log clue.
    // The fix is observability-only — cache invariants unchanged.
    if meshes.is_empty()
        && collisions.is_empty()
        && lights.is_empty()
        && particle_emitters.is_empty()
        && embedded_clip.is_none()
    {
        log::warn!(
            "NIF '{}' imported with zero meshes / collisions / lights / \
             emitters / clips — likely CSG-deferred (`_oc.nif` Shared \
             variant, #1188) or pure marker scene",
            label,
        );
    }
    // Phase 18 — walk the scene graph for a flame-marker node and
    // capture its world position relative to the root. Most Skyrim
    // candles + chandeliers + torches author this as a `Flame01` /
    // `AttachFire` / `AttachLight` NiNode child of the root; the
    // ESM-fallback light should sit at that offset, not at the
    // placement root. The nodes array doesn't survive into
    // `CachedNifImport`, so the offset is computed once here and
    // stored as `flame_attach_offset` for the spawn site to read.
    let flame_attach_offset = find_flame_attach_offset(&scene);

    // #985 / #1594 — materialize the FO4+ weapon-mod attach graph. The flat
    // import drops the node array, so pull the `BSConnectPoint` blocks
    // straight off the parsed scene (the transforms are already Y-up — the
    // extractor converts) and intern them into the ECS components here,
    // where the StringPool lives. The spawn site stamps them onto the
    // placement root. `None` for the dominant non-modular case.
    let attach_points = byroredux_nif::import::extract_attach_points(&scene)
        .map(|pts| attach_points_component(&pts, pool));
    let child_attach_connections = byroredux_nif::import::extract_child_attach_connections(&scene)
        .map(|c| child_attach_connections_component(&c, pool));

    Some(Arc::new(CachedNifImport {
        meshes,
        collisions,
        lights,
        particle_emitters,
        embedded_clip,
        // NIF cell-loader path leaves billboard wiring to a follow-up —
        // imported.nodes here represent the whole scene graph, not the
        // placement root, so we'd need a "which node corresponds to the
        // REFR placement" heuristic. Tracked alongside #994.
        placement_root_billboard: None,
        // #1214 / D1-NEW-03 — surface the BSXFlags bits on the cache
        // entry so the spawn site can attach a `BSXFlags` ECS row on
        // the placement root. The editor-marker bit (0x20) is consumed
        // above as an early-return; the remaining bits (havok-managed,
        // ragdoll, articulated, externally-emitted-particles, etc.)
        // ride through to the ECS for downstream consumers.
        bsx_flags: bsx,
        // #1235 / LC-D1-NEW-01 — root-node NiAVObject.flags for
        // placement-root SceneFlags parity with the loose-NIF loader.
        root_flags,
        flame_attach_offset,
        attach_points,
        child_attach_connections,
    }))
}

/// Intern an [`ImportedAttachPoint`] list into the `AttachPoints` ECS
/// component (#985 / #1594). Attach-point names + parent-bone tags become
/// `FixedString` handles so the equip-time `AttachPoints::find` lookup is an
/// integer compare. Transforms arrive already Y-up from the extractor.
pub(super) fn attach_points_component(
    imported: &[byroredux_nif::import::ImportedAttachPoint],
    pool: &mut byroredux_core::string::StringPool,
) -> byroredux_core::ecs::components::AttachPoints {
    use byroredux_core::ecs::components::{AttachPoint, AttachPoints};
    AttachPoints {
        points: imported
            .iter()
            .map(|p| AttachPoint {
                name: pool.intern(p.name.as_str()),
                // Empty `parent` → anchored on the host mesh root, not a bone.
                parent_bone: (!p.parent.is_empty()).then(|| pool.intern(p.parent.as_str())),
                translation: p.translation,
                rotation: p.rotation,
                scale: p.scale,
            })
            .collect(),
    }
}

/// Intern an [`ImportedChildAttachConnections`] into the
/// `ChildAttachConnections` ECS component (#985 / #1594).
pub(super) fn child_attach_connections_component(
    imported: &byroredux_nif::import::ImportedChildAttachConnections,
    pool: &mut byroredux_core::string::StringPool,
) -> byroredux_core::ecs::components::ChildAttachConnections {
    byroredux_core::ecs::components::ChildAttachConnections {
        connect_names: imported
            .point_names
            .iter()
            .map(|n| pool.intern(n.as_str()))
            .collect(),
        skinned: imported.skinned,
    }
}

/// Phase 18 — locate the flame-attach marker node in a parsed NIF
/// scene. Scans every node's name for the canonical flame-marker
/// substrings Skyrim's CK uses, then composes the node's world
/// position relative to the placement root by walking its parent
/// chain.
///
/// Names checked (case-insensitive substring match):
/// - `flame` — `Flame01`, `FlameNode`, `CandleFlame`
/// - `fire` — `FireNode01`, `AttachFire`
/// - `attachlight` — `AttachLight01`
///
/// First match wins. Returns `None` when no matching node is
/// authored — the typical case for static props that ship LIGH
/// data only on the REFR placement (no NIF marker). The spawn
/// path falls back to the placement-root position in that case,
/// preserving pre-Phase-18 behaviour.
///
/// Cost: O(nodes). NIF scenes typically have 10-100 nodes; the
/// search runs once per unique model path at cache fill time
/// and the result is cached across every placement.
pub(super) fn find_flame_attach_offset(scene: &byroredux_nif::scene::NifScene) -> Option<[f32; 3]> {
    const PATTERNS: &[&str] = &["flame", "fire", "attachlight"];

    // Walk raw NIF blocks. Limited to first-level lookup: returns
    // the flame node's local translation (relative to its
    // immediate parent — typically the scene root, where this
    // composes correctly). Deep-nested flame nodes (some
    // chandelier rigs) would need full parent-chain composition
    // by following `children` references back to root; deferred
    // until a visible bug surfaces.
    for idx in 0..scene.blocks.len() {
        // `NifScene::get_as` downcasts the boxed NiObject to the
        // concrete type via `as_any().downcast_ref()`. NiNode
        // carries `av.net.name` + `av.transform.translation` —
        // everything the flame-marker search needs.
        let Some(node) = scene.get_as::<byroredux_nif::blocks::node::NiNode>(idx) else {
            continue;
        };
        let name = match node.name() {
            Some(n) => n,
            None => continue,
        };
        let name_lower = name.to_ascii_lowercase();
        if PATTERNS.iter().any(|p| name_lower.contains(p)) {
            let t = node.transform().translation;
            // NIF is Z-up; the engine is Y-up. Route through the canonical
            // array-form flip so this stays in lockstep with the importer
            // (was an inline `[t.x, t.z, -t.y]` copy — #1318 / TD3-NEW-B).
            return Some(byroredux_core::math::coord::zup_to_yup_pos([t.x, t.y, t.z]));
        }
    }
    None
}

/// Parse a SpeedTree `.spt` byte slice and convert it to the same
/// [`CachedNifImport`] shape every other model goes through. Lets the
/// cache + spawn paths consume `.spt` REFRs without a parallel
/// dispatch tree.
///
/// Today (Phase 1.4 + 1.5) the SPT importer ships the **placeholder
/// fallback** — a single yaw-billboard quad textured with the leaf
/// icon resolved from the matching `TreeRecord` (TREE.ICON wins,
/// `.spt` tag 4003 falls back). When the geometry-tail decoder lands
/// later, `byroredux_spt::import_spt_scene` will start producing
/// real branch / frond meshes + per-leaf billboards without any
/// signature change here.
///
/// Returns `None` on parse failure or when the importer produces no
/// usable geometry (e.g. `.spt` magic missing) so subsequent REFRs
/// of the same model don't re-attempt the doomed parse.
fn parse_and_import_spt(
    spt_data: &[u8],
    label: &str,
    tree_record: Option<&byroredux_plugin::esm::records::TreeRecord>,
    pool: &mut byroredux_core::string::StringPool,
) -> Option<Arc<CachedNifImport>> {
    let scene = match byroredux_spt::parse_spt(spt_data) {
        Ok(s) => {
            log::debug!(
                "Parsed SPT '{}': {} entries, tail at offset {}",
                label,
                s.entries.len(),
                s.tail_offset,
            );
            if !s.unknown_tags.is_empty() {
                log::debug!(
                    "  SPT '{}' bailed at unknown tag {} (offset {}) — \
                     parameter section partial; placeholder still renders",
                    label,
                    s.unknown_tags[0].0,
                    s.unknown_tags[0].1,
                );
            }
            s
        }
        Err(e) => {
            log::warn!("Failed to parse SPT '{}': {}", label, e);
            return None;
        }
    };

    // Build SptImportParams from the matching TREE record. Every
    // field defaults gracefully when the record is absent — a `.spt`
    // referenced from a stub TREE (or from non-TREE content) still
    // gets a generic-sized placeholder.
    let leaf_texture_override = tree_record
        .map(|t| t.leaf_texture.as_str())
        .filter(|s| !s.is_empty());

    let bounds = tree_record.and_then(|t| t.bounds).map(|b| {
        let min = [b.min[0] as f32, b.min[1] as f32, b.min[2] as f32];
        let max = [b.max[0] as f32, b.max[1] as f32, b.max[2] as f32];
        (min, max)
    });

    // Wind sensitivity / strength would come from CNAM, not BNAM
    // (BNAM is billboard-card width/height per UESP). CNAM semantics
    // aren't pinned down yet — Phase 2 wires it. Leave None so the
    // placeholder doesn't pretend to know the wind response.
    let wind = None;

    let form_id = tree_record.map(|t| t.form_id);

    // #1001 — Oblivion ships MODB on 100 % of TREE records and OBND
    // on none, so the placeholder size fallback needs MODB to size
    // Cyrodiil trees correctly (vanilla MODB range 157–3621 game
    // units). FO3/FNV are inverse: 100 % OBND, 0 % MODB. Surface both
    // and let `compute_billboard_size` pick its precedence.
    let bound_radius = tree_record.map(|t| t.bound_radius).filter(|r| *r > 0.0);

    // #1002 — BNAM (FO3/FNV billboard width × height) as a fallback
    // BELOW OBND. Corpus inspection (2026-05-13) showed BNAM clamps
    // tall trees vs their physical OBND extent (e.g. `WhiteOak01`
    // BNAM 768×768 vs OBND 802×1567), so OBND wins for the
    // whole-tree placeholder. BNAM only reaches `compute_billboard_size`
    // when OBND is absent — a rare mod-content case in FO3/FNV.
    let billboard_size = tree_record.and_then(|t| t.billboard_size);

    let params = byroredux_spt::SptImportParams {
        leaf_texture_override,
        bounds,
        wind,
        form_id,
        bound_radius,
        billboard_size,
    };

    let imported = byroredux_spt::import_spt_scene(&scene, &params, pool);

    // #994 — the placeholder root node is authored with
    // `billboard_mode = Some(BsRotateAboutUp)` so the cell-loader spawn
    // can attach a `Billboard` ECS component to the placement root.
    // Pre-#994 this field was dropped because `CachedNifImport` carried
    // no node metadata; trees rendered as static quads facing whichever
    // direction the REFR was authored at.
    let placement_root_billboard = imported
        .nodes
        .first()
        .and_then(|n| n.billboard_mode)
        .map(BillboardMode::from_nif);

    Some(Arc::new(CachedNifImport {
        meshes: imported.meshes,
        // No collisions / lights / particles / animation clips on
        // the placeholder. Real branch geometry might emit a sphere
        // collision (tree-trunk collider) once the geometry tail is
        // decoded — follow-up sub-phase.
        collisions: Vec::new(),
        lights: Vec::new(),
        particle_emitters: Vec::new(),
        embedded_clip: None,
        placement_root_billboard,
        // SpeedTree `.spt` files carry no BSXFlags — they're a
        // separate format outside the NIF block hierarchy. #1214.
        bsx_flags: 0,
        // SpeedTree `.spt` placeholders have no NiAVObject root, so no
        // NiAVObject.flags to propagate. #1235 / LC-D1-NEW-01.
        root_flags: 0,
        // SpeedTree placeholders carry no flame markers — they're
        // pure billboard quads. Phase 18.
        flame_attach_offset: None,
        // SpeedTree `.spt` is a separate format with no BSConnectPoint
        // blocks. #1594.
        attach_points: None,
        child_attach_connections: None,
    }))
}

/// M47.0 Phase 3b — attach script-state components to a freshly-spawned
/// REFR's placement root. Three-stage lookup:
///
/// 1. `EsmIndex::base_record_script(base_form_id)` → SCPT form_id (or
///    `None` if the base record has no script).
/// 2. `EsmIndex.scripts.get(&script_form_id)` → `ScriptRecord` (or
///    `None` if the cross-ref dangled — a real data issue, but
///    survivable; logged at debug).
/// 3. `ScriptRegistry.lookup(&script.editor_id)` → spawn fn (or
///    `None` if M47.0 doesn't yet ship a handler for this script —
///    by far the most common miss path, ~1 256 / 1 257 vanilla FO3
///    scripts unregistered as of Phase 2).
///
/// Fall-through on every `None` is silent — see Phase 2 contract: M47.0
/// only ships hand-translated equivalents for ~5 R5-prototype scripts;
/// every other SCPT in vanilla content correctly reaches a "no spawner
/// registered" leaf and contributes nothing observable.
///
/// The function takes `&mut World` because the spawner mutates it (each
/// spawner does `query_mut::<…>().insert(entity, …)`). The
/// `ScriptRegistry` resource borrow is scoped tightly so the spawner
/// can re-borrow World freely.
/// Build a world-space [`TriggerVolume`](byroredux_scripting::TriggerVolume)
/// from a REFR's `XPRM` primitive + placement. `None` for non-containment
/// shapes (line / portal / plane).
///
/// `XPRM` bounds are Bethesda **z-up half-extents** — the Creation Kit
/// Primitive convention, consistent with `bhkBoxShape::aabb_half_extents`
/// and the `XMBO` half-extent bound. Permute to engine y-up (the position
/// swap is `[x, z, -y]`; extents are magnitudes, so the sign drops) and
/// bake the REFR scale in, since the volume is stored in world space. For
/// a sphere, `bounds[0]` is the radius (carried in `half_extents.x`).
fn trigger_volume_from_primitive(
    prim: &esm::cell::PrimitiveBounds,
    center: Vec3,
    rotation: Quat,
    scale: f32,
) -> Option<byroredux_scripting::TriggerVolume> {
    use byroredux_scripting::{TriggerShape, TriggerVolume};
    let shape = match prim.shape_type {
        1 => TriggerShape::Box,
        3 => TriggerShape::Sphere,
        _ => return None,
    };
    let half_extents = Vec3::new(
        prim.bounds[0].abs() * scale,
        prim.bounds[2].abs() * scale,
        prim.bounds[1].abs() * scale,
    );
    Some(TriggerVolume {
        center,
        half_extents,
        rotation,
        shape,
        // SCR-D6-NEW-02 / #1817 — `None`, not `false`. `false` is
        // indistinguishable from "known outside, primed" to
        // `trigger_detection_system`; a player who loads already
        // standing inside this volume would see a spurious
        // `OnTriggerEnterEvent` on frame 1. `None` lets the detection
        // system's first tick seed the real state silently instead.
        occupant_inside: None,
    })
}

/// Returns `true` when canonical behavior attached (either per-game arm
/// recognized the script) — the cell loader counts these for its summary.
fn attach_script_for_refr(
    world: &mut byroredux_core::ecs::world::World,
    entity: byroredux_core::ecs::EntityId,
    base_form_id: u32,
    index: &esm::records::EsmIndex,
    refr_script_instance: Option<&esm::records::script_instance::ScriptInstanceData>,
) -> bool {
    // Two mutually-exclusive per-game attach paths converge here. A
    // record carries either a pre-Skyrim `SCRI` → SCPT (Obscript, the
    // M47.0 registry path) or a Skyrim+ `VMAD` inline Papyrus block (the
    // M47.2 decompile path), never both in vanilla content. Run both —
    // each no-ops for the wrong era — and emit one `OnCellLoadEvent` if
    // either attached canonical behavior. `refr_script_instance` carries
    // the placed reference's OWN VMAD (#1737), attached additively with
    // the base record's by `attach_vmad_scripts`.
    let mut attached = attach_scpt_script(world, entity, base_form_id, index);
    attached |= attach_vmad_scripts(world, entity, base_form_id, index, refr_script_instance);

    if attached {
        // M47.0 Phase 5 — emit OnCellLoadEvent on the freshly-attached
        // entity so the script's first-tick init hook fires on the same
        // frame the cell loads. Mirrors Papyrus `OnLoad` semantics. The
        // marker is drained by `event_cleanup_system` at end-of-frame,
        // so each script sees exactly one OnCellLoad per cell entry.
        if let Some(mut q) = world.query_mut::<byroredux_scripting::OnCellLoadEvent>() {
            q.insert(entity, byroredux_scripting::OnCellLoadEvent);
        }
    }
    attached
}

/// FO3 / FNV / Oblivion path: resolve the base record's `SCRI` form id
/// to its SCPT editor id, look up a hand-written M47.0 spawner in the
/// [`ScriptRegistry`], and run it. Returns `true` when a spawner ran.
fn attach_scpt_script(
    world: &mut byroredux_core::ecs::world::World,
    entity: byroredux_core::ecs::EntityId,
    base_form_id: u32,
    index: &esm::records::EsmIndex,
) -> bool {
    let Some(script_form_id) = index.base_record_script(base_form_id) else {
        return false;
    };
    let Some(script) = index.scripts.get(&script_form_id) else {
        // SCPT cross-ref dangled. Pre-#443 the SCPT records weren't
        // parsed at all; post-#443 the index is populated, so a miss
        // here is genuinely a broken plugin / parser bug rather than
        // a missing-consumer story.
        log::debug!(
            "M47.0: SCRI {script_form_id:08X} on base {base_form_id:08X} not in index.scripts (dangling cross-ref)",
        );
        return false;
    };
    // Scope the registry borrow tightly — the spawn fn that comes back
    // is a function pointer (Copy), so we can drop the borrow before
    // invoking the spawner with `&mut World`.
    let spawn_fn = {
        let Some(registry) = world.try_resource::<byroredux_scripting::ScriptRegistry>() else {
            // Engine init didn't insert the registry — a programming
            // error. Log loudly the first time per process so the
            // misconfiguration surfaces during cell load instead of
            // silently disabling every script in the engine.
            log::error!(
                "M47.0: ScriptRegistry resource missing — \
                 byroredux_scripting::register and ScriptRegistry init \
                 must run before cell load. Script attach disabled."
            );
            return false;
        };
        registry.lookup(&script.editor_id)
    };
    let Some(spawn_fn) = spawn_fn else {
        // Most common miss path: a real SCPT with no Phase-2 handler.
        // log::trace! so it's available with `--RUST_LOG=trace` for
        // debugging without polluting INFO/DEBUG-level logs (a 1 200-
        // REFR cell load would emit ~1 200 misses).
        log::trace!(
            "M47.0: no spawner registered for SCPT editor_id '{}' (form {:08X})",
            script.editor_id,
            script_form_id,
        );
        return false;
    };
    spawn_fn(world, entity);
    log::debug!(
        "M47.0: attached script '{}' (SCPT {:08X}) to entity {entity:?} via base {base_form_id:08X}",
        script.editor_id,
        script_form_id,
    );
    true
}

/// Skyrim+ path: for each script named in the record's `VMAD`, fetch its
/// compiled `.pex` from the script archive, decompile it, and run it
/// through the recognizer chain
/// ([`byroredux_scripting::translate_pex`]). A recognized script inserts
/// its canonical ECS behavior; an unrecognized or missing one is a
/// silent miss. Returns `true` when at least one script was recognized.
///
/// Two VMAD sources are processed **additively** (SCR-D7-01 / #1737):
/// `refr_script_instance` is the placed reference's OWN `VMAD` (Skyrim+
/// objectReference override scripts — a uniquely-scripted lever / quest
/// item / activator), and the base record's `VMAD` is looked up from
/// `index`. Both are attached, mirroring Bethesda's additive
/// objectReference semantics; on a name collision the REFR's script wins
/// (it is processed first and the base copy is skipped), so a placement
/// can override a base script's binding without dropping the rest.
///
/// `owning_quest` is `None` here: base-record-attached scripts (lever,
/// door, trap activators; scripted containers / NPCs) bind their quest
/// through a VMAD `Quest` property, not an alias. Alias-attached scripts
/// (which need the owning quest id) flow through the quest-alias attach
/// path, not this one.
fn attach_vmad_scripts(
    world: &mut byroredux_core::ecs::world::World,
    entity: byroredux_core::ecs::EntityId,
    base_form_id: u32,
    index: &esm::records::EsmIndex,
    refr_script_instance: Option<&esm::records::script_instance::ScriptInstanceData>,
) -> bool {
    // Fast-out before any per-REFR work when no `--scripts-bsa` was
    // supplied (the common case for mesh-only / FO3-FNV launches): no
    // archive means every `.pex` lookup would miss anyway.
    let have_archive = world
        .try_resource::<crate::asset_provider::ScriptProvider>()
        .is_some_and(|p| !p.is_empty());
    if !have_archive {
        return false;
    }
    let base_script_instance = index.base_record_script_instance(base_form_id);
    // Nothing to attach if neither the REFR nor its base record carries a
    // VMAD. Pre-#1737 this returned on the base lookup alone, so a REFR
    // with its own override VMAD over a script-less base attached nothing.
    if base_script_instance.is_none() && refr_script_instance.is_none() {
        return false;
    }
    let game = index.game;
    let mut any = false;
    // REFR-own VMAD first so it wins name collisions; then the base record.
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for script_instance in [refr_script_instance, base_script_instance]
        .into_iter()
        .flatten()
    {
        for script in &script_instance.scripts {
            if !seen.insert(script.name.as_str()) {
                // Same script name already attached from the REFR override —
                // skip the base record's copy (REFR wins).
                continue;
            }
            // Scope the provider borrow: extract the owned `.pex` bytes,
            // then drop the resource read before the `&mut World` spawn.
            let bytes = {
                let provider = world.resource::<crate::asset_provider::ScriptProvider>();
                provider.extract_pex(&script.name)
            };
            let Some(bytes) = bytes else {
                log::trace!(
                    "M47.2: .pex '{}' not in script archive (base {base_form_id:08X})",
                    script.name,
                );
                continue;
            };
            // `script_instance` borrows `index` / the placed ref (not
            // `world`), so it stays valid across the `&mut World` spawn.
            match byroredux_scripting::translate_pex(&bytes, game, Some(script_instance), None) {
                Some(recognized) => {
                    log::debug!(
                        "M47.2: recognized '{}' from .pex '{}' on base {base_form_id:08X} → entity {entity:?}",
                        recognized.archetype,
                        script.name,
                    );
                    (recognized.spawn)(world, entity);
                    any = true;
                }
                None => {
                    log::trace!(
                        "M47.2: .pex '{}' decompiled but unrecognized (base {base_form_id:08X})",
                        script.name,
                    );
                }
            }
        }
    }
    any
}

/// Phase 17 — attach a [`LightFlicker`] component when the light's
/// FNAM flags request flicker / pulse animation. No-op for static
/// lights (the common case for sun proxies, exterior fill, mage
/// spells), so the per-frame `animate_lights_system` iterates only
/// the candle / torch / chandelier slice via sparse-set membership.
///
/// `base_translation` is captured from `ref_pos` so the animator can
/// restore the un-jittered position each frame and the movement
/// amplitude doesn't accumulate. Seeds `phase_offset_secs` from the
/// entity id so a room full of identical candles doesn't flicker in
/// lockstep — deterministic per session, scene-stable across cell
/// reloads since EntityIds reset on cell unload.
fn attach_light_flicker_if_needed(
    world: &mut World,
    entity: byroredux_core::ecs::EntityId,
    ld: &byroredux_plugin::esm::cell::LightData,
    base_translation: byroredux_core::math::Vec3,
) {
    const FLICKER_MASK: u32 =
        LIGHT_FLAG_FLICKER | LIGHT_FLAG_FLICKER_SLOW | LIGHT_FLAG_PULSE | LIGHT_FLAG_PULSE_SLOW;
    if ld.flags & FLICKER_MASK == 0 {
        return;
    }
    // Pre-Skyrim LIGH records truncate after byte 16 — `period_secs`
    // reads as 0.0 then. Fall back to 0.5 s (the Skyrim vanilla
    // default for candle FNAM authoring) so flicker still
    // visibly fires on those records.
    let period_secs = if ld.period_secs > 0.0 {
        ld.period_secs
    } else {
        0.5
    };
    // EntityId-derived phase offset in [0, period). The wrap-around
    // is automatic because the animator computes `phase = (t +
    // phase_offset) / period` mod 1. Cheap, deterministic, no RNG.
    let phase_offset_secs =
        (entity.wrapping_mul(2654435761) as f32 / u32::MAX as f32) * period_secs;
    world.insert(
        entity,
        LightFlicker {
            period_secs,
            intensity_amplitude: ld.intensity_amplitude,
            movement_amplitude: ld.movement_amplitude,
            base_translation: [base_translation.x, base_translation.y, base_translation.z],
            phase_offset_secs,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::string::StringPool;

    /// `XPRM` box primitive → world-space `TriggerVolume`: bounds are
    /// z-up half-extents, permuted to engine y-up `[x, z, y]` and scaled
    /// by the REFR scale. Center / rotation pass through verbatim.
    #[test]
    fn trigger_volume_from_box_primitive_permutes_and_scales() {
        let prim = esm::cell::PrimitiveBounds {
            bounds: [10.0, 20.0, 30.0], // z-up: x=10, y=20, z=30
            color: [1.0, 0.0, 0.0],
            unknown: 0.0,
            shape_type: 1, // Box
        };
        let center = Vec3::new(100.0, 5.0, -50.0);
        let v = trigger_volume_from_primitive(&prim, center, Quat::IDENTITY, 2.0)
            .expect("box primitive yields a volume");
        assert_eq!(v.shape, byroredux_scripting::TriggerShape::Box);
        assert_eq!(v.center, center);
        // y-up half-extents = [x, z, y] * scale = [10, 30, 20] * 2.
        assert_eq!(v.half_extents, Vec3::new(20.0, 60.0, 40.0));
    }

    /// Sphere primitive (shape 3): `bounds[0]` is the radius, carried in
    /// `half_extents.x` and scaled.
    #[test]
    fn trigger_volume_from_sphere_primitive_uses_radius() {
        let prim = esm::cell::PrimitiveBounds {
            bounds: [15.0, 0.0, 0.0],
            color: [0.0; 3],
            unknown: 0.0,
            shape_type: 3, // Sphere
        };
        let v = trigger_volume_from_primitive(&prim, Vec3::ZERO, Quat::IDENTITY, 3.0)
            .expect("sphere primitive yields a volume");
        assert_eq!(v.shape, byroredux_scripting::TriggerShape::Sphere);
        assert_eq!(v.half_extents.x, 45.0); // 15 * 3
    }

    /// Non-containment shapes (line / portal / plane) don't become
    /// trigger volumes — they're not solids a point can be inside.
    #[test]
    fn trigger_volume_rejects_non_containment_shapes() {
        for shape_type in [2u32, 4, 5] {
            let prim = esm::cell::PrimitiveBounds {
                bounds: [1.0, 1.0, 1.0],
                color: [0.0; 3],
                unknown: 0.0,
                shape_type,
            };
            assert!(
                trigger_volume_from_primitive(&prim, Vec3::ZERO, Quat::IDENTITY, 1.0).is_none(),
                "shape_type {shape_type} must not yield a containment volume",
            );
        }
    }

    /// The Skyrim+ `.pex` attach arm fast-outs when no script archive is
    /// supplied: no `--scripts-bsa` (no `ScriptProvider` resource, or an
    /// empty one) → returns `false` and attaches nothing, without touching
    /// the index. Pins the "no archive → clean miss" contract.
    #[test]
    fn attach_vmad_scripts_no_ops_without_a_script_archive() {
        use byroredux_core::ecs::world::World;
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        let index = esm::records::EsmIndex::default();
        let entity = world.spawn();

        // No ScriptProvider resource at all.
        assert!(!attach_vmad_scripts(&mut world, entity, 0x0000_1234, &index, None));

        // An empty ScriptProvider (flag absent) → same clean miss.
        world.insert_resource(crate::asset_provider::build_script_provider(&[]));
        assert!(!attach_vmad_scripts(&mut world, entity, 0x0000_1234, &index, None));
    }

    /// `attach_script_for_refr` on a base form with no SCPT and no VMAD
    /// emits no `OnCellLoadEvent` — the marker fires only when canonical
    /// behavior actually attaches (both per-game arms decline cleanly on
    /// an empty index).
    #[test]
    fn attach_script_for_refr_emits_no_event_when_nothing_attaches() {
        use byroredux_core::ecs::world::World;
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        let index = esm::records::EsmIndex::default();
        let entity = world.spawn();

        attach_script_for_refr(&mut world, entity, 0x0000_1234, &index, None);
        assert!(
            !world.has::<byroredux_scripting::OnCellLoadEvent>(entity),
            "no script attached → no OnCellLoadEvent",
        );
    }

    /// #1495 / REN2-10 — the RT absolute-space precision ceiling guard.
    /// Empty cells must not trip (bounds left at ±INF); vanilla-scale
    /// extents are clear; a mega-worldspace past 2^20 is flagged with its
    /// extent; the bound is inclusive.
    #[test]
    fn worldspace_extent_ceiling_predicate() {
        // Empty cell — bounds never accumulated, still ±INF → None.
        assert_eq!(
            worldspace_extent_over_rt_ceiling(
                Vec3::splat(f32::INFINITY),
                Vec3::splat(f32::NEG_INFINITY),
            ),
            None,
        );
        // Vanilla exterior corner (~±233k, Skyrim Tamriel) — clear.
        assert_eq!(
            worldspace_extent_over_rt_ceiling(
                Vec3::splat(-233_000.0),
                Vec3::splat(233_000.0),
            ),
            None,
        );
        // Mega-worldspace past 2^20 — flagged, returns the max |coord|.
        assert_eq!(
            worldspace_extent_over_rt_ceiling(
                Vec3::new(-1_200_000.0, 0.0, 0.0),
                Vec3::new(50.0, 50.0, 50.0),
            ),
            Some(1_200_000.0),
        );
        // Inclusive at the ceiling itself.
        assert!(worldspace_extent_over_rt_ceiling(
            Vec3::ZERO,
            Vec3::splat(RT_ABSOLUTE_PRECISION_CEILING),
        )
        .is_some());
    }

    /// Minimal vanilla-shaped `.spt` byte stream: 20-byte magic + one
    /// section marker tag + an out-of-range u32 sentinel so the walker
    /// stops cleanly at the geometry-tail boundary.
    fn minimal_spt_bytes() -> Vec<u8> {
        // Magic header (`E8 03 00 00 0C 00 00 00 __IdvSpt_02_`).
        let mut bytes = vec![0xE8, 0x03, 0x00, 0x00, 0x0C, 0x00, 0x00, 0x00];
        bytes.extend_from_slice(b"__IdvSpt_02_");
        // Single bare-marker tag (`1002` is in the bare set).
        bytes.extend_from_slice(&1002u32.to_le_bytes());
        // Tail sentinel — out-of-range u32 so the walker stops cleanly.
        bytes.extend_from_slice(&0x4E25u32.to_le_bytes());
        bytes
    }

    /// #994 regression — the SpeedTree importer's placeholder root
    /// authors `billboard_mode = Some(BsRotateAboutUp)`. The cell-loader
    /// adapter must surface that as `CachedNifImport::placement_root_billboard`
    /// so `spawn_placed_instances` can attach a `Billboard` ECS component
    /// to the placement root. Pre-fix the field was dropped on the
    /// floor; trees rendered as static quads.
    #[test]
    fn parse_and_import_spt_surfaces_billboard_mode_on_cache_entry() {
        let bytes = minimal_spt_bytes();
        let mut pool = StringPool::new();
        let cached = parse_and_import_spt(&bytes, "trees\\test.spt", None, &mut pool)
            .expect("minimal spt parses through the importer");
        assert_eq!(
            cached.placement_root_billboard,
            Some(BillboardMode::BsRotateAboutUp),
            "SPT placeholder must flag the placement root as a yaw-billboard",
        );
        assert_eq!(cached.meshes.len(), 1, "single placeholder quad");
    }

    /// #1798 / D7-NEW-01 — `load_references` has no live-Vulkan-context
    /// / ESM-fixture-free unit-test surface (it needs a real
    /// `VulkanContext`, BSA-backed `TextureProvider`, and parsed ESM
    /// indices), so a live call-through test is impractical here — the
    /// same constraint documented on `draw_frame_guards_on_empty_
    /// framebuffers_before_acquire` in the renderer crate. A static
    /// source assertion instead pins that both NPC dispatch call sites
    /// are wrapped in the `npc_spawn_wall` timing this fix added, so a
    /// future refactor can't silently drop the only visibility this
    /// loop has into its own per-NPC spawn cost.
    #[test]
    fn npc_spawn_call_sites_are_wall_clock_timed() {
        let full_src = include_str!("references.rs");
        // Slice off `mod tests` itself — this test's own source text
        // contains the literal strings it's searching for, which would
        // otherwise self-match and inflate the counts.
        let src = &full_src[..full_src
            .find("#[cfg(test)]\nmod tests {")
            .expect("references.rs must have a #[cfg(test)] mod tests block")];

        let runtime_facegen_pos = src
            .find("crate::npc_spawn::spawn_npc_entity(")
            .expect("load_references must call spawn_npc_entity");
        let prebaked_facegen_pos = src
            .find("crate::npc_spawn::spawn_prebaked_npc_entity(")
            .expect("load_references must call spawn_prebaked_npc_entity");

        let timer_starts: Vec<_> = src.match_indices("let spawn_t0 = std::time::Instant::now();").collect();
        assert_eq!(
            timer_starts.len(),
            2,
            "expected exactly one spawn_t0 timer per NPC dispatch arm (runtime-FaceGen \
             and pre-baked-FaceGen); #1798 / D7-NEW-01"
        );
        assert!(
            timer_starts.iter().any(|&(pos, _)| pos < runtime_facegen_pos),
            "runtime-FaceGen dispatch (spawn_npc_entity) must be preceded by a spawn_t0 timer"
        );
        assert!(
            timer_starts.iter().any(|&(pos, _)| pos < prebaked_facegen_pos),
            "pre-baked-FaceGen dispatch (spawn_prebaked_npc_entity) must be preceded by a spawn_t0 timer"
        );
        assert!(
            src.contains("npc_spawn_wall +="),
            "each timed dispatch must accumulate into npc_spawn_wall"
        );
        assert!(
            src.contains("{:.1}ms wall in spawn calls"),
            "the accumulated NPC-spawn wall time must be surfaced in the \
             end-of-cell summary log, or the cost stays invisible (the exact \
             gap #1798 reports)"
        );
    }
}
