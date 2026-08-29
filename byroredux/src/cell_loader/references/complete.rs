//! Completion half of the budgeted reference loader: fold the accumulated
//! per-REFR statistics into a spawn point and the cell-load report.
//!
//! Split out of `load_references_budgeted` (#2409 / TD1-006), which was
//! 766 LOC at cognitive complexity 58/25 — the busiest touch point in cell
//! loading. This is the straight-line tail that runs once, after the
//! per-REFR `while` loop has drained: it destructures the job + accumulator,
//! picks the spawn point (first door > bbox centroid > origin), emits the
//! diagnostic summary, and flushes the cell's queued texture uploads.
//!
//! Extracting the tail rather than the loop is deliberate: the loop carries
//! the resumable-yield contract (`job.next_ref` / `next_synth` /
//! `current_ref_synth`), and moving it would put that state machine behind a
//! call boundary. The tail owns none of it — it only runs when the loop has
//! finished — so the split cannot change resume behaviour. Contents moved
//! verbatim.

use super::*;
use crate::cell_loader::load_order::LoadOrder;

/// See the module doc. Consumes `job` because it destructures the
/// accumulator out of it; only reachable once the REFR loop is drained.
pub(super) fn complete_reference_load(
    job: Box<ReferenceLoadJob>,
    game: byroredux_plugin::esm::reader::GameKind,
    world: &mut World,
    ctx: &mut VulkanContext,
    label: &str,
    load_order: &LoadOrder,
) -> ReferenceLoadProgress {
    let ReferenceLoadJob {
        cache_hits_at_entry,
        cache_misses_at_entry,
        cache_size_at_entry,
        door_pos,
        enable_skipped,
        absorbed_skipped,
        absorbed_interactive_retained,
        npc_pending,
        npc_pending_sample,
        accum,
        ..
    } = *job;
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
        packed_collision_fallbacks,
        unresolved_packed_collision,
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
        // Sorted so the ticks this cell hands out are assigned in a
        // run-stable order (#3387). `pending_hits` became a `HashSet`,
        // whose iteration order varies per process; every key here lands
        // at the top of the LRU either way, so the order only decides
        // which of *this cell's own* hits is evicted first — reachable
        // only when one cell's unique-model count exceeds the whole
        // cache cap, but non-determinism in an eviction path is not
        // worth the saved sort over ~150 keys.
        let mut touched: Vec<&str> = pending_hits.iter().map(String::as_str).collect();
        touched.sort_unstable();
        reg.touch_keys(touched);
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
    if packed_collision_fallbacks > 0 || unresolved_packed_collision > 0 {
        log::info!(
            "  Packed collision compatibility: {} placements approximated, {} unresolved",
            packed_collision_fallbacks,
            unresolved_packed_collision,
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
    if absorbed_interactive_retained > 0 {
        log::info!(
            "  {} XPRI REFRs retained individually because their record type \
             carries gameplay/runtime identity that CSG cannot preserve",
            absorbed_interactive_retained,
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
    flush_pending_cell_textures(ctx);

    ReferenceLoadProgress::Complete(RefLoadResult {
        entity_count,
        center,
    })
}
