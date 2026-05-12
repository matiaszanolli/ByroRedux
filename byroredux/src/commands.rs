//! Console commands for the engine's built-in command system.

use byroredux_core::console::{CommandOutput, CommandRegistry, ConsoleCommand};
use byroredux_core::ecs::{
    AccessConflict, ActiveCamera, Camera, ConflictKind, DebugStats, EntityId, GlobalTransform,
    Material, MeshHandle, Name, SchedulerAccessReport, ScratchTelemetry, SelectedRef,
    SkinCoverageStats, SkinnedMesh, TextureHandle, Transform, World,
};
use byroredux_core::math::{Mat4, Quat, Vec3};
use byroredux_core::string::StringPool;
use std::collections::HashMap;

use byroredux_core::ecs::SystemList;
use crate::components::InputState;

struct HelpCommand;
impl ConsoleCommand for HelpCommand {
    fn name(&self) -> &str {
        "help"
    }
    fn description(&self) -> &str {
        "List all available commands"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let registry = world.resource::<CommandRegistry>();
        let mut lines = vec!["Available commands:".to_string()];
        for (name, desc) in registry.list() {
            lines.push(format!("  {:16} {}", name, desc));
        }
        CommandOutput::lines(lines)
    }
}

struct StatsCommand;
impl ConsoleCommand for StatsCommand {
    fn name(&self) -> &str {
        "stats"
    }
    fn description(&self) -> &str {
        "Show engine performance statistics"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let stats = world.resource::<DebugStats>();
        let (min_dt, max_dt) = stats.min_max_frame_time();
        CommandOutput::lines(vec![
            format!("FPS:       {:.0} (avg {:.0})", stats.fps, stats.avg_fps()),
            format!(
                "Frame:     {:.2}ms (min {:.2}ms, max {:.2}ms)",
                stats.frame_time_ms,
                min_dt * 1000.0,
                max_dt * 1000.0
            ),
            format!("Entities:  {}", stats.entity_count),
            // #637 / FNV-D5-02 — show registry-wide AND scene-scoped
            // counts so a leak that holds the last reference past cell
            // unload is visible as `<registry>` larger than `<in_use>`.
            // For single-cell sessions the two numbers usually match;
            // when M40 world streaming is active they should still
            // bounce in lockstep, so a steady-state gap = leak.
            format!(
                "Meshes:    {} registry / {} in use",
                stats.mesh_count, stats.meshes_in_use
            ),
            format!(
                "Textures:  {} registry / {} in use",
                stats.texture_count, stats.textures_in_use
            ),
            format!("Draws:     {}", stats.draw_call_count),
        ])
    }
}

struct EntitiesCommand;
impl ConsoleCommand for EntitiesCommand {
    fn name(&self) -> &str {
        "entities"
    }
    fn description(&self) -> &str {
        "Show entity count and component breakdown"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let total = world.next_entity_id();
        let mut lines = vec![format!("Total entities spawned: {}", total)];
        lines.push(format!("  Transform:     {}", world.count::<Transform>()));
        lines.push(format!("  MeshHandle:    {}", world.count::<MeshHandle>()));
        lines.push(format!(
            "  TextureHandle: {}",
            world.count::<TextureHandle>()
        ));
        lines.push(format!("  Camera:        {}", world.count::<Camera>()));
        CommandOutput::lines(lines)
    }
}

struct SystemsCommand;
impl ConsoleCommand for SystemsCommand {
    fn name(&self) -> &str {
        "systems"
    }
    fn description(&self) -> &str {
        "List registered ECS systems"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        if let Some(list) = world.try_resource::<SystemList>() {
            let mut lines = vec![format!("Registered systems ({}):", list.0.len())];
            for (i, name) in list.0.iter().enumerate() {
                lines.push(format!("  [{}] {}", i, name));
            }
            CommandOutput::lines(lines)
        } else {
            CommandOutput::line("No system list available")
        }
    }
}

struct TexMissingCommand;
impl ConsoleCommand for TexMissingCommand {
    fn name(&self) -> &str {
        "tex.missing"
    }
    fn description(&self) -> &str {
        "List entities with fallback (checkerboard) texture and their expected paths"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let tex_q = world.query::<TextureHandle>();
        let mat_q = world.query::<Material>();
        let (Some(tex_q), Some(mat_q)) = (tex_q, mat_q) else {
            return CommandOutput::line("No TextureHandle or Material components found");
        };

        let mut missing: HashMap<String, u32> = HashMap::new();
        for (entity, tex) in tex_q.iter() {
            if tex.0 != 0 {
                continue;
            }
            let mat = mat_q.get(entity);
            let path = mat
                .and_then(|m| m.texture_path.as_deref())
                .or_else(|| mat.and_then(|m| m.material_path.as_deref()))
                .unwrap_or("<no path, no material>");
            *missing.entry(path.to_string()).or_insert(0) += 1;
        }

        if missing.is_empty() {
            return CommandOutput::line(
                "No missing textures — all entities have resolved textures",
            );
        }

        let mut sorted: Vec<_> = missing.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));

        let mut lines = vec![format!("{} unique missing textures:", sorted.len())];
        for (path, count) in sorted.iter().take(50) {
            lines.push(format!("  {:4}x  {}", count, path));
        }
        if sorted.len() > 50 {
            lines.push(format!("  ... and {} more", sorted.len() - 50));
        }
        CommandOutput::lines(lines)
    }
}

struct TexLoadedCommand;
impl ConsoleCommand for TexLoadedCommand {
    fn name(&self) -> &str {
        "tex.loaded"
    }
    fn description(&self) -> &str {
        "Show count and sample of successfully loaded textures"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let tex_q = world.query::<TextureHandle>();
        let mat_q = world.query::<Material>();
        let (Some(tex_q), Some(mat_q)) = (tex_q, mat_q) else {
            return CommandOutput::line("No TextureHandle or Material components found");
        };

        let mut loaded: HashMap<String, u32> = HashMap::new();
        let mut fallback_count = 0u32;
        for (entity, tex) in tex_q.iter() {
            if tex.0 == 0 {
                fallback_count += 1;
                continue;
            }
            let path = mat_q
                .get(entity)
                .and_then(|m| m.texture_path.as_deref())
                .unwrap_or("<no path>");
            *loaded.entry(path.to_string()).or_insert(0) += 1;
        }

        let mut lines = vec![format!(
            "{} unique loaded textures, {} entities using fallback",
            loaded.len(),
            fallback_count
        )];
        let mut sorted: Vec<_> = loaded.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        for (path, count) in sorted.iter().take(30) {
            lines.push(format!("  {:4}x  {}", count, path));
        }
        CommandOutput::lines(lines)
    }
}

