//! Per-cell reference loading: walk PlacedRefs, expand PKIN/SCOL
//! containers, parse NIFs/SPTs through the registry cache, and dispatch
//! to `spawn_placed_instances` for actual entity creation.
//!
//! The bulk of cell load time lives here — parsing NIFs (cache miss
//! path), expanding container placements, resolving base records,
//! and committing the per-cell NifImportRegistry deltas.

use byroredux_core::ecs::{GlobalTransform, LightSource, Transform, World};
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
    pub(super) mesh_count: usize,
    pub(super) center: Vec3,
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
) -> RefLoadResult {
    let mut entity_count = 0;
    // Number of mesh-bearing entities (those that receive a
    // `MeshHandle` insert in `spawn_placed_instances`). Distinct from
    // `entity_count` which also sums LIGH-only / effect-sprite-light
    // entities that carry no renderable mesh. See #477.
    let mut mesh_entity_count = 0usize;
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

    let mut stat_miss = 0u32;
    let mut stat_hit = 0u32;
    let mut enable_skipped = 0u32;
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
    // ships only `_male\idle.kf` and uses it for both genders; Phase
    // 2.x can add a per-gender cache if a future game variant ships
    // separate clips.
    let idle_clip_handle = if game.has_kf_animations() {
        crate::npc_spawn::load_idle_clip(world, tex_provider, game, crate::npc_spawn::Gender::Male)
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

        // Convert the outer REFR's placement (Z-up Bethesda → Y-up
        // renderer). For normal REFRs this is the spawn transform; for
        // SCOL REFRs it's the parent transform the child placements
        // compose against.
        let outer_pos = Vec3::new(
            placed_ref.position[0],
            placed_ref.position[2],
            -placed_ref.position[1],
        );
        let outer_rot = euler_zup_to_quat_yup_refr(
            placed_ref.rotation[0],
            placed_ref.rotation[1],
            placed_ref.rotation[2],
        );
        let outer_scale = placed_ref.scale;

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
                            ..Default::default()
                        },
                    );
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

            let count = spawn_placed_instances(
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
            );
            entity_count += count;
            mesh_entity_count += count;
        }
    }

    let center = (bounds_min + bounds_max) * 0.5;
    let dims = bounds_max - bounds_min;
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
        "  Bounds: min=[{:.0},{:.0},{:.0}] max=[{:.0},{:.0},{:.0}] size=[{:.0},{:.0},{:.0}] center=[{:.0},{:.0},{:.0}]",
        bounds_min.x, bounds_min.y, bounds_min.z,
        bounds_max.x, bounds_max.y, bounds_max.z,
        dims.x, dims.y, dims.z,
        center.x, center.y, center.z,
    );
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
            "  {} NPCs spawned via {} path (sample: {}{})",
            npc_spawned,
            path_label,
            sample_str,
            trunc,
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
        log::warn!(
            "  {} base forms not found in statics table (sample: {}{}){}",
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
        // Mesh-bearing entities spawned this load. Pre-#477 this was
        // `reg.len() - cache_size_at_entry` — "newly parsed NIFs" —
        // which reported 0 on a repeat load of the same cell despite
        // spawning hundreds of entities. The new count is stable
        // across repeat loads and matches the rasterizer draw budget
        // (modulo instancing). The parse-work telemetry moved to the
        // `this_cell_unique` log line above; `NifImportRegistry.hits`
        // / `.misses` remain the source of truth for cache analysis.
        mesh_count: mesh_entity_count,
        center,
    }
}

/// Parse + import a NIF scene once. Returns `None` on parse failure
/// or when the scene has zero useful geometry. All per-block parse
/// warnings and the truncation message (if any) are emitted exactly
/// once per unique NIF at this step — subsequent placements read
/// from the cache without re-parsing. See runtime-spam incident from
/// the `AnvilHeinrichOakenHallsHouse` trace.
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

    // BSXFlags bit 5 (0x20) marks the entire NIF as an editor marker —
    // invisible in-game objects like XMarker, PrisonMarker, etc.
    let bsx = byroredux_nif::import::extract_bsx_flags(&scene);
    if bsx & 0x20 != 0 {
        log::debug!("Skipping editor marker NIF '{}'", label);
        return None;
    }

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
    Some(Arc::new(CachedNifImport {
        meshes,
        collisions,
        lights,
        particle_emitters,
        embedded_clip,
    }))
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

    let params = byroredux_spt::SptImportParams {
        leaf_texture_override,
        bounds,
        wind,
        form_id,
    };

    let imported = byroredux_spt::import_spt_scene(&scene, &params, pool);

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
    }))
}
