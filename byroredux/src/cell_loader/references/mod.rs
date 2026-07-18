//! Per-cell reference loading: walk PlacedRefs, expand PKIN/SCOL
//! containers, parse NIFs/SPTs through the registry cache, and dispatch
//! to `spawn_placed_instances` for actual entity creation.
//!
//! The bulk of cell load time lives here — parsing NIFs (cache miss
//! path), expanding container placements, resolving base records,
//! and committing the per-cell NifImportRegistry deltas.

use byroredux_core::ecs::{EntityId, GlobalTransform, LightSource, Transform, World};
use byroredux_core::form_id::{FormIdPair, LocalFormId, PluginId};
use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::esm;
use byroredux_renderer::VulkanContext;
use std::collections::HashMap;
use std::sync::Arc;

use crate::asset_provider::{MaterialProvider, TextureProvider};
use crate::components::VisibleWhenDistant;

use super::euler::euler_zup_to_quat_yup_refr;
use super::load_order::{self, plugin_for_form_id};
use super::nif_import_registry::{CachedNifImport, NifImportRegistry};
use super::refr::{build_refr_texture_overlay, expand_pkin_placements, expand_scol_placements};
use super::spawn::{light_radius_or_default, spawn_placed_instances};

mod attach;
mod import;

use attach::{
    attach_container_inventory, attach_light_flicker_if_needed, attach_script_for_refr,
    trigger_volume_from_primitive,
};
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
    let mut door_pos: Option<Vec3> = None;

    let mut enable_skipped = 0u32;
    // #1188 — count REFRs skipped because the CK absorbed them into a
    // precombined `_oc.nif`. Surfaced in the end-of-cell summary so an
    // operator can spot a missing precombined-spawn step (would manifest
    // as "absorbed=N but precombined_spawned=0" pair below).
    let mut absorbed_skipped = 0u32;
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
    // the `SandboxSitClip` resource, and clear stale seat reservations from
    // the previous cell (entity ids reset on unload). `None` for
    // Skyrim+/Havok games → those actors are not seated.
    let sit_clip = crate::npc_spawn::load_sit_clip(world, tex_provider, game);
    if let Some(mut r) = world.try_resource_mut::<crate::components::SandboxSitClip>() {
        r.0 = sit_clip;
    }
    if let Some(mut r) = world.try_resource_mut::<crate::components::SeatReservations>() {
        r.0.clear();
    }

    // Per-call NIF-cache accumulators (this_call_hits / misses / pending_new
    // / pending_hits / pending_clip_handles) live on `accum` and are committed
    // to `NifImportRegistry` in a single `resource_mut` borrow after the loop
    // rather than a write lock per REFR (#523 / #635 / #544). See the
    // `RefLoadAccum` field docs for each one's role.
    let cell = CellLoadCtx {
        index,
        record_index,
        npcs,
        races,
        game,
        tex_provider,
        load_order,
        idle_pool: &idle_pool,
    };
    let mut accum = RefLoadAccum::new();
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
        for (synth_idx, (child_form_id, ref_pos, ref_rot, ref_scale)) in
            synth_refs.into_iter().enumerate()
        {
            let refr_script_instance = refr_script_instance_for_synth_child(
                synth_idx,
                placed_ref.script_instance.as_ref(),
            );
            spawn_synth_child(
                &mut accum,
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
            );
        }
    }
    let RefLoadAccum {
        entity_count,
        bounds_min,
        bounds_max,
        stat_miss,
        stat_hit,
        nif_not_found,
        nif_not_found_sample,
        stat_miss_sample,
        npc_spawned,
        npc_spawned_sample,
        npc_spawn_wall,
        scripts_recognized,
        trigger_volumes,
        containers_attached,
        this_call_hits,
        this_call_misses,
        pending_new,
        pending_hits,
        pending_clip_handles,
    } = accum;

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
    //
    // #1854 — commit `pending_clip_handles` BEFORE the `pending_new`
    // insert loop, not after. Every `pending_clip_handles` key is also a
    // `pending_new` key (both are populated together, see the `#544`
    // comment at the parse site above), so this ordering is safe. It
    // matters when a single batched commit inserts more keys than
    // `BYRO_NIF_CACHE_MAX`, so the insert loop's own LRU eviction can
    // evict an earlier key from THIS SAME loop: `NifImportRegistry::
    // insert`'s eviction path only releases a clip handle it finds
    // already in `self.clip_handles` — committing clip handles second
    // meant an evicted-this-loop key's handle hadn't been committed yet,
    // so eviction found nothing to free, and the later `set_clip_handle`
    // then planted a handle for a key no longer resident in the cache —
    // never released, leaking the `AnimationClipRegistry` slot. Not
    // reachable on vanilla FNV today (no single cell has anywhere near
    // 2048 unique models), but the ordering bug is real for any future
    // caller batching more.
    let (this_cell_hits, this_cell_misses, this_cell_unique, lifetime_hit_rate, freed_clip_handles) = {
        let mut reg = world.resource_mut::<NifImportRegistry>();
        let mut freed: Vec<u32> = Vec::new();
        reg.accumulate_hits(this_call_hits);
        reg.accumulate_misses(this_call_misses);
        reg.touch_keys(pending_hits.iter().map(String::as_str));
        // #544 — commit per-call clip handles into the process-lifetime
        // registry. Future cell loads of the same NIF reach the
        // memoised handle through `clip_handle_for` without
        // re-converting the channel arrays.
        for (key, handle) in pending_clip_handles {
            reg.set_clip_handle(key, handle);
        }
        for (key, entry) in pending_new {
            // #863 — accumulate LRU-evicted clip handles from each
            // insert; the AnimationClipRegistry release happens after
            // we drop the NifImportRegistry write lock.
            freed.extend(reg.insert(key, entry));
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
    if containers_attached > 0 {
        // #1359 / D6-06a — how many CONT REFRs now carry an `Inventory`
        // populated from their typed `ContainerRecord`.
        log::info!(
            "  {} containers attached an Inventory component",
            containers_attached
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
    npcs: &'a HashMap<u32, byroredux_plugin::esm::records::NpcRecord>,
    races: &'a HashMap<u32, byroredux_plugin::esm::records::RaceRecord>,
    game: byroredux_plugin::esm::reader::GameKind,
    tex_provider: &'a TextureProvider,
    load_order: &'a [String],
    idle_pool: &'a [u32],
}

/// Dispatch one synthetic child placement (SCOL/PKIN-expanded or the lone
/// default) by record kind — NPC actor, invisible trigger volume, light-only
/// LIGH, marker/FX skip, or the main static-mesh spawn — accumulating its
/// telemetry into `accum`. Split verbatim out of [`load_references`] (#2058);
/// each former `continue` (skip this child) is now an early `return`.
#[allow(clippy::too_many_arguments)]
fn spawn_synth_child(
    accum: &mut RefLoadAccum,
    world: &mut World,
    ctx: &mut VulkanContext,
    cell: &CellLoadCtx,
    mut mat_provider: Option<&mut MaterialProvider>,
    placed_ref: &esm::cell::PlacedRef,
    refr_overlay: &Option<super::refr::RefrTextureOverlay>,
    child_form_id: u32,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    refr_script_instance: Option<&esm::records::script_instance::ScriptInstanceData>,
) {
    let &CellLoadCtx {
        index,
        record_index,
        npcs,
        races,
        game,
        tex_provider,
        load_order,
        idle_pool,
    } = cell;
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
        accum.bounds_min = accum.bounds_min.min(ref_pos);
        accum.bounds_max = accum.bounds_max.max(ref_pos);
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
                idle_pool,
                ref_pos,
                ref_rot,
                ref_scale,
                record_index,
            );
            accum.npc_spawn_wall += spawn_t0.elapsed();
            if spawned.is_some() {
                accum.npc_spawned += 1;
                if accum.npc_spawned_sample.len() < 8
                    && !accum.npc_spawned_sample.contains(&child_form_id)
                {
                    accum.npc_spawned_sample.push(child_form_id);
                }
                accum.entity_count += 1;
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
            let plugin = load_order::plugin_for_form_id(child_form_id, load_order).unwrap_or("");
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
            accum.npc_spawn_wall += spawn_t0.elapsed();
            if spawned.is_some() {
                accum.npc_spawned += 1;
                if accum.npc_spawned_sample.len() < 8
                    && !accum.npc_spawned_sample.contains(&child_form_id)
                {
                    accum.npc_spawned_sample.push(child_form_id);
                }
                accum.entity_count += 1;
            }
        }
        return;
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
                // #2026 — gated on `refr_script_instance`, not the raw
                // `placed_ref.script_instance`, so only the first
                // synthetic child of a SCOL/PKIN expansion qualifies on
                // this basis; the base-record checks above still apply
                // to every child (they're keyed by `child_form_id`).
                || refr_script_instance.is_some();
    if !has_mesh && has_script {
        if let Some(prim) = placed_ref.primitive.as_ref() {
            if let Some(volume) = trigger_volume_from_primitive(prim, ref_pos, ref_rot, ref_scale) {
                let entity = world.spawn();
                world.insert(entity, Transform::new(ref_pos, ref_rot, ref_scale));
                world.insert(entity, GlobalTransform::new(ref_pos, ref_rot, ref_scale));
                world.insert(entity, volume);
                if attach_script_for_refr(
                    world,
                    entity,
                    child_form_id,
                    record_index,
                    refr_script_instance,
                ) {
                    accum.scripts_recognized += 1;
                }
                accum.trigger_volumes += 1;
                accum.bounds_min = accum.bounds_min.min(ref_pos);
                accum.bounds_max = accum.bounds_max.max(ref_pos);
                accum.entity_count += 1;
                return;
            }
        }
    }

    let stat = match index.statics.get(&child_form_id) {
        Some(s) => {
            accum.stat_hit += 1;
            s
        }
        None => {
            accum.stat_miss += 1;
            // Collect a bounded sample so the summary line can
            // surface actual FormIDs without pulling down a
            // full RUST_LOG=debug run. Linear dedup is fine
            // for 20 entries. See #386.
            if accum.stat_miss_sample.len() < 20 && !accum.stat_miss_sample.contains(&child_form_id)
            {
                accum.stat_miss_sample.push(child_form_id);
            }
            log::debug!("REFR base {:08X} not in statics table", child_form_id);
            return;
        }
    };

    // Update bounds from the (possibly SCOL-composed) placement.
    accum.bounds_min = accum.bounds_min.min(ref_pos);
    accum.bounds_max = accum.bounds_max.max(ref_pos);

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
            accum.entity_count += 1;
        }
        return;
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
        return;
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
            accum.entity_count += 1;
        }
        return;
    }

    let model_path = if model_lower.starts_with("meshes\\") || model_lower.starts_with("meshes/") {
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
    let cached = if let Some(entry) = accum.pending_new.get(&cache_key).cloned() {
        accum.this_call_hits += 1;
        entry
    } else {
        let reg_entry = {
            let reg = world.resource::<NifImportRegistry>();
            reg.get(&cache_key).cloned()
        };
        match reg_entry {
            Some(entry) => {
                accum.this_call_hits += 1;
                // Mark for LRU touch at the end-of-load batched
                // commit so frequently-revisited meshes don't
                // get evicted under `BYRO_NIF_CACHE_MAX`. The
                // batched flush keeps the read path on a shared
                // lock — preserves the #523 invariant.
                accum.pending_hits.push(cache_key.clone());
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
                        let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
                        if is_spt {
                            let tree_record = record_index.trees.get(&child_form_id);
                            parse_and_import_spt(&d, &model_path, tree_record, &mut pool)
                        } else {
                            parse_and_import_nif(
                                &d,
                                &model_path,
                                mat_provider,
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
                        accum.nif_not_found += 1;
                        if accum.nif_not_found_sample.len() < 5 {
                            accum.nif_not_found_sample.push(model_path.clone());
                        }
                        None
                    }
                };
                accum.this_call_misses += 1;
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
                            let clip = crate::anim_convert::convert_nif_clip(nif_clip, &mut pool);
                            drop(pool);
                            let mut clip_reg = world
                                .resource_mut::<byroredux_core::animation::AnimationClipRegistry>(
                            );
                            clip_reg.add(clip)
                        };
                        accum.pending_clip_handles.insert(cache_key.clone(), handle);
                    }
                }
                accum.pending_new.insert(cache_key.clone(), parsed.clone());
                parsed
            }
        }
    };
    let Some(cached) = cached else { return };

    // #544 — embedded animation-clip handle for this REFR's
    // model. Three-tier lookup mirrors the cache:
    //   1. `pending_clip_handles` — registered earlier in this
    //      call's slow path.
    //   2. `NifImportRegistry::clip_handle_for` — registered
    //      by an earlier cell load. Read-only / shared lock.
    //   3. `None` — the cached NIF authored no controllers.
    // Subsequent REFRs of the same model in this same load
    // hit case (1) and never touch the registry write path.
    let clip_handle = accum
        .pending_clip_handles
        .get(&cache_key)
        .copied()
        .or_else(|| {
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
    accum.entity_count += count;

    // #1889 / EXAL §5.2 — materialise the base record's
    // Visible-When-Distant flag onto the placement root.
    stamp_visible_when_distant(world, placement_root, stat.visible_when_distant);

    // #1359 / D6-06a — CONT REFRs already spawn a mesh via the
    // `statics` lookup above; attach the typed record's inventory
    // contents so the data layer is no longer absent.
    if attach_container_inventory(world, placement_root, child_form_id, record_index) {
        accum.containers_attached += 1;
    }

    // M47.0 Phase 3b — attach script state to the placement
    // root. `child_form_id` is the leaf base record (SCOL /
    // PKIN children each get their own; non-expanded REFRs
    // pass placed_ref.base_form_id verbatim). Index → SCPT →
    // editor_id → ScriptRegistry → spawner; misses fall
    // through silently per Phase 2's "unregistered scripts are
    // common" contract. See docs/engine/m47-0-design.md.
    // #2026 — `refr_script_instance` (the outer REFR's own VMAD,
    // gated to the first synthetic child only) replaces the raw
    // `placed_ref.script_instance` here.
    if attach_script_for_refr(
        world,
        placement_root,
        child_form_id,
        record_index,
        refr_script_instance,
    ) {
        accum.scripts_recognized += 1;
    }
}

/// #1889 / EXAL §5.2 — materialise the base record's Visible-When-Distant flag
/// onto the placement root. This is the per-record signal a full-model LOD cull
/// reads; see the [`VisibleWhenDistant`] doc comment for why it has no
/// render-time consumer under the current conservative streaming-ring rule (the
/// ring already guarantees a full model and its LOD proxy never coexist, #1866).
///
/// Extracted from the spawn loop (#1890) so the flag→marker plumbing is
/// unit-testable without the full Vulkan spawn path; the record→flag half is
/// pinned in `crates/plugin/src/esm/cell/tests/addn_stat.rs`.
fn stamp_visible_when_distant(
    world: &mut World,
    placement_root: EntityId,
    visible_when_distant: bool,
) {
    if visible_when_distant {
        world.insert(placement_root, VisibleWhenDistant);
    }
}

/// #2026 / SCR-D7-NEW2-01 — the outer REFR's own VMAD is a property of
/// that single REFR, not of each synthetic child a SCOL/PKIN expansion
/// fans it out into. Only the first synthetic child (`synth_idx == 0`)
/// gets it; the rest get `None`, so a VMAD-scripted SCOL/PKIN's behavior
/// (including the `OnCellLoadEvent` that follows a successful attach)
/// instantiates once per REFR, not once per decorative piece.
///
/// Extracted from the spawn loop so the gating is unit-testable without
/// the full Vulkan spawn path — mirrors `stamp_visible_when_distant`
/// just above.
fn refr_script_instance_for_synth_child(
    synth_idx: usize,
    script_instance: Option<&esm::records::script_instance::ScriptInstanceData>,
) -> Option<&esm::records::script_instance::ScriptInstanceData> {
    if synth_idx == 0 {
        script_instance
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Test-only symbols not referenced by production code in this module
    // (they'd warn as unused at file scope). #1877 split.
    use super::attach::attach_vmad_scripts;
    use byroredux_core::ecs::BillboardMode;
    use byroredux_core::string::StringPool;

    /// #1890 / DELTA-01 — the spawn-path half of the VWD chain: a base record
    /// whose `visible_when_distant` flag is set ends with a `VisibleWhenDistant`
    /// marker on its placement root, and an unflagged one does not. Complements
    /// the record→flag pin in `esm/cell/tests/addn_stat.rs` (#1889), closing the
    /// parse→spawn plumbing the audit flagged as untested.
    #[test]
    fn stamp_visible_when_distant_marks_only_flagged_roots() {
        let mut world = World::new();

        let flagged = world.spawn();
        stamp_visible_when_distant(&mut world, flagged, true);
        let unflagged = world.spawn();
        stamp_visible_when_distant(&mut world, unflagged, false);

        let q = world
            .query::<VisibleWhenDistant>()
            .expect("VisibleWhenDistant storage exists after one insert");
        assert!(
            q.get(flagged).is_some(),
            "a VWD-flagged base record must stamp the marker on its placement root",
        );
        assert!(
            q.get(unflagged).is_none(),
            "an unflagged base record must NOT carry the marker",
        );
    }

    /// #2026 / SCR-D7-NEW2-01 — a VMAD-carrying SCOL/PKIN outer REFR must
    /// attach its own script instance to the first synthetic child only;
    /// every later child gets `None`, not a copy. Pre-fix, every
    /// synthetic child received the same `Some(&script_instance)`, so a
    /// SCOL/PKIN expansion would instantiate the outer REFR's behavior
    /// (including `OnCellLoadEvent`) once per decorative piece instead
    /// of once per REFR.
    #[test]
    fn refr_script_instance_attaches_to_first_synth_child_only() {
        let script_instance = esm::records::script_instance::ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![esm::records::script_instance::ScriptInstance {
                name: "MyTriggerScript".to_string(),
                status: 0,
                properties: Vec::new(),
            }],
        };

        assert_eq!(
            refr_script_instance_for_synth_child(0, Some(&script_instance)),
            Some(&script_instance),
            "the first synthetic child (idx 0) must receive the outer REFR's VMAD",
        );
        for idx in 1..5 {
            assert_eq!(
                refr_script_instance_for_synth_child(idx, Some(&script_instance)),
                None,
                "synthetic child {idx} must NOT receive a copy of the outer REFR's VMAD",
            );
        }
        // A REFR with no VMAD at all: every child (including the first)
        // correctly gets `None` — nothing to propagate in the first place.
        assert_eq!(refr_script_instance_for_synth_child(0, None), None);
    }

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

    /// #1742 / SCR-D7-02 — the audit flagged (verify-not-confirmed) that
    /// `half_extents` is permuted z-up→y-up (`[x, z, y]`) while `rotation`
    /// passes through verbatim, and worried the two might not be in the
    /// same frame for a non-axis-aligned trigger box.
    ///
    /// They ARE the same frame. `rotation` here is exactly
    /// `euler_zup_to_quat_yup_refr`'s output — the same conversion every
    /// other REFR placement in this loader uses — derived specifically so
    /// it rotates y-up-frame vectors consistently with `zup_to_yup_pos`'s
    /// position conversion. This test proves it end-to-end without
    /// leaning on that self-consistency: it rotates a point in the box's
    /// OWN z-up local frame (on the z-up +Y face — `bounds[1]`, one of the
    /// two permuted axes) using Bethesda's independently-implemented
    /// clockwise convention, converts the ROTATED point to y-up via the
    /// canonical `zup_to_yup_pos` (not via `rotation`, and not via
    /// anything `trigger_volume_from_primitive` touches), and checks
    /// `TriggerVolume::contains` classifies points just inside/outside
    /// that rotated face correctly. Deliberately probes `bounds[1]`
    /// (z-up Y), not `bounds[0]` (z-up X, untouched by the permutation) —
    /// a swapped `[x, z, y]` → `[x, y, z]` regression wouldn't move this
    /// axis and this test would falsely pass.
    #[test]
    fn rotated_box_trigger_composes_rotation_in_same_frame_as_permuted_extents() {
        use std::f32::consts::FRAC_PI_2;

        // z-up: x=10, y=20 (the axis under test), z=30.
        let prim = esm::cell::PrimitiveBounds {
            bounds: [10.0, 20.0, 30.0],
            color: [0.0; 3],
            unknown: 0.0,
            shape_type: 1, // Box
        };
        let center = Vec3::ZERO;
        // A pure 90° yaw (Bethesda's "rz" Euler component) — same helper,
        // same shipping mode (1 = CW+ZYX), as every other placed REFR.
        let ref_rot = euler_zup_to_quat_yup_refr(0.0, 0.0, FRAC_PI_2);
        let v = trigger_volume_from_primitive(&prim, center, ref_rot, 1.0)
            .expect("box primitive yields a volume");

        // Bethesda's clockwise rotation about z-up's Z axis, applied
        // directly to a z-up local point — independent of `ref_rot`.
        let cw_rotate_zup_by_z = |p: Vec3, theta: f32| {
            let (s, c) = theta.sin_cos();
            Vec3::new(p.x * c + p.y * s, -p.x * s + p.y * c, p.z)
        };
        let just_inside_zup = cw_rotate_zup_by_z(Vec3::new(0.0, 19.9, 0.0), FRAC_PI_2);
        let just_outside_zup = cw_rotate_zup_by_z(Vec3::new(0.0, 20.1, 0.0), FRAC_PI_2);
        let just_inside_world = Vec3::from_array(byroredux_core::math::coord::zup_to_yup_pos(
            just_inside_zup.to_array(),
        ));
        let just_outside_world = Vec3::from_array(byroredux_core::math::coord::zup_to_yup_pos(
            just_outside_zup.to_array(),
        ));

        assert!(
            v.contains(just_inside_world),
            "a point 0.1 units inside the box's rotated z-up +Y face must test \
             inside (just_inside_world = {just_inside_world:?})"
        );
        assert!(
            !v.contains(just_outside_world),
            "a point 0.1 units outside the same rotated face must test outside \
             (just_outside_world = {just_outside_world:?})"
        );
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
        assert!(!attach_vmad_scripts(
            &mut world,
            entity,
            0x0000_1234,
            &index,
            None
        ));

        // An empty ScriptProvider (flag absent) → same clean miss.
        world.insert_resource(crate::asset_provider::build_script_provider(&[]));
        assert!(!attach_vmad_scripts(
            &mut world,
            entity,
            0x0000_1234,
            &index,
            None
        ));
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
            worldspace_extent_over_rt_ceiling(Vec3::splat(-233_000.0), Vec3::splat(233_000.0),),
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

    /// #1820 / SPT-NEW-01 — pins the logged sanity check
    /// `parse_and_import_spt` now computes via `detect_variant`. The
    /// call itself can't be observed without a log-capturing dependency
    /// (none exists in this workspace), so this asserts the value the
    /// production code path would log for the same fixture bytes
    /// `parse_and_import_spt_surfaces_billboard_mode_on_cache_entry`
    /// exercises above — a vanilla `__IdvSpt_02_`-prefixed stream
    /// resolves to `V5Fnv` per `detect_variant`'s documented default.
    #[test]
    fn minimal_spt_fixture_detects_as_v5fnv_variant() {
        let bytes = minimal_spt_bytes();
        assert_eq!(
            byroredux_spt::detect_variant(&bytes),
            byroredux_spt::SpeedTreeVariant::V5Fnv,
            "the same bytes parse_and_import_spt's logged sanity check \
             receives must resolve to V5Fnv, matching MAGIC_HEAD's \
             documented default",
        );
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
        let full_src = include_str!("mod.rs");
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

        let timer_starts: Vec<_> = src
            .match_indices("let spawn_t0 = std::time::Instant::now();")
            .collect();
        assert_eq!(
            timer_starts.len(),
            2,
            "expected exactly one spawn_t0 timer per NPC dispatch arm (runtime-FaceGen \
             and pre-baked-FaceGen); #1798 / D7-NEW-01"
        );
        assert!(
            timer_starts
                .iter()
                .any(|&(pos, _)| pos < runtime_facegen_pos),
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
