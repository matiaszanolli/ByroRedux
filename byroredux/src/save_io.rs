//! M45 Save/Load — binary-side wiring for the [`byroredux_save`] crate.
//!
//! The save crate is engine-agnostic: it knows how to snapshot a
//! [`World`] given a [`SaveRegistry`], but only the binary sees every
//! component type, so the binary populates the registry
//! ([`build_save_registry`]) and installs it as a resource.
//!
//! ## What's wired
//!
//! - `save [slot]` — runs the pre-save validation pass, snapshots the
//!   live world, and writes a CRC-protected file atomically. Read-only
//!   against the World, so it's a plain [`ConsoleCommand`].
//! - `save.info <slot>` — decodes + verifies a slot (magic / version /
//!   CRC / schema fingerprint) and prints what it contains, without
//!   touching the live world.
//!
//! ## What's deferred (M45.1)
//!
//! Applying a decoded snapshot to a *live* Vulkan session means clearing
//! the world and re-instantiating GPU meshes/textures, BLAS, physics
//! bodies, and the camera from the restored components — a renderer
//! re-sync that's its own milestone-sized integration. The destructive
//! [`restore_world`](byroredux_save::restore_world) path is fully
//! implemented and tested headlessly in the save crate; `save.info` is
//! the safe in-engine surface until the re-instantiation lands.

use std::path::PathBuf;

use byroredux_core::console::{CommandOutput, ConsoleCommand};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::World;
use byroredux_save::validate::validate_world;
use byroredux_save::{disk, encode, save_world, SaveRegistry, Snapshot};

/// The **mutable game-state** component columns a live load overlays onto
/// a reloaded cell, keyed by stable form id. Deliberately excludes
/// structural/identity columns (`Name` / `Parent` / `Children` / the
/// form-id key) — the reloaded cell already owns those; only post-spawn
/// *changes* (moved objects, inventory, equip, light/anim/script state)
/// are replayed.
const MUTABLE_DELTA_COLUMNS: &[&str] = &[
    "Transform",
    "Inventory",
    "EquipmentSlots",
    "LightSource",
    "LightFlicker",
    "AnimationPlayer",
    "AnimationStack",
    "ScriptTimer",
];

/// Where save slots live, plus the round-robin ring cursor.
///
/// Installed as a resource at startup. Default root is `<cwd>/saves`.
pub struct SaveState {
    pub dir: PathBuf,
    pub ring: disk::SaveRing,
}

impl Resource for SaveState {}

impl SaveState {
    pub fn new(dir: PathBuf, ring_size: u32) -> Self {
        Self {
            dir,
            ring: disk::SaveRing::new(ring_size),
        }
    }
}

/// Queued live-load request: a decoded, container-verified snapshot
/// awaiting the next frame's `&mut World + &mut VulkanContext` drain.
///
/// The `load` console command (which holds only `&World`) decodes + pushes
/// here; [`execute_pending_save_loads`] consumes it between frames, where
/// the App has the mutable access the cell reload needs. Mirrors the
/// `PendingDebugLoadSlot` / `PendingCellTransitionSlot` deferred shape.
#[derive(Default)]
pub struct PendingSaveLoadSlot(pub Option<Snapshot>);

impl Resource for PendingSaveLoadSlot {}