struct MeshInfoCommand;
impl ConsoleCommand for MeshInfoCommand {
    fn name(&self) -> &str {
        "mesh.info"
    }
    fn description(&self) -> &str {
        "Show mesh/texture/material info for an entity: mesh.info <entity_id>"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let id: u32 = match args.trim().parse() {
            Ok(v) => v,
            Err(_) => return CommandOutput::line("Usage: mesh.info <entity_id>"),
        };
        let mut lines = vec![format!("Entity {}:", id)];
        if let Some(mh) = world.get::<MeshHandle>(id) {
            lines.push(format!("  MeshHandle: {}", mh.0));
        } else {
            lines.push("  MeshHandle: (none)".to_string());
        }
        if let Some(th) = world.get::<TextureHandle>(id) {
            lines.push(format!(
                "  TextureHandle: {}{}",
                th.0,
                if th.0 == 0 { " (FALLBACK)" } else { "" }
            ));
        } else {
            lines.push("  TextureHandle: (none)".to_string());
        }
        if let Some(mat) = world.get::<Material>(id) {
            lines.push(format!(
                "  texture_path:  {}",
                mat.texture_path.as_deref().unwrap_or("(none)")
            ));
            lines.push(format!(
                "  material_path: {}",
                mat.material_path.as_deref().unwrap_or("(none)")
            ));
            lines.push(format!(
                "  normal_map:    {}",
                mat.normal_map.as_deref().unwrap_or("(none)")
            ));
            lines.push(format!(
                "  glow_map:      {}",
                mat.glow_map.as_deref().unwrap_or("(none)")
            ));
        } else {
            lines.push("  Material: (none)".to_string());
        }
        CommandOutput::lines(lines)
    }
}

/// `mesh.cache` — inspect the process-lifetime NIF import cache.
/// Reports cache size, parsed/failed counts, and lifetime hit rate.
/// See [`crate::cell_loader::NifImportRegistry`] / #381.
struct MeshCacheCommand;
impl ConsoleCommand for MeshCacheCommand {
    fn name(&self) -> &str {
        "mesh.cache"
    }
    fn description(&self) -> &str {
        "Show NIF import cache stats (size, hits, misses, hit rate)"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(reg) = world.try_resource::<crate::cell_loader::NifImportRegistry>() else {
            return CommandOutput::line("NifImportRegistry resource not present");
        };
        let cap_str = if reg.max_entries() == 0 {
            "unlimited (set BYRO_NIF_CACHE_MAX=N to enable LRU)".to_string()
        } else {
            format!("{} (LRU eviction)", reg.max_entries())
        };
        CommandOutput::lines(vec![
            "NIF import cache:".to_string(),
            format!(
                "  entries:       {} ({} parsed, {} failed)",
                reg.len(),
                reg.core.parsed_count(),
                reg.core.failed_count(),
            ),
            format!("  capacity:      {}", cap_str),
            format!("  lifetime hits: {}", reg.core.hits()),
            format!("  lifetime miss: {}", reg.core.misses()),
            format!("  evictions:     {}", reg.evictions),
            format!("  hit rate:      {:.1}%", reg.hit_rate_pct()),
        ])
    }
}

/// `ctx.scratch` — print per-Vec capacity / len / heap-bytes for every
/// persistent CPU-side scratch buffer in the renderer (R6).
///
/// Designed to surface unbounded growth across long sessions or
/// multi-cell streaming (M40), where a `Vec::reserve` driven by an
/// outlier frame would otherwise pin capacity at the high-water mark
/// indefinitely with zero observability. Read this after suspect
/// activity to see if any row's `capacity` × `elem_size` looks
/// disproportionate to the working set.
struct CtxScratchCommand;
impl ConsoleCommand for CtxScratchCommand {
    fn name(&self) -> &str {
        "ctx.scratch"
    }
    fn description(&self) -> &str {
        "Show renderer scratch-Vec capacities (R6 — catch unbounded growth)"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(tlm) = world.try_resource::<ScratchTelemetry>() else {
            return CommandOutput::line("ScratchTelemetry resource not present");
        };
        if tlm.rows.is_empty() {
            return CommandOutput::line(
                "ScratchTelemetry has no rows — renderer not initialized yet",
            );
        }
        let mut lines = vec![
            "VulkanContext scratch buffers (R6):".to_string(),
            format!(
                "  {:<26} {:>10} {:>10} {:>12} {:>12}",
                "name", "len", "capacity", "bytes_used", "wasted"
            ),
        ];
        for row in &tlm.rows {
            lines.push(format!(
                "  {:<26} {:>10} {:>10} {:>10} B {:>10} B",
                row.name,
                row.len,
                row.capacity,
                row.bytes_used(),
                row.wasted_bytes(),
            ));
        }
        lines.push(format!(
            "  total: {} bytes used, {} bytes wasted across {} scratches",
            tlm.total_bytes(),
            tlm.total_wasted(),
            tlm.rows.len(),
        ));
        // #780 / PERF-N1 — R1 MaterialTable dedup ratio. Reads zero
        // until the first `build_render_data` populates the resource;
        // after that, divergence between unique and interned counts
        // is what catches a dedup regression at scale.
        if tlm.materials_interned > 0 {
            let ratio = tlm.materials_interned as f64 / tlm.materials_unique.max(1) as f64;
            lines.push(format!(
                "  materials: {} unique / {} interned ({:.1}× dedup)",
                tlm.materials_unique, tlm.materials_interned, ratio,
            ));
        }
        CommandOutput::lines(lines)
    }
}

