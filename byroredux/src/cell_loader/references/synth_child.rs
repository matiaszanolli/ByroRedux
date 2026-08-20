//! Synthetic-child spawning for SCOL / PKIN-expanded REFRs, plus the
//! quest-reference identity + script-attach helpers shared with the main
//! reference loader.
//!
//! Split out of `references/mod.rs` (#2409 / TD1-006): `spawn_synth_child`
//! alone was 488 LOC at cognitive complexity 31/25, and the file had crossed
//! 2000 LOC. Contents moved verbatim — only the visibility of the four items
//! `mod.rs` still calls was widened, and `super::refr::` re-anchored a level
//! deeper.

use super::*;

fn water_current_volume_from_ref(
    placed_ref: &esm::cell::PlacedRef,
    position: Vec3,
    scale: f32,
) -> Option<WaterCurrentVolume> {
    let velocity = placed_ref.water_velocity?;
    let primitive = placed_ref.primitive?;
    let speed = velocity[0].hypot(velocity[1]);
    if !speed.is_finite() || speed <= 1.0e-5 || !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let extents = [
        primitive.bounds[0].abs() * scale,
        primitive.bounds[2].abs() * scale,
        primitive.bounds[1].abs() * scale,
    ];
    if !extents
        .iter()
        .all(|extent| extent.is_finite() && *extent > 0.0)
    {
        return None;
    }
    Some(WaterCurrentVolume {
        volume: WaterVolume {
            min: [
                position.x - extents[0],
                position.y - extents[1],
                position.z - extents[2],
            ],
            max: [
                position.x + extents[0],
                position.y + extents[1],
                position.z + extents[2],
            ],
        },
        flow: WaterFlow::new([velocity[0], 0.0, -velocity[1]], speed),
    })
}

pub(super) fn stamp_quest_reference(
    world: &mut World,
    entity: EntityId,
    placed_ref: &esm::cell::PlacedRef,
    load_order: &[String],
) {
    let plugin_name = plugin_for_form_id(placed_ref.form_id, load_order).unwrap_or("Engine.esm");
    let placement = world.resource_mut::<FormIdPool>().intern(FormIdPair {
        plugin: PluginId::from_filename(plugin_name),
        local: LocalFormId(placed_ref.form_id),
    });
    world.insert(entity, FormIdComponent(placement));
    world.insert(
        entity,
        byroredux_scripting::SceneAliasCandidate {
            reference_form_id: placed_ref.form_id,
            base_form_id: placed_ref.base_form_id,
            linked_refs: placed_ref
                .linked_refs
                .iter()
                .map(|link| (link.keyword, link.target))
                .collect(),
            location_ref_types: placed_ref.location_ref_types.clone(),
        },
    );
    byroredux_scripting::mark_scene_actor_bindings_dirty(world);
}

/// Spawn the identity-only entity for a REFR that produced no 3D, carrying a
/// transform so it stays a *rankable* alias candidate.
///
/// The transform is the load-bearing part, not decoration: `resolve_alias_
/// bindings` ranks distance-anchored aliases (`closest_to_alias`, or
/// `ALIAS_FLAG_CLOSEST` anchored on the player) with
/// `world.get::<GlobalTransform>(entity)?` *inside a `filter_map`*, so a
/// candidate without one is dropped from the `min_by` entirely rather than
/// merely ranked last. An alias whose only candidates are transform-less
/// stubs silently stays unfilled — no log line, no error.
///
/// `pub(crate)` since #2664: the worldspace persistent-cell loader
/// (`cell_loader::exterior`) has the same "logical actor identity, no 3D"
/// case for remote / spawn-less persistent `ACHR`s, and used to open-code a
/// copy of [`stamp_quest_reference`] that omitted the transform.
pub(crate) fn spawn_logical_quest_reference(
    world: &mut World,
    placed_ref: &esm::cell::PlacedRef,
    load_order: &[String],
    position: Vec3,
    rotation: Quat,
    scale: f32,
) -> EntityId {
    let entity = world.spawn();
    world.insert(entity, Transform::new(position, rotation, scale));
    world.insert(entity, GlobalTransform::new(position, rotation, scale));
    stamp_quest_reference(world, entity, placed_ref, load_order);
    entity
}