/// Build the curated game-state save registry.
///
/// Only types that carry *player-visible game state* are registered —
/// derived data (`GlobalTransform`, `WorldBound`), GPU handles
/// (`MeshHandle`, `TextureHandle`, `SkinnedMesh`), and transient event
/// markers are reconstructed on load, never serialised.
pub fn build_save_registry() -> SaveRegistry {
    use byroredux_core::animation::{AnimationPlayer, AnimationStack};
    use byroredux_core::ecs::components::{
        Children, EquipmentSlots, Inventory, LightFlicker, LightSource, Name, Parent, Transform,
    };
    use byroredux_core::ecs::resources::ItemInstancePool;
    use byroredux_scripting::ScriptTimer;

    use crate::cell_loader::CurrentCellContext;

    let mut r = SaveRegistry::new();
    r.register_component::<Transform>("Transform")
        .register_component::<Name>("Name")
        .register_component::<Parent>("Parent")
        .register_component::<Children>("Children")
        .register_component::<Inventory>("Inventory")
        .register_component::<EquipmentSlots>("EquipmentSlots")
        .register_component::<LightSource>("LightSource")
        .register_component::<LightFlicker>("LightFlicker")
        .register_component::<AnimationPlayer>("AnimationPlayer")
        .register_component::<AnimationStack>("AnimationStack")
        .register_component::<ScriptTimer>("ScriptTimer")
        .register_form_id_component("FormIdComponent")
        .register_resource::<ItemInstancePool>("ItemInstancePool")
        // M45.1 — the cell identity + plugin set the save was taken in,
        // so `load` knows which cell to reload before applying deltas.
        .register_resource::<CurrentCellContext>("CurrentCellContext");
    r
}