/// `skin.coverage` — print last frame's skinned-mesh BLAS coverage
/// snapshot.
///
/// Closes the "skinned BLAS coverage" observability gap: until this
/// command, there was no way to confirm whether *every* visible
/// skinned entity got a pre-skin + BLAS refit this frame. The
/// green-bar is `refits_succeeded == dispatches_total && slots_failed
/// == 0` (printed as `coverage: full` / `coverage: PARTIAL`).
///
/// Read after spawning a multi-NPC cell (FNV `GSDocMitchellHouse`,
/// SSE `WhiterunBanneredMare`, FO4 `MedTekResearch01`) to verify the
/// equip-pipeline NPCs reach the RT path. A drop in `refits_succeeded`
/// relative to `dispatches_total` flags a regression somewhere between
/// the visible-entity stream (`build_render_data`) and the per-frame
/// refit (`draw_frame`).
struct SkinCoverageCommand;
impl ConsoleCommand for SkinCoverageCommand {
    fn name(&self) -> &str {
        "skin.coverage"
    }
    fn description(&self) -> &str {
        "Show last-frame skinned BLAS coverage (dispatches / first-sight / refit)"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(cov) = world.try_resource::<SkinCoverageStats>() else {
            return CommandOutput::line("SkinCoverageStats resource not present");
        };
        let mut lines = vec!["Skinned BLAS coverage (last frame):".to_string()];
        lines.push(format!(
            "  dispatches_total       = {}  (visible skinned entities)",
            cov.dispatches_total,
        ));
        lines.push(format!(
            "  slots_active           = {} / {}  (pool {:.0}% full)",
            cov.slots_active,
            cov.slot_pool_capacity,
            if cov.slot_pool_capacity == 0 {
                0.0
            } else {
                100.0 * cov.slots_active as f64 / cov.slot_pool_capacity as f64
            },
        ));
        lines.push(format!(
            "  slots_failed           = {}  (suppressed until LRU eviction)",
            cov.slots_failed,
        ));
        lines.push(format!(
            "  first_sight_attempted  = {}",
            cov.first_sight_attempted,
        ));
        lines.push(format!(
            "  first_sight_succeeded  = {}",
            cov.first_sight_succeeded,
        ));
        lines.push(format!(
            "  refits_attempted       = {}",
            cov.refits_attempted,
        ));
        lines.push(format!(
            "  refits_succeeded       = {}",
            cov.refits_succeeded,
        ));
        if cov.dispatches_total == 0 {
            lines.push("  coverage: n/a (no skinned entities this frame)".to_string());
        } else if cov.fully_covered() {
            lines.push("  coverage: full".to_string());
        } else {
            let missed = cov
                .dispatches_total
                .saturating_sub(cov.refits_succeeded);
            lines.push(format!(
                "  coverage: PARTIAL — {} of {} visible skinned entities missed this frame",
                missed, cov.dispatches_total,
            ));
        }
        if !cov.failed_entity_ids.is_empty() {
            let sample: Vec<String> = cov
                .failed_entity_ids
                .iter()
                .map(|id| id.to_string())
                .collect();
            lines.push(format!(
                "  failed_entity_ids (sample): [{}]",
                sample.join(", "),
            ));
        }
        CommandOutput::lines(lines)
    }
}

/// Resolve an entity's `Name` to a printable String via the
/// `StringPool` resource. Returns `None` when the entity has no
/// `Name`, when the pool isn't installed, or when the symbol doesn't
/// resolve (which would itself be a string-pool integrity bug).
fn resolve_entity_name(world: &World, entity: EntityId) -> Option<String> {
    let name_q = world.query::<Name>()?;
    let name = name_q.get(entity)?;
    let pool = world.try_resource::<StringPool>()?;
    pool.resolve(name.0).map(|s| s.to_string())
}

/// `prid <entity_id>` — pick a reference (Bethesda console heritage).
///
/// Sets the world-scoped [`SelectedRef`] to the given entity so that
/// follow-up commands operate on it by default. Today's consumers:
/// `inspect` (no args), `cam.tp` (no args). The natural workflow:
///
/// ```text
/// byro> entities Inventory          # list NPCs with equip state
/// byro> prid 42                     # pick one
/// byro> cam.tp                      # frame it
/// byro> inspect                     # dump every component on it
/// byro> skin.coverage               # read coverage against this view
/// ```
///
/// With no arg, `prid` prints the current selection (`SelectedRef`
/// resource state). The selection is not implicitly cleared on cell
/// unload — a re-issued generational `EntityId` could re-bind to a
/// new entity. This is a known dev-tool sharp edge that matches
/// Bethesda's own `prid` semantics; M40 cell streaming will need an
/// explicit clear-on-unload pass later.
struct PridCommand;
impl ConsoleCommand for PridCommand {
    fn name(&self) -> &str {
        "prid"
    }
    fn description(&self) -> &str {
        "Pick a reference for follow-up commands (usage: prid <entity_id>)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let trimmed = args.trim();
        if trimmed.is_empty() {
            let Some(sel) = world.try_resource::<SelectedRef>() else {
                return CommandOutput::line("SelectedRef resource not present");
            };
            return match sel.0 {
                None => CommandOutput::line("no entity selected (usage: prid <entity_id>)"),
                Some(entity) => {
                    let name = resolve_entity_name(world, entity);
                    drop(sel);
                    CommandOutput::line(match name {
                        Some(n) => format!("selected: entity {entity} ({n})"),
                        None => format!("selected: entity {entity}"),
                    })
                }
            };
        }
        let Ok(target) = trimmed.parse::<EntityId>() else {
            return CommandOutput::line(format!(
                "prid: failed to parse entity id from `{trimmed}`"
            ));
        };
        // Validate the entity exists. Transform is the closest thing
        // to "every entity has it" in this ECS — placement roots,
        // NPCs, bones, cameras all carry one. A bone with only a
        // hierarchy parent + Name (rare) would fail this check;
        // that's a deliberately conservative bar to keep `prid` from
        // accepting typos silently. Falls back to GlobalTransform to
        // catch the bone case.
        let has_transform = world
            .query::<Transform>()
            .map(|q| q.contains(target))
            .unwrap_or(false);
        let has_global = world
            .query::<GlobalTransform>()
            .map(|q| q.contains(target))
            .unwrap_or(false);
        if !has_transform && !has_global {
            return CommandOutput::line(format!(
                "prid: entity {target} has no Transform/GlobalTransform — \
                 does it exist? (use `entities` to list)"
            ));
        }
        let Some(mut sel) = world.try_resource_mut::<SelectedRef>() else {
            return CommandOutput::line("SelectedRef resource not present");
        };
        sel.0 = Some(target);
        drop(sel);
        let name = resolve_entity_name(world, target);
        CommandOutput::line(match name {
            Some(n) => format!("selected: entity {target} ({n})"),
            None => format!("selected: entity {target}"),
        })
    }
}