/// #3016 — policy for every spawn branch that calls this: **always call it,
/// never gate the call itself on `is_primary_synth`.** `base_form_id` (i.e.
/// `child_form_id`) is the leaf base record for *this* synthetic child, so
/// its own SCRI/VMAD belongs to this child, not to the SCOL/PKIN's first
/// child alone — it must attach once per spawned entity, mirroring the fact
/// that each of these branches spawns a real, independent entity per child.
///
/// This is orthogonal to `refr_script_instance`: the *outer REFR's own*
/// VMAD (passed in here as `refr_script_instance`) is a property of the one
/// REFR being expanded, not of each child, so callers pre-gate it to `None`
/// past the first child via `refr_script_instance_for_synth_child` (#2026)
/// before it ever reaches this function — gating the call itself would
/// double-gate the outer-REFR half and additionally, wrongly, drop the
/// per-child base-record half.
pub(super) fn attach_quest_reference_script(
    world: &mut World,
    entity: EntityId,
    base_form_id: u32,
    record_index: &byroredux_plugin::esm::records::EsmIndex,
    refr_script_instance: Option<&esm::records::script_instance::ScriptInstanceData>,
    accum: &mut RefLoadAccum,
) {
    if attach_script_for_refr(
        world,
        entity,
        base_form_id,
        record_index,
        refr_script_instance,
    ) {
        accum.scripts_recognized += 1;
    }
}