/// Pull the saved cell context out of a decoded snapshot, if present.
///
/// Returns `None` for saves taken outside an interior cell (loose-NIF /
/// exterior modes never set `CurrentCellContext`).
pub fn snapshot_cell_context(
    snap: &byroredux_save::Snapshot,
) -> Option<crate::cell_loader::CurrentCellContext> {
    snap.resources
        .get("CurrentCellContext")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// `save [slot]` — validate, snapshot, and atomically write the world.
pub struct SaveCommand;

impl ConsoleCommand for SaveCommand {
    fn name(&self) -> &str {
        "save"
    }
    fn description(&self) -> &str {
        "save [slot] — validate + snapshot the world to a slot (default: next ring slot)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(registry) = world.try_resource::<SaveRegistry>() else {
            return CommandOutput::error("save registry not installed");
        };
        let Some(mut state) = world.try_resource_mut::<SaveState>() else {
            return CommandOutput::error("save directory not installed");
        };

        // Explicit slot, or advance the ring so the previous save survives.
        let slot = match args.trim() {
            "" => state.ring.advance(),
            s => match s.parse::<u32>() {
                Ok(n) => n,
                Err(_) => return CommandOutput::error(format!("invalid slot '{s}'")),
            },
        };

        // Pre-save validation — refuse to persist a broken world.
        let issues = validate_world(world);
        if !issues.is_empty() {
            let mut lines = vec![format!(
                "save ABORTED: {} referential-integrity issue(s) — refusing to write a poisoned save:",
                issues.len()
            )];
            for issue in issues.iter().take(20) {
                lines.push(format!("  [{:?}] entity {}: {}", issue.kind, issue.entity, issue.detail));
            }
            if issues.len() > 20 {
                lines.push(format!("  … and {} more", issues.len() - 20));
            }
            return CommandOutput::lines(lines);
        }

        let snapshot = match save_world(world, &registry) {
            Ok(s) => s,
            Err(e) => return CommandOutput::error(format!("snapshot failed: {e}")),
        };
        let bytes = match encode(&snapshot, registry.schema_fingerprint()) {
            Ok(b) => b,
            Err(e) => return CommandOutput::error(format!("encode failed: {e}")),
        };
        match disk::write_slot(&state.dir, slot, &bytes) {
            Ok(path) => CommandOutput::lines(vec![
                format!("saved slot {slot} → {}", path.display()),
                format!(
                    "  {} entities-worth of rows across {} component columns, {} resource(s)",
                    snapshot.row_count(),
                    snapshot.components.len(),
                    snapshot.resources.len()
                ),
                format!("  {} bytes (next_entity={})", bytes.len(), snapshot.next_entity),
            ]),
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
                match snapshot_cell_context(&snap) {
                    Some(ctx) => lines.push(format!(
                        "  cell: {} (esm {}, {} master(s))",
                        ctx.cell_editor_id,
                        ctx.esm_path,
                        ctx.masters.len()
                    )),
                    None => lines.push("  cell: <none — loose/exterior save>".to_string()),
                }
                for (name, col) in &snap.components {
                    let rows = col.as_array().map_or(0, |a| a.len());
                    lines.push(format!("  {name}: {rows} rows"));
                }
                for name in snap.resources.keys() {
                    lines.push(format!("  resource {name}"));
                }
                lines.push(
                    "  (live load/apply is deferred to M45.1 — needs GPU/physics re-instantiation)"
                        .to_string(),
                );
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
        let Some(ctx) = snapshot_cell_context(&snapshot) else {
            return CommandOutput::error(
                "save has no cell context (loose/exterior save) — live load needs an interior cell",
            );
        };

        // Queue for the between-frames drain (needs &mut World + renderer).
        match world.try_resource_mut::<PendingSaveLoadSlot>() {
            Some(mut pending) => {
                pending.0 = Some(snapshot);
                CommandOutput::line(format!(
                    "queued load of slot {slot} → cell {} (applies next frame)",
                    ctx.cell_editor_id
                ))
            }
            None => CommandOutput::error("load slot not installed"),
        }
    }
}

/// Drain a queued live-load: reload the saved interior cell via the
/// existing loader (full GPU/physics/camera setup), restore saved
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
    let snapshot = {
        let Some(mut slot) = world.try_resource_mut::<PendingSaveLoadSlot>() else {
            return;
        };
        match slot.0.take() {
            Some(s) => s,
            None => return,
        }
    };
    let Some(cell_ctx) = snapshot_cell_context(&snapshot) else {
        log::error!("save load: snapshot lost its cell context between queue and drain");
        return;
    };

    let registry = build_save_registry();

    // Build asset providers from the boot CLI args (same BSAs the engine
    // is running with) — matches the cell-transition path.
    let args = crate::cli_args::effective_args();
    let tex_provider = crate::asset_provider::build_texture_provider(&args);
    let mut mat_provider = crate::asset_provider::build_material_provider(&args);

    // Tear down whatever's loaded, then reload the saved cell fresh.
    if streaming.is_some() {
        crate::streaming_helpers::drain_streaming_state(world, ctx, streaming);
    }
    crate::cell_loader::unload_current_interior(world, ctx);

    let result = crate::cell_loader::load_cell_with_masters(
        &cell_ctx.masters,
        &cell_ctx.esm_path,
        &cell_ctx.cell_editor_id,
        world,
        ctx,
        &tex_provider,
        Some(&mut mat_provider),
    );
    let entity_count = match result {
        Ok(r) => {
            if let Some(ref lit) = r.lighting {
                crate::cell_loader::apply_interior_cell_lighting(world, lit);
            }
            ctx.signal_temporal_discontinuity(
                crate::streaming_helpers::SVGF_TAA_STREAMING_RECOVERY_FRAMES,
            );
            world.insert_resource(crate::cell_loader::LoadedPluginSet {
                masters: cell_ctx.masters.clone(),
                esm_path: cell_ctx.esm_path.clone(),
            });
            r.entity_count
        }
        Err(e) => {
            log::error!(
                "save load: failed to reload cell '{}': {:#}",
                cell_ctx.cell_editor_id,
                e
            );
            return;
        }
    };

    // Restore saved resources (ItemInstancePool) so inventory instance
    // ids resolve, then overlay the form-id-keyed mutable deltas.
    if let Err(e) = byroredux_save::restore_resources(world, &registry, &snapshot) {
        log::error!("save load: resource restore failed: {e}");
        return;
    }
    let remap = byroredux_save::build_form_id_remap(world, &registry, &snapshot);
    match byroredux_save::apply_deltas(world, &registry, &snapshot, &remap, MUTABLE_DELTA_COLUMNS) {
        Ok(applied) => log::info!(
            "save load: cell '{}' reloaded ({} entities); applied {} saved deltas across {} \
             form-id-matched entities",
            cell_ctx.cell_editor_id,
            entity_count,
            applied,
            remap.len()
        ),
        Err(e) => log::error!("save load: delta apply failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::Transform;
    use byroredux_core::form_id::FormIdPool;
    use byroredux_core::math::Vec3;
    use byroredux_core::string::StringPool;
    use byroredux_save::{decode, restore_world};
    use byroredux_scripting::ScriptTimer;

    /// The binary's curated registry must round-trip its full type set —
    /// including the cross-crate `ScriptTimer` and a stable form id —
    /// through encode → decode → restore into a fresh World.
    #[test]
    fn binary_registry_round_trips_including_scripttimer() {
        let reg = build_save_registry();

        let mut src = World::new();
        src.insert_resource(StringPool::new());
        src.insert_resource(FormIdPool::new());
        let e = src.spawn();
        let other = src.spawn();
        src.insert(e, Transform::from_translation(Vec3::new(4.0, 5.0, 6.0)));
        src.insert(e, ScriptTimer { id: 42, remaining: 3.5 });
        src.insert(other, ScriptTimer { id: 7, remaining: 0.25 });

        let snap = save_world(&src, &reg).unwrap();
        let bytes = encode(&snap, reg.schema_fingerprint()).unwrap();
        let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

        let mut dst = World::new();
        dst.insert_resource(FormIdPool::new());
        restore_world(&mut dst, &reg, &decoded).unwrap();

        assert_eq!(dst.next_entity_id(), 2);
        let q = dst.query::<ScriptTimer>().unwrap();
        let timers: std::collections::HashMap<u32, (u32, f32)> =
            q.iter().map(|(en, t)| (en, (t.id, t.remaining))).collect();
        assert_eq!(timers[&0], (42, 3.5));
        assert_eq!(timers[&1], (7, 0.25));

        let qt = dst.query::<Transform>().unwrap();
        assert_eq!(qt.iter().next().unwrap().1.translation, Vec3::new(4.0, 5.0, 6.0));
    }

    /// A clean validation pass is the precondition every save checks.
    #[test]
    fn fresh_world_validates_clean() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Transform::default());
        assert!(validate_world(&world).is_empty());
    }

    /// `save` then `load` (commands) round-trip through disk: the save
    /// captures the live `CurrentCellContext`, and `load` decodes it back
    /// and queues a snapshot whose cell context matches. Exercises the
    /// command plumbing end-to-end minus the GPU drain.
    #[test]
    fn save_then_load_command_queues_with_cell_context() {
        use crate::cell_loader::CurrentCellContext;

        let mut world = World::new();
        world.insert_resource(StringPool::new());
        world.insert_resource(FormIdPool::new());
        world.insert_resource(build_save_registry());
        let dir = std::env::temp_dir().join(format!("byro_m451_cmd_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        world.insert_resource(SaveState::new(dir.clone(), 4));
        world.insert_resource(PendingSaveLoadSlot::default());
        world.insert_resource(CurrentCellContext {
            cell_editor_id: "GSDocMitchellHouse".to_string(),
            esm_path: "FalloutNV.esm".to_string(),
            masters: vec![],
        });

        let e = world.spawn();
        world.insert(e, Transform::from_translation(Vec3::new(7.0, 8.0, 9.0)));

        // save → slot 0
        let out = SaveCommand.execute(&world, "0");
        assert!(
            out.lines.iter().any(|l| l.contains("saved slot 0")),
            "save output: {:?}",
            out.lines
        );

        // load → should queue a snapshot carrying the cell context
        let out = LoadCommand.execute(&world, "0");
        assert!(
            out.lines.iter().any(|l| l.contains("GSDocMitchellHouse")),
            "load output: {:?}",
            out.lines
        );
        let pending = world.resource::<PendingSaveLoadSlot>();
        let snap = pending.0.as_ref().expect("snapshot queued");
        let ctx = snapshot_cell_context(snap).expect("cell context survived round-trip");
        assert_eq!(ctx.cell_editor_id, "GSDocMitchellHouse");
        assert_eq!(ctx.esm_path, "FalloutNV.esm");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
