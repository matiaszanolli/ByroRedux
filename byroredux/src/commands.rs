//! Console commands for the engine's built-in command system.

use byroredux_core::console::{CommandOutput, CommandRegistry, ConsoleCommand};
use byroredux_core::ecs::{
    AccessConflict, Camera, ConflictKind, DebugStats, GlobalTransform, Material, MeshHandle, Name,
    SchedulerAccessReport, ScratchTelemetry, SkinnedMesh, TextureHandle, Transform, World,
};
use byroredux_core::math::Mat4;
use byroredux_core::string::StringPool;
use std::collections::HashMap;

use byroredux_core::ecs::SystemList;

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
            format!("Meshes:    {}", stats.mesh_count),
            format!("Textures:  {}", stats.texture_count),
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
                reg.parsed_count,
                reg.failed_count,
            ),
            format!("  capacity:      {}", cap_str),
            format!("  lifetime hits: {}", reg.hits),
            format!("  lifetime miss: {}", reg.misses),
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
            return CommandOutput::line(format!(
                "Entity {} has no SkinnedMesh component",
                entity
            ));
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
        lines.push(format!(
            "  global_skin_transform: NON-IDENTITY (informational; not multiplied at runtime)"
        ));
        lines.push(format!("    {}", format_mat4_row(&skin.global_skin_transform)));
    } else {
        lines.push("  global_skin_transform: identity".to_string());
    }
    lines.push(String::new());
    lines.push(format!(
        "  {:>4} {:>10} {:<24} {:<11} {:<11} {:<11}",
        "slot", "entity", "name", "world(T)", "bind_inv(T)", "palette(T)"
    ));
    for (i, (maybe_bone, bind_inv)) in skin
        .bones
        .iter()
        .zip(skin.bind_inverses.iter())
        .enumerate()
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
        let palette = world_mat
            .map(|w| w * *bind_inv)
            .unwrap_or(Mat4::IDENTITY);
        let pal_t = format_translation(&palette);
        lines.push(format!(
            "  {:>4} {:>10} {:<24} {:<11} {:<11} {:<11}",
            i, entity_str, truncate(&name_str, 24), world_t, bind_t, pal_t
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
    registry.register(SysAccessesCommand);
    registry.register(SkinListCommand);
    registry.register(SkinDumpCommand);
    registry.register(MemFragCommand);
    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::{GlobalTransform, Name, SkinnedMesh, Transform};
    use byroredux_core::ecs::World;
    use byroredux_core::math::{Quat, Vec3};

    /// `skin.dump` regression for #841 — the dump must surface the
    /// resolved bone entity, its `Name`, the per-bone `bind_inverse`
    /// translation, and the composed palette translation in the
    /// summary table. Identity-fallback bones (no GT for the bone
    /// entity) print `(no GT)` and `palette: identity` so the
    /// `SKIN_DROPOUT_DUMPED` slots are obvious in the dump.
    #[test]
    fn skin_dump_renders_resolved_bone_with_world_and_palette() {
        let mut world = World::new();
        let mut pool = StringPool::new();
        let bip01 = pool.intern("Bip01 Spine");
        world.insert_resource(pool);

        // Bone entity: positioned at world (0, 5, 0), with a Name.
        let bone = world.spawn();
        world.insert(bone, Transform::from_translation(Vec3::new(0.0, 5.0, 0.0)));
        world.insert(
            bone,
            GlobalTransform::new(Vec3::new(0.0, 5.0, 0.0), Quat::IDENTITY, 1.0),
        );
        world.insert(bone, Name(bip01));

        // Skinned mesh: one bone, bind_inverse cancels the bind-pose
        // translation so palette = world * bind_inv = identity (the
        // canonical "bone hasn't moved relative to bind" case).
        let bind_inv = Mat4::from_translation(Vec3::new(0.0, -5.0, 0.0));
        let skin_entity = world.spawn();
        let skin = SkinnedMesh::new(Some(bone), vec![Some(bone)], vec![bind_inv]);
        let lines = format_skin_dump(&world, skin_entity, &skin);
        let dump = lines.join("\n");

        // Header — dump is for the right entity and bone count.
        assert!(
            dump.contains(&format!("dump for entity {} (1 bones)", skin_entity)),
            "header missing or wrong: {}",
            dump
        );
        // Bone slot 0 row: shows resolved bone entity + Name +
        // world_t (0, 5, 0) + bind_inv_t (0, -5, 0) + palette_t (0, 0, 0).
        assert!(dump.contains("bip01 spine"), "Name missing: {}", dump);
        assert!(
            dump.contains("(0.00,5.00,0.00)"),
            "world translation missing: {}",
            dump
        );
        assert!(
            dump.contains("(0.00,-5.00,0.00)"),
            "bind_inv translation missing: {}",
            dump
        );
        // palette = T(0,5,0) * T(0,-5,0) = identity → translation (0,0,0).
        assert!(
            dump.contains("(0.00,0.00,0.00)"),
            "palette translation missing: {}",
            dump
        );
    }

    #[test]
    fn skin_dump_marks_unresolved_bone_slots() {
        // Phase 1b.x DROPOUT scenario — a bone slot that didn't
        // resolve to an entity must show up as `(None)` /
        // `(unresolved)` so the operator can correlate against the
        // SKIN_DROPOUT_DUMPED warn.
        let mut world = World::new();
        world.insert_resource(StringPool::new());
        let skin_entity = world.spawn();
        let skin = SkinnedMesh::new(None, vec![None], vec![Mat4::IDENTITY]);
        let lines = format_skin_dump(&world, skin_entity, &skin);
        let dump = lines.join("\n");
        assert!(dump.contains("(None)"), "unresolved entity missing: {}", dump);
        assert!(dump.contains("(unresolved)"), "unresolved name missing: {}", dump);
    }

    #[test]
    fn skin_dump_reports_non_identity_global_skin_transform() {
        // A non-identity `global_skin_transform` is informational
        // (not multiplied at runtime) but its presence is exactly
        // the kind of authoring quirk #841 surfaced on Doc Mitchell;
        // the dump must call it out so it isn't missed.
        let mut world = World::new();
        world.insert_resource(StringPool::new());
        let skin_entity = world.spawn();
        let global = Mat4::from_quat(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2));
        let skin = SkinnedMesh::new_with_global(None, vec![], vec![], global);
        let lines = format_skin_dump(&world, skin_entity, &skin);
        let dump = lines.join("\n");
        assert!(
            dump.contains("global_skin_transform: NON-IDENTITY"),
            "non-identity global must be flagged: {}",
            dump
        );
    }
}