/// Derive fly-camera `(yaw, pitch)` in radians for a camera at `from`
/// to look at `to`. Matches `fly_camera_system`'s rotation composition
/// (`Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch)` with
/// `forward = rotation * -Z`), so updating `InputState.{yaw, pitch}`
/// alongside `Transform.rotation` survives the next fly-camera tick.
/// Degenerate `to == from` returns `(0, 0)`.
fn look_at_yaw_pitch(from: Vec3, to: Vec3) -> (f32, f32) {
    let diff = to - from;
    let len_sq = diff.length_squared();
    if len_sq < 1e-6 {
        return (0.0, 0.0);
    }
    let dir = diff / len_sq.sqrt();
    let pitch = dir.y.clamp(-1.0, 1.0).asin();
    let yaw = (-dir.x).atan2(-dir.z);
    (yaw, pitch)
}

/// `cam.where` — print the active camera's world position + yaw/pitch.
///
/// Use to capture the current viewpoint before teleporting elsewhere
/// so you can return to it (`cam.pos x y z`). Pairs with `skin.
/// coverage` for documenting which viewpoint produced a given coverage
/// reading.
struct CamWhereCommand;
impl ConsoleCommand for CamWhereCommand {
    fn name(&self) -> &str {
        "cam.where"
    }
    fn description(&self) -> &str {
        "Print active camera position + yaw/pitch (radians)"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(active) = world.try_resource::<ActiveCamera>() else {
            return CommandOutput::line("ActiveCamera resource not present");
        };
        let cam_entity = active.0;
        drop(active);
        let pos = world
            .query::<Transform>()
            .and_then(|q| q.get(cam_entity).map(|t| t.translation));
        let Some(pos) = pos else {
            return CommandOutput::line(format!(
                "Camera entity {cam_entity} has no Transform"
            ));
        };
        let (yaw, pitch) = if let Some(input) = world.try_resource::<InputState>() {
            (input.yaw, input.pitch)
        } else {
            (0.0, 0.0)
        };
        CommandOutput::lines(vec![
            format!("Camera entity: {}", cam_entity),
            format!(
                "  position: ({:.2}, {:.2}, {:.2})",
                pos.x, pos.y, pos.z
            ),
            format!(
                "  yaw:      {:.4} rad ({:.1}°)",
                yaw,
                yaw.to_degrees()
            ),
            format!(
                "  pitch:    {:.4} rad ({:.1}°)",
                pitch,
                pitch.to_degrees()
            ),
        ])
    }
}

/// `cam.pos x y z` — teleport the active camera to an absolute world
/// position (renderer Y-up). Leaves rotation untouched.
///
/// `fly_camera_system` early-returns when the mouse isn't captured
/// (the default for `--bench-hold`), so the new position persists
/// across frames. With mouse capture active the camera still moves
/// relative to WASD input, so this command sets the *anchor* for that
/// frame's worth of input rather than locking the camera in place.
struct CamPosCommand;
impl ConsoleCommand for CamPosCommand {
    fn name(&self) -> &str {
        "cam.pos"
    }
    fn description(&self) -> &str {
        "Teleport camera to absolute world position (usage: cam.pos x y z)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let parts: Vec<&str> = args.split_whitespace().collect();
        if parts.len() != 3 {
            return CommandOutput::line(
                "usage: cam.pos <x> <y> <z>  (renderer Y-up coordinates)",
            );
        }
        let parse = |s: &str| -> Option<f32> { s.parse::<f32>().ok() };
        let (Some(x), Some(y), Some(z)) =
            (parse(parts[0]), parse(parts[1]), parse(parts[2]))
        else {
            return CommandOutput::line(format!(
                "cam.pos: failed to parse coordinates from `{args}`"
            ));
        };
        let Some(active) = world.try_resource::<ActiveCamera>() else {
            return CommandOutput::line("ActiveCamera resource not present");
        };
        let cam_entity = active.0;
        drop(active);
        let Some(mut tq) = world.query_mut::<Transform>() else {
            return CommandOutput::line("Transform storage not present");
        };
        let Some(transform) = tq.get_mut(cam_entity) else {
            return CommandOutput::line(format!(
                "Camera entity {cam_entity} has no Transform"
            ));
        };
        transform.translation = Vec3::new(x, y, z);
        CommandOutput::line(format!(
            "Camera teleported to ({x:.2}, {y:.2}, {z:.2})"
        ))
    }
}