/// Dispatch one synthetic child placement (SCOL/PKIN-expanded or the lone
/// default) by record kind — NPC actor, invisible trigger volume, light-only
/// LIGH, marker/FX skip, or the main static-mesh spawn — accumulating its
/// telemetry into `accum`. Split verbatim out of [`load_references`] (#2058);
/// each former `continue` (skip this child) is now an early `return`.
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_synth_child(
    accum: &mut RefLoadAccum,
    world: &mut World,
    ctx: &mut VulkanContext,
    cell: &CellLoadCtx,
    mat_provider: Option<&mut MaterialProvider>,
    placed_ref: &esm::cell::PlacedRef,
    refr_overlay: &Option<super::super::refr::RefrTextureOverlay>,
    child_form_id: u32,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    refr_script_instance: Option<&esm::records::script_instance::ScriptInstanceData>,
    is_primary_synth: bool,
) {
    let &CellLoadCtx {
        index,
        record_index,
        game,
        tex_provider,
        load_order,
    } = cell;
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
                // this basis.
                || refr_script_instance.is_some();
    // #3015 — the whole branch is gated on `is_primary_synth`, not just
    // `stamp_quest_reference` below. `placed_ref.primitive` (the `XPRM`
    // this volume is built from) is a field on the outer REFR itself —
    // authored once, like `teleport`/`lock` (#3098) — not per synthetic
    // child. Composing it with a non-primary child's own transform would
    // manufacture a trigger volume that was never authored, at a
    // position nothing placed it at, and inflate `accum.trigger_volumes`
    // once per child instead of once per REFR. A non-primary child whose
    // own base record independently carries a script still gets it
    // attached — just through whichever ordinary per-child branch below
    // actually matches its own record kind (LIGH / static mesh / etc.,
    // #3016), not through this REFR-level trigger path.
    if trigger_volume_should_spawn_for_synth_child(is_primary_synth, has_mesh, has_script) {
        if let Some(prim) = placed_ref.primitive.as_ref() {
            if let Some(volume) = trigger_volume_from_primitive(prim, ref_pos, ref_rot, ref_scale) {
                let entity = world.spawn();
                world.insert(entity, Transform::new(ref_pos, ref_rot, ref_scale));
                world.insert(entity, GlobalTransform::new(ref_pos, ref_rot, ref_scale));
                world.insert(entity, volume);
                stamp_quest_reference(world, entity, placed_ref, load_order);
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
            if is_primary_synth {
                let entity = spawn_logical_quest_reference(
                    world, placed_ref, load_order, ref_pos, ref_rot, ref_scale,
                );
                attach_quest_reference_script(
                    world,
                    entity,
                    child_form_id,
                    record_index,
                    refr_script_instance,
                    accum,
                );
                accum.entity_count += 1;
                accum.bounds_min = accum.bounds_min.min(ref_pos);
                accum.bounds_max = accum.bounds_max.max(ref_pos);
            }
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
            // #2439 / NIFAL-D2-01 — geometry half (kind/direction/
            // outer_angle) of the same translation boundary the shadow
            // flags below already route through.
            let geometry = crate::systems::translate_light(ld, game, ref_rot);
            world.insert(
                entity,
                LightSource::from_legacy_world_units(
                    light_radius_or_default(ld.radius),
                    ld.color,
                    ld.flags,
                    ld.falloff_exponent,
                    geometry.kind,
                    geometry.direction,
                    geometry.outer_angle,
                    crate::systems::canonical_light_shadow_flags(game, ld.flags),
                ),
            );
            let animation_flags = crate::systems::canonical_light_animation_flags(game, ld.flags);
            attach_light_flicker_if_needed(world, entity, ld, ref_pos, animation_flags);
            // #3016 — `stamp_quest_reference` (outer-REFR identity, #2026)
            // stays gated to the first synthetic child, but the script
            // attach below is NOT: `child_form_id`'s own base-record
            // SCRI/VMAD belongs to *this* child's base record, and this
            // branch spawns a real entity for every synthetic child, not
            // just the first. `refr_script_instance` is already pre-gated
            // to `None` past the first child by `refr_script_instance_for_
            // synth_child`, so the outer-REFR-VMAD half of #2026 still only
            // binds once — only the base-record half now runs per child,
            // matching the trigger-volume / main-static-mesh / actor
            // branches below and in `mod.rs`.
            if is_primary_synth {
                stamp_quest_reference(world, entity, placed_ref, load_order);
            }
            attach_quest_reference_script(
                world,
                entity,
                child_form_id,
                record_index,
                refr_script_instance,
                accum,
            );
            accum.entity_count += 1;
        } else if is_primary_synth {
            let entity = spawn_logical_quest_reference(
                world, placed_ref, load_order, ref_pos, ref_rot, ref_scale,
            );
            attach_quest_reference_script(
                world,
                entity,
                child_form_id,
                record_index,
                refr_script_instance,
                accum,
            );
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
        if is_primary_synth {
            let entity = spawn_logical_quest_reference(
                world, placed_ref, load_order, ref_pos, ref_rot, ref_scale,
            );
            attach_quest_reference_script(
                world,
                entity,
                child_form_id,
                record_index,
                refr_script_instance,
                accum,
            );
            accum.entity_count += 1;
        }
        return;
    }

    if model_lower.contains("fxlightrays") || model_lower.contains("fxlight") {
        if let Some(ref ld) = stat.light_data {
            let entity = world.spawn();
            world.insert(entity, Transform::from_translation(ref_pos));
            world.insert(entity, GlobalTransform::new(ref_pos, Quat::IDENTITY, 1.0));
            // #2439 / NIFAL-D2-01 — geometry half of the translation
            // boundary. Uses the REFR's OWN authored `ref_rot`, not the
            // `Quat::IDENTITY` this fxlight sprite entity's own transform
            // carries above — the light's cone direction follows the
            // authored placement rotation regardless of how the visual
            // effect sprite is oriented.
            let geometry = crate::systems::translate_light(ld, game, ref_rot);
            world.insert(
                entity,
                LightSource::from_legacy_world_units(
                    light_radius_or_default(ld.radius),
                    ld.color,
                    ld.flags,
                    ld.falloff_exponent,
                    geometry.kind,
                    geometry.direction,
                    geometry.outer_angle,
                    crate::systems::canonical_light_shadow_flags(game, ld.flags),
                ),
            );
            let animation_flags = crate::systems::canonical_light_animation_flags(game, ld.flags);
            attach_light_flicker_if_needed(world, entity, ld, ref_pos, animation_flags);
            // #3016 — same split as the LIGH-only branch above: outer-REFR
            // identity stays gated to the first synthetic child, the
            // per-child base-record script attach does not.
            if is_primary_synth {
                stamp_quest_reference(world, entity, placed_ref, load_order);
            }
            attach_quest_reference_script(
                world,
                entity,
                child_form_id,
                record_index,
                refr_script_instance,
                accum,
            );
            accum.entity_count += 1;
        } else if is_primary_synth {
            let entity = spawn_logical_quest_reference(
                world, placed_ref, load_order, ref_pos, ref_rot, ref_scale,
            );
            attach_quest_reference_script(
                world,
                entity,
                child_form_id,
                record_index,
                refr_script_instance,
                accum,
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
    //
    // #3038 — the registry key MUST come from `canonical_model_path_key`,
    // not a bare `.to_ascii_lowercase()` of the (already meshes\-prefixed)
    // `model_path` above. The exterior-streaming loader (`streaming.rs`)
    // builds the same key from the same `canonical_model_path_key` call
    // against the raw `stat.model_path`; two independent inline
    // normalisations (this one used to prefix before lowercasing, the
    // streaming one didn't) produced two keys for one asset.
    let cache_key = canonical_model_path_key(&stat.model_path);
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
    let Some(cached) = cached else {
        if is_primary_synth {
            let entity = spawn_logical_quest_reference(
                world, placed_ref, load_order, ref_pos, ref_rot, ref_scale,
            );
            attach_quest_reference_script(
                world,
                entity,
                child_form_id,
                record_index,
                refr_script_instance,
                accum,
            );
            accum.entity_count += 1;
        }
        return;
    };

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
    // #2439 (NIFAL-D2-01) — geometry half of the same translation
    // boundary the animation/shadow flags below already route through.
    let light_geometry = stat
        .light_data
        .as_ref()
        .map(|ld| crate::systems::translate_light(ld, game, ref_rot))
        .unwrap_or_default();
    let (placement_root, count, spawn_stats) = spawn_placed_instances(
        world,
        ctx,
        &cached,
        tex_provider,
        ref_pos,
        ref_rot,
        ref_scale,
        stat.light_data.as_ref(),
        stat.light_data
            .as_ref()
            .map(|ld| crate::systems::canonical_light_animation_flags(game, ld.flags))
            .unwrap_or(0),
        stat.light_data
            .as_ref()
            .map(|ld| crate::systems::canonical_light_shadow_flags(game, ld.flags))
            .unwrap_or(0),
        light_geometry.kind,
        light_geometry.direction,
        light_geometry.outer_angle,
        refr_overlay.as_ref(),
        clip_handle,
        stat.record_type.render_layer(),
        Some(cache_key.as_str()),
        is_primary_synth.then_some(placement_pair),
        is_primary_synth.then_some(placed_ref.teleport).flatten(),
        // #3098 — same primary-synth gating as `teleport` above: XLOC is
        // REFR-level data, not per-synthetic-child, so only the first
        // SCOL/PKIN-expansion child carries it.
        is_primary_synth.then_some(placed_ref.lock).flatten(),
    );
    accum.entity_count += count;
    accum.packed_collision_fallbacks += spawn_stats.packed_collision_fallbacks;
    accum.unresolved_packed_collision += spawn_stats.unresolved_packed_collision;
    if is_primary_synth {
        stamp_quest_reference(world, placement_root, placed_ref, load_order);
        if let Some(current) = water_current_volume_from_ref(placed_ref, ref_pos, ref_scale) {
            world.insert(placement_root, current);
        }
    }

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
    // #3016 — the call itself is deliberately NOT gated on
    // `is_primary_synth`: see `attach_quest_reference_script`'s doc
    // comment for the shared policy.
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

/// #3015 — whether `spawn_synth_child`'s invisible-trigger branch should
/// spawn a `TriggerVolume` for this synthetic child. Extracted so the
/// gating decision is unit-testable without the full Vulkan spawn path
/// (mirrors [`stamp_visible_when_distant`] just below).
///
/// `is_primary_synth` gates the whole branch, not just the identity stamp
/// it contains: the volume is built from `placed_ref.primitive` (the
/// outer REFR's own `XPRM`), authored once per REFR like `teleport`/
/// `lock` (#3098), never per synthetic child. A non-primary child can
/// still independently satisfy `has_mesh`/`has_script` from its OWN base
/// record, but that must not manufacture a second, differently-placed
/// copy of a trigger volume that was authored exactly once.
pub(super) fn trigger_volume_should_spawn_for_synth_child(
    is_primary_synth: bool,
    has_mesh: bool,
    has_script: bool,
) -> bool {
    is_primary_synth && !has_mesh && has_script
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
pub(super) fn stamp_visible_when_distant(
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
pub(super) fn refr_script_instance_for_synth_child(
    synth_idx: usize,
    script_instance: Option<&esm::records::script_instance::ScriptInstanceData>,
) -> Option<&esm::records::script_instance::ScriptInstanceData> {
    if synth_idx == 0 {
        script_instance
    } else {
        None
    }
}
