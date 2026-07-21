//! Per-REFR ECS component attach helpers — attach-points / child-attach
//! connections, trigger volumes, script fragments, and light flicker. Split
//! out of the original `cell_loader/references.rs` (#1877).

//! Per-cell reference loading: walk PlacedRefs, expand PKIN/SCOL
//! containers, parse NIFs/SPTs through the registry cache, and dispatch
//! to `spawn_placed_instances` for actual entity creation.
//!
//! The bulk of cell load time lives here — parsing NIFs (cache miss
//! path), expanding container placements, resolving base records,
//! and committing the per-cell NifImportRegistry deltas.

use byroredux_core::ecs::{LightFlicker, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::esm;

/// Intern an [`ImportedAttachPoint`] list into the `AttachPoints` ECS
/// component (#985 / #1594). Attach-point names + parent-bone tags become
/// `FixedString` handles so the equip-time `AttachPoints::find` lookup is an
/// integer compare. Transforms arrive already Y-up from the extractor.
pub(crate) fn attach_points_component(
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

/// Map an [`ImportedFurnitureMarker`] list into the `Furniture` ECS
/// component (M41.5 Phase B). Pure data — no string interning: offsets
/// arrive already Y-up from the extractor; heading is kept in Gamebryo
/// source space (see `FurnitureMarker` docs). Caller guarantees a
/// non-empty slice (a marker-less furniture attaches no component).
///
/// The sole translate-boundary site that resolves each marker's
/// [`FurnitureMarkerKind`] from the raw `AnimationType` (#2010 /
/// NIFAL-D4-01) — gameplay consumers (`systems::sandbox::is_sit_marker`)
/// read the already-resolved `kind` instead of re-deriving an era
/// discriminant from `heading_z_radians`'s presence.
pub(crate) fn furniture_component(
    imported: &[byroredux_nif::import::ImportedFurnitureMarker],
) -> byroredux_core::ecs::components::Furniture {
    use byroredux_core::ecs::components::{Furniture, FurnitureMarker, FurnitureMarkerKind};
    Furniture {
        markers: imported
            .iter()
            .map(|m| FurnitureMarker {
                local_offset: m.offset,
                heading_z_radians: m.heading_z_radians,
                animation_type: m.animation_type,
                kind: match m.animation_type {
                    2 => FurnitureMarkerKind::Sleep,
                    3 => FurnitureMarkerKind::Lean,
                    // 1 = explicit Skyrim+ Sit; 0 = legacy (Oblivion/FO3/FNV,
                    // no AnimationType authored at all) — v0 default, the
                    // dominant furniture kind in target cells.
                    _ => FurnitureMarkerKind::Sit,
                },
            })
            .collect(),
    }
}

/// Intern an [`ImportedChildAttachConnections`] into the
/// `ChildAttachConnections` ECS component (#985 / #1594).
pub(crate) fn child_attach_connections_component(
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
pub(super) fn trigger_volume_from_primitive(
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
pub(super) fn attach_script_for_refr(
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

/// #1359 / D6-06a — attach an `Inventory` ECS component to a CONT-based
/// REFR from its typed [`ContainerRecord`](esm::records::ContainerRecord).
/// Container REFRs already spawn a visual mesh via the `statics` lookup
/// (CONT is dual-dispatched into both `index.cells.statics` and
/// `index.containers` at parse time); this closes the gap where the
/// typed record's inventory contents were parsed but never consumed,
/// leaving containers empty in the ECS. Returns `true` when a
/// `ContainerRecord` was found and attached (regardless of whether its
/// `contents` list was empty — an intentionally-empty container is
/// still a container).
///
/// Scope: this wires the DATA layer only. Interaction (loot prompt,
/// opening a container, `open_sound` playback) is a separate
/// interaction-layer milestone — see the issue for the full context.
pub(super) fn attach_container_inventory(
    world: &mut byroredux_core::ecs::world::World,
    entity: byroredux_core::ecs::EntityId,
    base_form_id: u32,
    index: &esm::records::EsmIndex,
) -> bool {
    use byroredux_core::ecs::components::{Inventory, ItemStack};

    let Some(cont) = index.containers.get(&base_form_id) else {
        return false;
    };
    let mut inventory = Inventory::new();
    for entry in &cont.contents {
        // Negative parsed counts are remove-from-inventory deltas, not
        // live state — clamp at runtime per `ItemStack::count` docs
        // (matches the same normalization `npc_spawn.rs` applies to
        // NPC/actor inventories built from the same CNTO entry shape).
        let runtime_count = entry.count.max(0) as u32;
        if runtime_count == 0 {
            continue;
        }
        inventory.push(ItemStack::new(entry.item_form_id, runtime_count));
    }
    world.insert(entity, inventory);
    true
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
pub(super) fn attach_vmad_scripts(
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
pub(crate) fn attach_light_flicker_if_needed(
    world: &mut World,
    entity: byroredux_core::ecs::EntityId,
    ld: &byroredux_plugin::esm::cell::LightData,
    base_translation: byroredux_core::math::Vec3,
    animation_flags: u32,
) {
    if animation_flags == 0 {
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
            animation_flags,
            period_secs,
            intensity_amplitude: ld.intensity_amplitude,
            movement_amplitude: ld.movement_amplitude,
            base_translation: [base_translation.x, base_translation.y, base_translation.z],
            phase_offset_secs,
        },
    );
}

#[cfg(test)]
mod furniture_component_tests {
    use super::*;
    use byroredux_core::ecs::components::FurnitureMarkerKind;
    use byroredux_nif::import::ImportedFurnitureMarker;

    fn imported(heading: Option<f32>, anim: u16) -> ImportedFurnitureMarker {
        ImportedFurnitureMarker {
            offset: [0.0, 0.0, 0.0],
            heading_z_radians: heading,
            animation_type: anim,
        }
    }

    /// Regression for #2010 / NIFAL-D4-01 — `furniture_component` is the
    /// single translate boundary that resolves each marker's
    /// `FurnitureMarkerKind` from the raw `AnimationType`, so gameplay
    /// consumers never need to re-derive it from `heading_z_radians`.
    #[test]
    fn resolves_kind_from_animation_type_at_the_boundary() {
        let furn = furniture_component(&[
            imported(Some(0.0), 1), // Skyrim+ sit
            imported(Some(0.0), 2), // Skyrim+ sleep
            imported(Some(0.0), 3), // Skyrim+ lean
            imported(None, 0),      // legacy — no AnimationType, v0 default
        ]);
        assert_eq!(furn.markers[0].kind, FurnitureMarkerKind::Sit);
        assert_eq!(furn.markers[1].kind, FurnitureMarkerKind::Sleep);
        assert_eq!(furn.markers[2].kind, FurnitureMarkerKind::Lean);
        assert_eq!(
            furn.markers[3].kind,
            FurnitureMarkerKind::Sit,
            "legacy markers with no AnimationType default to Sit (v0)"
        );
    }
}

#[cfg(test)]
mod container_inventory_tests {
    use super::*;
    use byroredux_core::ecs::components::Inventory;
    use byroredux_plugin::esm::records::{ContainerRecord, EsmIndex, InventoryEntry};

    fn container_with_contents(form_id: u32, entries: &[(u32, i32)]) -> ContainerRecord {
        ContainerRecord {
            form_id,
            editor_id: String::new(),
            full_name: String::new(),
            model_path: String::new(),
            weight: 0.0,
            flags: 0,
            open_sound: 0,
            close_sound: 0,
            script_form_id: 0,
            script_instance: None,
            contents: entries
                .iter()
                .map(|&(item_form_id, count)| InventoryEntry {
                    item_form_id,
                    count,
                })
                .collect(),
        }
    }

    #[test]
    fn attaches_inventory_from_container_record() {
        let mut index = EsmIndex::default();
        index
            .containers
            .insert(0x1234, container_with_contents(0x1234, &[(0xAAAA, 5), (0xBBBB, 1)]));

        let mut world = World::new();
        let entity = world.spawn();

        assert!(attach_container_inventory(&mut world, entity, 0x1234, &index));

        let inv = world.get::<Inventory>(entity).expect("Inventory attached");
        assert_eq!(inv.items.len(), 2);
        assert_eq!(inv.items[0].base_form_id, 0xAAAA);
        assert_eq!(inv.items[0].count, 5);
        assert_eq!(inv.items[1].base_form_id, 0xBBBB);
        assert_eq!(inv.items[1].count, 1);
    }

    /// Negative CNTO counts are remove-from-inventory deltas on disk, never
    /// live state — they must be dropped, not underflow into a huge `u32`.
    #[test]
    fn negative_counts_are_dropped() {
        let mut index = EsmIndex::default();
        index
            .containers
            .insert(0x1, container_with_contents(0x1, &[(0x10, -3), (0x20, 2)]));

        let mut world = World::new();
        let entity = world.spawn();
        attach_container_inventory(&mut world, entity, 0x1, &index);

        let inv = world.get::<Inventory>(entity).expect("Inventory attached");
        assert_eq!(inv.items.len(), 1);
        assert_eq!(inv.items[0].base_form_id, 0x20);
    }

    #[test]
    fn empty_container_still_attaches() {
        let mut index = EsmIndex::default();
        index.containers.insert(0x1, container_with_contents(0x1, &[]));

        let mut world = World::new();
        let entity = world.spawn();

        assert!(attach_container_inventory(&mut world, entity, 0x1, &index));
        assert!(world.get::<Inventory>(entity).expect("Inventory attached").is_empty());
    }

    #[test]
    fn non_container_base_form_returns_false_and_attaches_nothing() {
        let index = EsmIndex::default();
        let mut world = World::new();
        let entity = world.spawn();

        assert!(!attach_container_inventory(&mut world, entity, 0xFFFF, &index));
        assert!(world.get::<Inventory>(entity).is_none());
    }
}