/// `cam.tp <entity_id>` — teleport the active camera to look at the
/// given entity. The camera lands ~200 units back along the target's
/// -Z axis at +50 Y for a reasonable over-the-shoulder framing on
/// FNV / Skyrim+ NPCs (~100 unit tall humanoids).
///
/// Both `Transform.rotation` and `InputState.{yaw, pitch}` are
/// updated so the orientation survives the next `fly_camera_system`
/// tick even when the mouse is captured.
///
/// The natural usage with `skin.coverage`: spawn a multi-NPC cell with
/// `--bench-hold`, `cam.tp <npc_entity_id>` to frame the actor, then
/// `skin.coverage` reads the new viewpoint's dispatches_total.
struct CamTpCommand;
impl ConsoleCommand for CamTpCommand {
    fn name(&self) -> &str {
        "cam.tp"
    }
    fn description(&self) -> &str {
        "Teleport camera to look at entity (usage: cam.tp <entity_id>)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let trimmed = args.trim();
        let target_id = if trimmed.is_empty() {
            // Fall back to the picked reference (`prid <id>` workflow).
            // No selection AND no arg → user error, point them at the
            // shorter path.
            let Some(sel) = world.try_resource::<SelectedRef>() else {
                return CommandOutput::line("SelectedRef resource not present");
            };
            let Some(id) = sel.0 else {
                return CommandOutput::line(
                    "usage: cam.tp <entity_id>  (or `prid <id>` then `cam.tp`)",
                );
            };
            id
        } else {
            let Ok(id) = trimmed.parse::<EntityId>() else {
                return CommandOutput::line(format!(
                    "cam.tp: failed to parse entity id from `{trimmed}`"
                ));
            };
            id
        };
        let Some(active) = world.try_resource::<ActiveCamera>() else {
            return CommandOutput::line("ActiveCamera resource not present");
        };
        let cam_entity = active.0;
        drop(active);
        // Read the target's world position. GlobalTransform is updated
        // by `transform_propagation_system` each frame — for entities
        // freshly spawned this frame the value may still be the
        // identity-default, but for cell-stable entities it's the
        // resolved position. Read-only — no lock contention with the
        // mutate below.
        let target_pos = world
            .query::<GlobalTransform>()
            .and_then(|q| q.get(target_id).map(|gt| gt.translation));
        let Some(target_pos) = target_pos else {
            return CommandOutput::line(format!(
                "Entity {target_id} has no GlobalTransform (does it exist? `entities` to list)"
            ));
        };
        // Land ~200 units back + 50 up. World-space offset, not local
        // — keeps the over-the-shoulder framing predictable regardless
        // of the target's own orientation.
        let camera_pos = target_pos + Vec3::new(0.0, 50.0, 200.0);
        let (yaw, pitch) = look_at_yaw_pitch(camera_pos, target_pos);
        let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
        // Apply Transform mutation under its own scope so the input-
        // state mutation doesn't hold two write guards simultaneously.
        {
            let Some(mut tq) = world.query_mut::<Transform>() else {
                return CommandOutput::line("Transform storage not present");
            };
            let Some(transform) = tq.get_mut(cam_entity) else {
                return CommandOutput::line(format!(
                    "Camera entity {cam_entity} has no Transform"
                ));
            };
            transform.translation = camera_pos;
            transform.rotation = rotation;
        }
        // Sync InputState so the next fly_camera tick under mouse
        // capture reads back the same yaw/pitch instead of overwriting
        // the look direction with stale accumulator values.
        if let Some(mut input) = world.try_resource_mut::<InputState>() {
            input.yaw = yaw;
            input.pitch = pitch;
        }
        CommandOutput::lines(vec![
            format!(
                "Camera teleported to look at entity {target_id} at \
                 ({:.2}, {:.2}, {:.2})",
                target_pos.x, target_pos.y, target_pos.z,
            ),
            format!(
                "  camera now at ({:.2}, {:.2}, {:.2}) yaw {:.1}° pitch {:.1}°",
                camera_pos.x,
                camera_pos.y,
                camera_pos.z,
                yaw.to_degrees(),
                pitch.to_degrees(),
            ),
        ])
    }
}

/// `sys.accesses` — print the scheduler's declared-access report.
///
/// For each stage, lists every system + its declared (or undeclared)
/// access pattern, then any inter-system conflict pairs (Conflict for
/// known disagreements between two declared systems, Unknown when at
/// least one side hasn't declared). Operator tool for R7 — the static
/// view of "what will serialize when M27 turns on parallel dispatch."
struct SysAccessesCommand;
impl ConsoleCommand for SysAccessesCommand {
    fn name(&self) -> &str {
        "sys.accesses"
    }
    fn description(&self) -> &str {
        "Show declared-access report for the scheduler (R7)"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(report_res) = world.try_resource::<SchedulerAccessReport>() else {
            return CommandOutput::line(
                "SchedulerAccessReport resource not present (engine not started?)",
            );
        };
        let report = &report_res.0;

        let mut lines = Vec::new();
        lines.push(format!(
            "Scheduler access report — {} systems, {} undeclared, \
             {} known conflicts, {} unknown pairs",
            report.system_count(),
            report.undeclared_count(),
            report.known_conflict_count(),
            report.unknown_pair_count(),
        ));

        for stage_report in &report.stages {
            lines.push(String::new());
            lines.push(format!("─── stage {:?} ────", stage_report.stage));
            for row in &stage_report.systems {
                let tag = if row.is_exclusive {
                    "exclusive"
                } else {
                    "parallel "
                };
                let summary = match &row.declared {
                    None => "(undeclared)".to_string(),
                    Some(a) if a.is_empty() => "(declared, empty)".to_string(),
                    Some(a) => {
                        let parts: Vec<String> = a
                            .components_read
                            .iter()
                            .map(|e| format!("read {}", short(e.type_name)))
                            .chain(
                                a.components_write
                                    .iter()
                                    .map(|e| format!("write {}", short(e.type_name))),
                            )
                            .chain(
                                a.resources_read
                                    .iter()
                                    .map(|e| format!("read res {}", short(e.type_name))),
                            )
                            .chain(
                                a.resources_write
                                    .iter()
                                    .map(|e| format!("write res {}", short(e.type_name))),
                            )
                            .collect();
                        parts.join(", ")
                    }
                };
                lines.push(format!("  [{}] {}: {}", tag, row.name, summary));
            }
            if !stage_report.conflicts.is_empty() {
                lines.push(format!("  conflicts ({}):", stage_report.conflicts.len()));
                for c in &stage_report.conflicts {
                    match &c.conflict {
                        AccessConflict::Conflict { pairs } => {
                            for p in pairs {
                                let arrow = match p.kind {
                                    ConflictKind::ReadWrite => "reads, other writes",
                                    ConflictKind::WriteRead => "writes, other reads",
                                    ConflictKind::WriteWrite => "both write",
                                };
                                let kind = if p.is_resource { "res " } else { "" };
                                lines.push(format!(
                                    "    CONFLICT  {} <-> {} on {}{} ({})",
                                    c.left,
                                    c.right,
                                    kind,
                                    short(p.type_name),
                                    arrow,
                                ));
                            }
                        }
                        AccessConflict::Unknown {
                            left_undeclared,
                            right_undeclared,
                        } => {
                            let why = match (left_undeclared, right_undeclared) {
                                (true, true) => "both undeclared",
                                (true, false) => "left undeclared",
                                (false, true) => "right undeclared",
                                (false, false) => "?",
                            };
                            lines.push(format!(
                                "    UNKNOWN   {} <-> {} ({})",
                                c.left, c.right, why,
                            ));
                        }
                        AccessConflict::None => {}
                    }
                }
            }
        }
        CommandOutput::lines(lines)
    }
}

/// Strip the leading module path off a `std::any::type_name` so report
/// lines stay readable on narrow terminals.
fn short(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name)
}

/// `skin.list` — enumerate every entity carrying [`SkinnedMesh`].
///
/// Companion to [`SkinDumpCommand`] (#841): operators run `skin.list`
/// to find the entity_id of the actor whose body is misrendering, then
/// `skin.dump <id>` to inspect its palette.
struct SkinListCommand;
impl ConsoleCommand for SkinListCommand {
    fn name(&self) -> &str {
        "skin.list"
    }
    fn description(&self) -> &str {
        "List all SkinnedMesh entities (id, bone_count, skeleton_root, name)"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(skin_q) = world.query::<SkinnedMesh>() else {
            return CommandOutput::line("No SkinnedMesh components found");
        };
        let pool = world.try_resource::<StringPool>();
        let name_q = world.query::<Name>();
        let mut lines = vec![format!("{} skinned mesh entities:", skin_q.len())];
        lines.push(format!(
            "  {:>8}  {:>5}  {:>13}  name",
            "entity", "bones", "skeleton_root"
        ));
        let mut rows: Vec<(u32, usize, Option<u32>, String)> = Vec::new();
        for (entity, skin) in skin_q.iter() {
            let name = name_q
                .as_ref()
                .and_then(|q| q.get(entity))
                .and_then(|n| pool.as_ref().and_then(|p| p.resolve(n.0)))
                .map(|s| s.to_string())
                .unwrap_or_else(|| "(no Name)".to_string());
            rows.push((entity, skin.bones.len(), skin.skeleton_root, name));
        }
        rows.sort_by_key(|r| r.0);
        for (entity, bone_count, root, name) in rows {
            let root_str = match root {
                Some(r) => format!("{}", r),
                None => "(none)".to_string(),
            };
            lines.push(format!(
                "  {:>8}  {:>5}  {:>13}  {}",
                entity, bone_count, root_str, name
            ));
        }
        CommandOutput::lines(lines)
    }
}

/// `skin.dump <entity_id>` — dump per-bone palette state for one
/// skinned mesh entity. Phase 1b.x diagnostic for the body-spike
/// artifact (#841): for each bone slot, prints its resolved entity,
/// `Name`, current `GlobalTransform`, baked `bind_inverse`, and the
/// composed palette matrix `world × bind_inverse`. Decomposition
/// (translation + rotation as quat + scale) is the readable form;
/// full 16-float matrices follow on continuation lines so a
/// hand-computation against `skinning_e2e`'s working baseline can
/// pinpoint the diverging slot.
///
/// Pairs with the [`SKIN_DROPOUT_DUMPED`] Once-gated warn at
/// `render.rs:348` — that path emits a one-shot `(slot, was_None)`
/// summary; this command emits the full palette on demand.
///
/// Lock pattern: read-only on `SkinnedMesh` + `GlobalTransform` +
/// `Name` (matches `animation_system`'s declared accesses), so safe
/// to invoke from the debug-server CLI mid-frame.
///
/// `[`SKIN_DROPOUT_DUMPED`]: crate::render::SKIN_DROPOUT_DUMPED
struct SkinDumpCommand;
impl ConsoleCommand for SkinDumpCommand {
    fn name(&self) -> &str {
        "skin.dump"
    }
    fn description(&self) -> &str {
        "Dump per-bone palette for one entity: skin.dump <entity_id> (#841)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let entity: u32 = match args.trim().parse() {
            Ok(v) => v,
            Err(_) => return CommandOutput::line("Usage: skin.dump <entity_id>"),
        };
        let Some(skin) = world.get::<SkinnedMesh>(entity) else {
            return CommandOutput::line(format!("Entity {} has no SkinnedMesh component", entity));
        };
        let lines = format_skin_dump(world, entity, &skin);
        CommandOutput::lines(lines)
    }
}

/// Pure formatter — kept separate from the command impl so the test
/// can drive it without standing up a `ConsoleCommand` dispatcher.
fn format_skin_dump(world: &World, entity: u32, skin: &SkinnedMesh) -> Vec<String> {
    let pool = world.try_resource::<StringPool>();
    let mut lines = vec![format!(
        "SkinnedMesh dump for entity {} ({} bones):",
        entity,
        skin.bones.len()
    )];
    if let Some(root) = skin.skeleton_root {
        lines.push(format!("  skeleton_root: entity {}", root));
    } else {
        lines.push("  skeleton_root: (none)".to_string());
    }
    if skin.global_skin_transform != Mat4::IDENTITY {
        lines.push("  global_skin_transform: NON-IDENTITY (informational; not multiplied at runtime)".to_string());
        lines.push(format!(
            "    {}",
            format_mat4_row(&skin.global_skin_transform)
        ));
    } else {
        lines.push("  global_skin_transform: identity".to_string());
    }
    lines.push(String::new());
    lines.push(format!(
        "  {:>4} {:>10} {:<24} {:<11} {:<11} {:<11}",
        "slot", "entity", "name", "world(T)", "bind_inv(T)", "palette(T)"
    ));
    for (i, (maybe_bone, bind_inv)) in skin.bones.iter().zip(skin.bind_inverses.iter()).enumerate()
    {
        let (entity_str, name_str, world_mat) = match maybe_bone {
            Some(bone_e) => {
                let name = world
                    .get::<Name>(*bone_e)
                    .and_then(|n| pool.as_ref().and_then(|p| p.resolve(n.0)))
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "(no Name)".to_string());
                let world_mat = world
                    .get::<GlobalTransform>(*bone_e)
                    .map(|gt| gt.to_matrix());
                (format!("{}", bone_e), name, world_mat)
            }
            None => ("(None)".to_string(), "(unresolved)".to_string(), None),
        };
        let world_t = world_mat
            .map(|m| format_translation(&m))
            .unwrap_or_else(|| "(no GT)".to_string());
        let bind_t = format_translation(bind_inv);
        let palette = world_mat.map(|w| w * *bind_inv).unwrap_or(Mat4::IDENTITY);
        let pal_t = format_translation(&palette);
        lines.push(format!(
            "  {:>4} {:>10} {:<24} {:<11} {:<11} {:<11}",
            i,
            entity_str,
            truncate(&name_str, 24),
            world_t,
            bind_t,
            pal_t
        ));
        // Continuation lines: full matrices (one row of `world`,
        // `bind_inverse`, `palette`). Operators copy these into a
        // diff against `skinning_e2e`'s working baseline to find
        // the diverging slot per the #841 plan.
        if let Some(w) = world_mat {
            lines.push(format!("       world:   {}", format_mat4_row(&w)));
        }
        lines.push(format!("       bind_inv:{}", format_mat4_row(bind_inv)));
        lines.push(format!("       palette: {}", format_mat4_row(&palette)));
    }
    lines
}

fn format_translation(m: &Mat4) -> String {
    let t = m.w_axis;
    format!("({:.2},{:.2},{:.2})", t.x, t.y, t.z)
}

fn format_mat4_row(m: &Mat4) -> String {
    // Print matrix in row-major order on one line for grep/diff
    // friendliness. Column-vector convention — column N is
    // m.{x,y,z,w}_axis.
    let c = m.to_cols_array();
    format!(
        "[{:>7.3} {:>7.3} {:>7.3} {:>7.3} | {:>7.3} {:>7.3} {:>7.3} {:>7.3} | {:>7.3} {:>7.3} {:>7.3} {:>7.3} | {:>7.3} {:>7.3} {:>7.3} {:>7.3}]",
        c[0], c[4], c[8], c[12],
        c[1], c[5], c[9], c[13],
        c[2], c[6], c[10], c[14],
        c[3], c[7], c[11], c[15],
    )
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}

/// `mem.frag` — compute and emit a per-block GPU memory fragmentation
/// report. Pulls the live `gpu_allocator` report through the
/// `AllocatorResource` newtype the binary inserts at engine init, so
/// the calculation only runs when the user explicitly asks for it (the
/// audit `AUDIT_PERFORMANCE_2026-04-20.md` D2-L1 explicitly forbids
/// per-frame fragmentation calc). Reports the worst block's
/// `largest_free / total_free` ratio and warns when any block falls
/// below 0.5 — the signal that a long-running session has fragmented
/// enough that future allocations may fail despite headline "free
/// bytes" being adequate. See #503.
struct MemFragCommand;
impl ConsoleCommand for MemFragCommand {
    fn name(&self) -> &str {
        "mem.frag"
    }
    fn description(&self) -> &str {
        "Show per-block GPU memory fragmentation (#503 D2-L1)"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(alloc) =
            world.try_resource::<byroredux_renderer::vulkan::allocator::AllocatorResource>()
        else {
            return CommandOutput::line(
                "AllocatorResource not present — renderer not initialized yet",
            );
        };
        let report = alloc
            .0
            .lock()
            .expect("allocator lock poisoned")
            .generate_report();
        let frags = byroredux_renderer::vulkan::allocator::compute_block_fragmentation(&report);
        CommandOutput::lines(
            byroredux_renderer::vulkan::allocator::fragmentation_report_lines(&frags),
        )
    }
}

/// Dump the active scene lighting resources — cell ambient / directional,
/// sky / sun, current game time. Companion to `tex.missing` for diagnosing
/// "scene is too dark" symptoms without grepping logs (#890 followup —
/// see Markarth investigation 2026-05-10). The output flags the
/// resource-not-present case explicitly so it's obvious whether the
/// engine is on the procedural fallback, a resolved WTHR / CLMT, or a
/// per-cell XCLL/LGTM override.
struct LightDumpCommand;
impl ConsoleCommand for LightDumpCommand {
    fn name(&self) -> &str {
        "light.dump"
    }
    fn description(&self) -> &str {
        "Dump active CellLightingRes + SkyParamsRes + GameTimeRes \
         (diagnoses 'scene is too dark' — see Markarth probe)"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let mut lines = Vec::new();

        // ── CellLightingRes ───────────────────────────────────────────
        match world.try_resource::<crate::components::CellLightingRes>() {
            Some(lit) => {
                lines.push("CellLightingRes:".to_string());
                lines.push(format!(
                    "  ambient            = [{:.3}, {:.3}, {:.3}]",
                    lit.ambient[0], lit.ambient[1], lit.ambient[2]
                ));
                lines.push(format!(
                    "  directional_color  = [{:.3}, {:.3}, {:.3}]",
                    lit.directional_color[0], lit.directional_color[1], lit.directional_color[2]
                ));
                lines.push(format!(
                    "  directional_dir    = [{:.3}, {:.3}, {:.3}]",
                    lit.directional_dir[0], lit.directional_dir[1], lit.directional_dir[2]
                ));
                lines.push(format!("  is_interior        = {}", lit.is_interior));
                lines.push(format!(
                    "  fog                = color=[{:.2}, {:.2}, {:.2}] near={:.1} far={:.1}",
                    lit.fog_color[0], lit.fog_color[1], lit.fog_color[2], lit.fog_near, lit.fog_far
                ));
                // Extended XCLL — surface presence so the user can tell
                // apart "engine fallback" / "LGTM template" / "explicit XCLL".
                lines.push("  XCLL extended:".to_string());
                lines.push(format!(
                    "    directional_fade    = {}",
                    fmt_opt_f32(lit.directional_fade)
                ));
                lines.push(format!(
                    "    fog_clip / fog_power= {} / {}",
                    fmt_opt_f32(lit.fog_clip),
                    fmt_opt_f32(lit.fog_power)
                ));
                lines.push(format!(
                    "    fog_far_color       = {}",
                    fmt_opt_rgb(lit.fog_far_color)
                ));
                lines.push(format!(
                    "    fog_max             = {}",
                    fmt_opt_f32(lit.fog_max)
                ));
                lines.push(format!(
                    "    light_fade_begin/end= {} / {}",
                    fmt_opt_f32(lit.light_fade_begin),
                    fmt_opt_f32(lit.light_fade_end)
                ));
                lines.push(format!(
                    "    directional_ambient = {}",
                    if lit.directional_ambient.is_some() {
                        "present (Skyrim+ 6-axis cube)"
                    } else {
                        "None"
                    }
                ));
                lines.push(format!(
                    "    specular            = color={} alpha={}",
                    fmt_opt_rgb(lit.specular_color),
                    fmt_opt_f32(lit.specular_alpha)
                ));
                lines.push(format!(
                    "    fresnel_power       = {}",
                    fmt_opt_f32(lit.fresnel_power)
                ));
            }
            None => {
                lines.push("CellLightingRes: <not present — no cell loaded yet>".to_string());
            }
        }
        lines.push(String::new());

        // ── SkyParamsRes ──────────────────────────────────────────────
        match world.try_resource::<crate::components::SkyParamsRes>() {
            Some(sky) => {
                lines.push("SkyParamsRes:".to_string());
                lines.push(format!(
                    "  sun_direction      = [{:.3}, {:.3}, {:.3}]",
                    sky.sun_direction[0], sky.sun_direction[1], sky.sun_direction[2]
                ));
                lines.push(format!(
                    "  sun_color          = [{:.3}, {:.3}, {:.3}]",
                    sky.sun_color[0], sky.sun_color[1], sky.sun_color[2]
                ));
                lines.push(format!("  sun_intensity      = {:.3}", sky.sun_intensity));
                lines.push(format!("  sun_size           = {:.4}", sky.sun_size));
                lines.push(format!("  is_exterior        = {}", sky.is_exterior));
                lines.push(format!(
                    "  zenith             = [{:.3}, {:.3}, {:.3}]",
                    sky.zenith_color[0], sky.zenith_color[1], sky.zenith_color[2]
                ));
                lines.push(format!(
                    "  horizon            = [{:.3}, {:.3}, {:.3}]",
                    sky.horizon_color[0], sky.horizon_color[1], sky.horizon_color[2]
                ));
                lines.push(format!(
                    "  lower              = [{:.3}, {:.3}, {:.3}]",
                    sky.lower_color[0], sky.lower_color[1], sky.lower_color[2]
                ));
                lines.push(format!(
                    "  clouds (tile/tex)  = [{:.2}/{}] [{:.2}/{}] [{:.2}/{}] [{:.2}/{}]",
                    sky.cloud_tile_scale,
                    sky.cloud_texture_index,
                    sky.cloud_tile_scale_1,
                    sky.cloud_texture_index_1,
                    sky.cloud_tile_scale_2,
                    sky.cloud_texture_index_2,
                    sky.cloud_tile_scale_3,
                    sky.cloud_texture_index_3
                ));
                lines.push(format!(
                    "  sun_texture_index  = {} ({})",
                    sky.sun_texture_index,
                    if sky.sun_texture_index == 0 {
                        "procedural disc fallback"
                    } else {
                        "CLMT FNAM sprite"
                    }
                ));
            }
            None => {
                lines.push("SkyParamsRes: <not present — no exterior cell loaded>".to_string());
            }
        }
        lines.push(String::new());

        // ── GameTimeRes ───────────────────────────────────────────────
        match world.try_resource::<crate::components::GameTimeRes>() {
            Some(gt) => {
                let h = gt.hour.rem_euclid(24.0);
                let hour_int = h.floor() as u32;
                let minute_int = ((h - h.floor()) * 60.0).floor() as u32;
                let suffix = if hour_int < 12 { "AM" } else { "PM" };
                let display_hour = match hour_int {
                    0 => 12,
                    1..=12 => hour_int,
                    _ => hour_int - 12,
                };
                lines.push("GameTimeRes:".to_string());
                lines.push(format!(
                    "  hour          = {:.3} ({}:{:02} {})",
                    gt.hour, display_hour, minute_int, suffix
                ));
                lines.push(format!(
                    "  time_scale    = {:.1}\u{00d7} real-time",
                    gt.time_scale
                ));
            }
            None => {
                lines.push("GameTimeRes: <not present>".to_string());
            }
        }

        CommandOutput::lines(lines)
    }
}

fn fmt_opt_f32(v: Option<f32>) -> String {
    match v {
        Some(x) => format!("{:.3}", x),
        None => "None".to_string(),
    }
}

fn fmt_opt_rgb(v: Option<[f32; 3]>) -> String {
    match v {
        Some(c) => format!("[{:.3}, {:.3}, {:.3}]", c[0], c[1], c[2]),
        None => "None".to_string(),
    }
}

pub(crate) fn build_command_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    registry.register(HelpCommand);
    registry.register(StatsCommand);
    registry.register(EntitiesCommand);
    registry.register(SystemsCommand);
    registry.register(TexMissingCommand);
    registry.register(TexLoadedCommand);
    registry.register(MeshInfoCommand);
    registry.register(MeshCacheCommand);
    registry.register(CtxScratchCommand);
    registry.register(SkinCoverageCommand);
    registry.register(PridCommand);
    registry.register(CamWhereCommand);
    registry.register(CamPosCommand);
    registry.register(CamTpCommand);
    registry.register(SysAccessesCommand);
    registry.register(SkinListCommand);
    registry.register(SkinDumpCommand);
    registry.register(MemFragCommand);
    registry.register(LightDumpCommand);
    registry
}


#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
