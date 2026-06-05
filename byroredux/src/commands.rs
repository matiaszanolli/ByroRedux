//! Console commands for the engine's built-in command system.

use byroredux_core::console::{CommandOutput, CommandRegistry, ConsoleCommand};
use byroredux_core::ecs::{
    AccessConflict, ActiveCamera, Camera, ConflictKind, DebugStats, EntityId, GlobalTransform,
    LightSource, Material, MeshHandle, Name, Parent, ParticleEmitter, SchedulerAccessReport,
    ScratchTelemetry, SceneFlags, SelectedRef, SkinCoverageStats, SkinnedMesh, TextureHandle,
    Transform, World, WorldBound,
};
use byroredux_core::ecs::components::{CollisionShape, FormIdComponent, RenderLayer, RigidBodyData};
use crate::components::{AlphaBlend, DoorTeleport, InputState, IsFxMesh, IsCollisionOnly, TwoSided};
use crate::helpers::world_resource_set;
use byroredux_core::math::{Mat4, Quat, Vec3};
use byroredux_core::string::StringPool;
use std::collections::HashMap;

use byroredux_core::ecs::SystemList;
use crate::cell_loader::{
    LoadedCellIndex, LoadedPluginSet, PendingCellTransition, PendingCellTransitionSlot,
    TransitionDestination,
};

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
            // #1258 / PERF-D3-NEW-03 — three-line view of the draw
            // pipeline: input to the batcher, post-merge batch count,
            // and actual GPU draw call count. Pre-fix only the first
            // number was surfaced under the misleading label "Draws",
            // which made every perf audit's "µs/draw" arithmetic use
            // the wrong denominator.
            format!(
                "Draws:     {} cmds → {} batches → {} GPU calls",
                stats.draw_command_count, stats.batch_count, stats.indirect_call_count
            ),
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
        let mesh_count = world.count::<MeshHandle>();
        let collision_count = world.count::<CollisionShape>();
        let collision_only_count = world.count::<IsCollisionOnly>();

        // Entities with CollisionShape but no MeshHandle are pure physics
        // proxies (bhk-authored or synthesized ghost entities). These carry
        // no GPU footprint — no BLAS, no TLAS instance, no render cost.
        let physics_only_count = match world.query::<CollisionShape>() {
            Some(cq) => {
                let mesh_q = world.query::<MeshHandle>();
                cq.iter()
                    .filter(|(e, _)| mesh_q.as_ref().map_or(true, |mq| !mq.contains(*e)))
                    .count()
            }
            None => 0,
        };

        let mut lines = vec![format!("Total entities spawned: {}", total)];
        lines.push(format!("  Transform:           {}", world.count::<Transform>()));
        lines.push(format!("  MeshHandle (render): {}", mesh_count));
        lines.push(format!("  TextureHandle:       {}", world.count::<TextureHandle>()));
        lines.push(format!("  Camera:              {}", world.count::<Camera>()));
        lines.push(format!("  CollisionShape:      {}", collision_count));
        lines.push(format!(
            "    physics-only (no MeshHandle): {}",
            physics_only_count
        ));
        lines.push(format!(
            "    IsCollisionOnly (render+phys combined, expect 0): {}",
            collision_only_count
        ));
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
        "List entities with fallback (checkerboard) texture and their expected paths \
         (use `tex.missing entities` to sample entity IDs per bucket)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let want_entities = args.trim().eq_ignore_ascii_case("entities");
        let tex_q = world.query::<TextureHandle>();
        let mat_q = world.query::<Material>();
        let (Some(tex_q), Some(mat_q)) = (tex_q, mat_q) else {
            return CommandOutput::line("No TextureHandle or Material components found");
        };

        // Bucket aggregator: path → (count, first-N entity IDs).
        let mut missing: HashMap<String, (u32, Vec<EntityId>)> = HashMap::new();
        const ENTITY_SAMPLE_LIMIT: usize = 5;
        for (entity, tex) in tex_q.iter() {
            if tex.0 != 0 {
                continue;
            }
            let mat = mat_q.get(entity);
            let path = mat
                .and_then(|m| m.texture_path.as_deref())
                .or_else(|| mat.and_then(|m| m.material_path.as_deref()))
                .unwrap_or("<no path, no material>");
            let slot = missing.entry(path.to_string()).or_insert((0, Vec::new()));
            slot.0 += 1;
            if slot.1.len() < ENTITY_SAMPLE_LIMIT {
                slot.1.push(entity);
            }
        }

        if missing.is_empty() {
            return CommandOutput::line(
                "No missing textures — all entities have resolved textures",
            );
        }

        let mut sorted: Vec<_> = missing.into_iter().collect();
        sorted.sort_by(|a, b| b.1.0.cmp(&a.1.0));

        let mut lines = vec![format!("{} unique missing textures:", sorted.len())];
        for (path, (count, samples)) in sorted.iter().take(50) {
            if want_entities {
                let sample_str = samples
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                lines.push(format!("  {:4}x  {}  [ids: {}]", count, path, sample_str));
            } else {
                lines.push(format!("  {:4}x  {}", count, path));
            }
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
        "Show mesh/texture/material/transform/parent/FormID for an entity: mesh.info <entity_id>"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let id: u32 = match args.trim().parse() {
            Ok(v) => v,
            Err(_) => return CommandOutput::line("Usage: mesh.info <entity_id>"),
        };
        let name = resolve_entity_name(world, id);
        let mut lines = vec![match &name {
            Some(n) => format!("Entity {} ({}):", id, n),
            None => format!("Entity {}:", id),
        }];

        // Local Transform: translation / Euler-from-quat (degrees) / scale.
        // Euler is for human reading only — the canonical rotation is the
        // quat; we surface ZYX-extracted Euler so it matches the
        // FNVEdit / CK convention the user sees in the ESM (REFR DATA
        // stores RX RY RZ as ZYX-composed Euler in radians).
        if let Some(t) = world.get::<Transform>(id) {
            let (z, y, x) = t.rotation.to_euler(byroredux_core::math::EulerRot::ZYX);
            lines.push(format!(
                "  Transform.local:   pos ({:>+8.2},{:>+8.2},{:>+8.2})",
                t.translation.x, t.translation.y, t.translation.z
            ));
            lines.push(format!(
                "                     rot (deg ZYX-extracted) rx={:>+7.2}  ry={:>+7.2}  rz={:>+7.2}",
                x.to_degrees(),
                y.to_degrees(),
                z.to_degrees()
            ));
            lines.push(format!(
                "                     scale {:.3}  (uniform)",
                t.scale
            ));
        } else {
            lines.push("  Transform.local:   (none)".to_string());
        }
        if let Some(gt) = world.get::<GlobalTransform>(id) {
            lines.push(format!(
                "  Transform.global:  pos ({:>+8.2},{:>+8.2},{:>+8.2})",
                gt.translation.x, gt.translation.y, gt.translation.z
            ));
        } else {
            lines.push("  Transform.global:  (none)".to_string());
        }

        // Parent chain walk → first FormIdComponent up the tree. The
        // FormID is attached only at the REFR placement_root by
        // spawn.rs:183 (#1212), so a mesh-sub-entity must walk up
        // through its BSFadeNode / NiNode parents to find it.
        let mut chain: Vec<EntityId> = vec![id];
        let mut current = id;
        let mut found_form: Option<byroredux_core::form_id::FormId> = None;
        if let Some(fid) = world.get::<FormIdComponent>(current) {
            found_form = Some(fid.0);
        }
        // Cap the walk to guard against a hypothetical cyclic parent
        // (would indicate a deeper invariant bug we'd want to surface).
        for _ in 0..32 {
            let Some(parent) = world.get::<Parent>(current).map(|p| p.0) else {
                break;
            };
            chain.push(parent);
            if found_form.is_none() {
                if let Some(fid) = world.get::<FormIdComponent>(parent) {
                    found_form = Some(fid.0);
                }
            }
            current = parent;
        }
        if chain.len() > 1 {
            let chain_str = chain
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join(" -> ");
            lines.push(format!("  Parent chain:      {}", chain_str));
        } else {
            lines.push("  Parent chain:      (root, no Parent component)".to_string());
        }
        match found_form {
            Some(fid) => {
                // Resolve runtime FormId → plugin-local FormIdPair via
                // the FormIdPool resource. The `LocalFormId(u32)` is the
                // 24-bit FNVEdit / xEdit handle the user can paste into
                // those tools. PluginId is content-addressed (UUID v5
                // of the filename), so we surface it as a 32-bit prefix
                // for at-a-glance disambiguation between masters.
                if let Some(pool) = world.try_resource::<byroredux_core::form_id::FormIdPool>() {
                    if let Some(pair) = pool.resolve(fid) {
                        let plugin_prefix = (pair.plugin.0 >> 96) as u32;
                        lines.push(format!(
                            "  REFR FormID:       0x{:06X}  (plugin uuid hi32 = 0x{:08X})",
                            pair.local.0 & 0x00FF_FFFF,
                            plugin_prefix
                        ));
                    } else {
                        lines.push(
                            "  REFR FormID:       <unresolved in FormIdPool>".to_string(),
                        );
                    }
                } else {
                    lines.push("  REFR FormID:       <FormIdPool resource missing>".to_string());
                }
            }
            None => lines.push("  REFR FormID:       (none found in parent chain)".to_string()),
        }

        if let Some(mh) = world.get::<MeshHandle>(id) {
            lines.push(format!("  MeshHandle:        {}", mh.0));
        } else {
            lines.push("  MeshHandle:        (none)".to_string());
        }
        if let Some(th) = world.get::<TextureHandle>(id) {
            lines.push(format!(
                "  TextureHandle:     {}{}",
                th.0,
                if th.0 == 0 { " (FALLBACK)" } else { "" }
            ));
        } else {
            lines.push("  TextureHandle:     (none)".to_string());
        }
        if let Some(mat) = world.get::<Material>(id) {
            lines.push(format!(
                "  texture_path:      {}",
                mat.texture_path.as_deref().unwrap_or("(none)")
            ));
            lines.push(format!(
                "  material_path:     {}",
                mat.material_path.as_deref().unwrap_or("(none)")
            ));
            lines.push(format!(
                "  normal_map:        {}",
                mat.normal_map.as_deref().unwrap_or("(none)")
            ));
            lines.push(format!(
                "  glow_map:          {}",
                mat.glow_map.as_deref().unwrap_or("(none)")
            ));
            lines.push(format!(
                "  detail/gloss/dark: {}/{}/{}",
                if mat.detail_map.is_some() { "set" } else { "-" },
                if mat.gloss_map.is_some() { "set" } else { "-" },
                if mat.dark_map.is_some() { "set" } else { "-" },
            ));
            // Material kind / shader-type enum (0 = Default lit, 1 = Envmap,
            // 2 = Glow, … 20 vanilla; 100+ synthesized for GLASS / FX).
            // Critical for diagnosing "what shader path did this take?" —
            // an entity with material_kind=0 + no texture is a different
            // bug class from an entity with material_kind=20 + no texture.
            lines.push(format!("  material_kind:     {}", mat.material_kind));
            // PBR convergence ground-truth (canonical-material pass).
            // `metalness` / `roughness` are now resolved canonical
            // scalars (BGSM-authored or keyword-classified, fully
            // resolved once at `translate_material` — no render-time
            // fallback). Showing them alongside the legacy `glossiness`
            // input lets the per-game material-divergence sweep read the
            // convention each game produces, with the materials BA2
            // loaded (so FO4/Skyrim BGSM values appear).
            lines.push(format!(
                "  pbr metal/rough/gloss: {:.2} / {:.2} / {:.0}",
                mat.metalness, mat.roughness, mat.glossiness,
            ));
            lines.push(format!(
                "  alpha (val/test):  {:.2} / test={} (thr={:.2}, func={})",
                mat.alpha, mat.alpha_test, mat.alpha_threshold, mat.alpha_test_func
            ));
            lines.push(format!(
                "  emissive (mult/color): {:.2} / [{:.2},{:.2},{:.2}]",
                mat.emissive_mult,
                mat.emissive_color[0], mat.emissive_color[1], mat.emissive_color[2],
            ));
            // Effect-shader flags + env_map_scale + vertex_color_mode all
            // tell us what *kind* of material this is even if no texture
            // path resolved. A non-zero effect_shader_flags or non-default
            // vertex_color_mode strongly hints at which importer path
            // populated the (empty-path) Material.
            lines.push(format!(
                "  effect_flags / env / vcm: 0x{:08X} / {:.2} / {}",
                mat.effect_shader_flags, mat.env_map_scale, mat.vertex_color_mode
            ));
        } else {
            lines.push("  Material:          (none)".to_string());
        }
        // Marker components — these tell us what category of render path
        // the entity routes through. A bare "no Material" entity that
        // *also* carries AlphaBlend or TwoSided proves an importer arm
        // populated some of the shape but not the texture/material.
        let mut markers: Vec<String> = Vec::new();
        if let Some(ab) = world.get::<AlphaBlend>(id) {
            markers.push(format!("AlphaBlend(src={}, dst={})", ab.src_blend, ab.dst_blend));
        }
        if world.get::<TwoSided>(id).is_some() {
            markers.push("TwoSided".to_string());
        }
        if world.get::<IsFxMesh>(id).is_some() {
            markers.push("IsFxMesh".to_string());
        }
        if let Some(rl) = world.get::<RenderLayer>(id) {
            markers.push(format!("RenderLayer({:?})", *rl));
        }
        if let Some(sf) = world.get::<SceneFlags>(id) {
            markers.push(format!("SceneFlags(0x{:08X})", sf.0));
        }
        if let Some(dt) = world.get::<DoorTeleport>(id) {
            markers.push(format!("DoorTeleport(→0x{:08X})", dt.destination_form_id));
        }
        if markers.is_empty() {
            lines.push("  Markers:           (none)".to_string());
        } else {
            lines.push(format!("  Markers:           {}", markers.join(", ")));
        }
        // Aux-component detection. Entities that have a Transform but no
        // MeshHandle look like "orphans" in the basic dump above — but
        // many of them are load-bearing collision shapes, light sources,
        // or particle emitters spawned by the cell loader. Surface their
        // presence explicitly so they stop looking like ghosts.
        let mut aux: Vec<&'static str> = Vec::new();
        if world.get::<CollisionShape>(id).is_some() {
            aux.push("CollisionShape");
        }
        if world.get::<RigidBodyData>(id).is_some() {
            aux.push("RigidBodyData");
        }
        if world.get::<LightSource>(id).is_some() {
            aux.push("LightSource");
        }
        if world.get::<ParticleEmitter>(id).is_some() {
            aux.push("ParticleEmitter");
        }
        if world.get::<SkinnedMesh>(id).is_some() {
            aux.push("SkinnedMesh");
        }
        if aux.is_empty() {
            lines.push("  Aux components:    (none)".to_string());
        } else {
            lines.push(format!("  Aux components:    {}", aux.join(", ")));
        }
        CommandOutput::lines(lines)
    }
}

/// `mesh.cache` — inspect the process-lifetime NIF import cache.
/// Reports cache size, parsed/failed counts, and lifetime hit rate.
/// `mesh.cache failed` enumerates every cached path whose parse
/// returned `None` — the source of "N failed" in the stats line.
/// See [`crate::cell_loader::NifImportRegistry`] / #381.
struct MeshCacheCommand;
impl ConsoleCommand for MeshCacheCommand {
    fn name(&self) -> &str {
        "mesh.cache"
    }
    fn description(&self) -> &str {
        "Show NIF import cache stats. `mesh.cache failed` lists every failed-parse path."
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(reg) = world.try_resource::<crate::cell_loader::NifImportRegistry>() else {
            return CommandOutput::line("NifImportRegistry resource not present");
        };
        if args.trim().eq_ignore_ascii_case("failed") {
            // Enumerate every cache entry whose value is `None` (negative
            // cache — parse returned None). The cache HashMap iteration
            // order is unspecified; sort the result so successive runs
            // produce comparable output.
            let mut failed: Vec<&str> = reg
                .core
                .cache
                .iter()
                .filter(|(_, v)| v.is_none())
                .map(|(k, _)| k.as_str())
                .collect();
            failed.sort_unstable();
            if failed.is_empty() {
                return CommandOutput::line("No failed NIF parses in cache.");
            }
            let mut lines = vec![format!("{} failed-parse paths:", failed.len())];
            for path in &failed {
                lines.push(format!("  {}", path));
            }
            return CommandOutput::lines(lines);
        }
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
            "  (use `mesh.cache failed` to list failed-parse paths)".to_string(),
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
            let mut line = format!(
                "  materials: {} unique / {} interned ({:.1}× dedup)",
                tlm.materials_unique, tlm.materials_interned, ratio,
            );
            if tlm.materials_overflow > 0 {
                line.push_str(&format!(
                    ", OVERFLOW {} → id 0 (raise MAX_MATERIALS)",
                    tlm.materials_overflow,
                ));
            }
            lines.push(line);
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
            "  dispatches_skipped     = {}  (#1194 — bone palette unchanged; \
             dispatch elided. PERF-DIM7-01 is the first consumer)",
            cov.dispatches_skipped,
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
        // #1194 — per-pass GPU timer. ms == 0.0 means either the
        // driver lacks timestampComputeAndGraphics OR the bracket
        // didn't fire this snapshot (skinned chain skipped, TAA
        // disabled, first pipelined cycle hasn't completed).
        lines.push(format!(
            "  gpu_skin_dispatch_ms   = {:.3}",
            cov.gpu_skin_dispatch_ms,
        ));
        lines.push(format!(
            "  gpu_skin_blas_refit_ms = {:.3}",
            cov.gpu_skin_blas_refit_ms,
        ));
        lines.push(format!(
            "  gpu_taa_ms             = {:.3}",
            cov.gpu_taa_ms,
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

/// `near [radius]` — list entities with `GlobalTransform` within
/// `radius` units of the active camera, sorted by distance ascending.
/// Default radius 300; the cap on the result list is 30 rows.
///
/// Use when there's no raycast picker and you need to identify the
/// REFR you're looking at — walk close to it, run `near 100`, eyeball
/// the closest hits for matching `texture_path` / `material_path` /
/// `Name`, then `prid <entity_id>` for the full inspect. The native
/// REFR rotation chain `(rx, ry, rz)` is NOT directly visible from
/// this command — for that, follow up with `prid` and then look up
/// the source REFR in the ESM via `dump_prospector_saloon_refrs`-
/// style tooling.
///
/// Output columns: distance, entity_id, `Name` (or `Material`-derived
/// label), texture/material path (whichever populated first), pos.
struct NearCommand;
impl ConsoleCommand for NearCommand {
    fn name(&self) -> &str {
        "near"
    }
    fn description(&self) -> &str {
        "List entities near the camera, sorted by distance (usage: near [radius=300])"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let radius: f32 = args.trim().parse().unwrap_or(300.0);
        let Some(active) = world.try_resource::<ActiveCamera>() else {
            return CommandOutput::line("ActiveCamera resource not present");
        };
        let cam_entity = active.0;
        drop(active);
        let cam_pos = world
            .query::<Transform>()
            .and_then(|q| q.get(cam_entity).map(|t| t.translation));
        let Some(cam_pos) = cam_pos else {
            return CommandOutput::line(format!(
                "Camera entity {cam_entity} has no Transform"
            ));
        };
        let Some(gtq) = world.query::<GlobalTransform>() else {
            return CommandOutput::line("GlobalTransform storage not present");
        };
        let r2 = radius * radius;
        let mut hits: Vec<(f32, EntityId, Vec3)> = Vec::new();
        for (entity, gt) in gtq.iter() {
            if entity == cam_entity {
                continue;
            }
            let pos = gt.translation;
            let d2 = (pos - cam_pos).length_squared();
            if d2 <= r2 {
                hits.push((d2.sqrt(), entity, pos));
            }
        }
        drop(gtq);
        hits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        if hits.is_empty() {
            return CommandOutput::line(format!(
                "no entities within {:.1} units of camera ({:.1},{:.1},{:.1})",
                radius, cam_pos.x, cam_pos.y, cam_pos.z
            ));
        }
        let take_n = 30.min(hits.len());
        let mut lines = Vec::with_capacity(take_n + 2);
        lines.push(format!(
            "camera at ({:.1},{:.1},{:.1}) — {} entities within {:.1} units \
             (showing nearest {}):",
            cam_pos.x,
            cam_pos.y,
            cam_pos.z,
            hits.len(),
            radius,
            take_n
        ));
        lines.push(format!(
            "{:>7}  {:>6}  {:<28}  {:<48}  {}",
            "dist", "id", "name", "tex/mat path", "position"
        ));
        for (dist, entity, pos) in hits.iter().take(take_n) {
            let name_str = resolve_entity_name(world, *entity).unwrap_or_else(|| "-".to_string());
            let path = world
                .get::<Material>(*entity)
                .and_then(|m| {
                    m.texture_path
                        .as_deref()
                        .or(m.material_path.as_deref())
                        .map(|s| s.to_string())
                })
                .unwrap_or_default();
            lines.push(format!(
                "{:>7.1}  {:>6}  {:<28.28}  {:<48.48}  ({:>+6.1},{:>+6.1},{:>+6.1})",
                dist, entity, name_str, path, pos.x, pos.y, pos.z
            ));
        }
        CommandOutput::lines(lines)
    }
}

/// `pick [count]` — ray-cast from the active camera along its forward
/// direction and list entities whose `WorldBound` sphere the ray
/// intersects, sorted by ray-parameter (closest first). Default count
/// 10.
///
/// Use this to identify "the thing I'm looking at" without the noise
/// of `near` (which lists everything within a radial sphere). Pair
/// with `mesh.info <id>` for the full inspect on the top hit.
///
/// Caveat: matches against bounding spheres only — a hit at the
/// nearest sphere's edge can register before a small geometry inside
/// a bigger sphere. The first 2-3 hits are usually what you want.
struct PickCommand;
impl ConsoleCommand for PickCommand {
    fn name(&self) -> &str {
        "pick"
    }
    fn description(&self) -> &str {
        "Ray-cast from camera forward; list entities the ray pierces (usage: pick [count=10])"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let count: usize = args.trim().parse().unwrap_or(10);
        let Some(active) = world.try_resource::<ActiveCamera>() else {
            return CommandOutput::line("ActiveCamera resource not present");
        };
        let cam_entity = active.0;
        drop(active);
        let cam_pos = world
            .query::<Transform>()
            .and_then(|q| q.get(cam_entity).map(|t| t.translation));
        let Some(cam_pos) = cam_pos else {
            return CommandOutput::line(format!(
                "Camera entity {cam_entity} has no Transform"
            ));
        };
        // Camera forward derived from InputState (yaw, pitch) the way
        // fly_camera_system computes it: forward = R_y(yaw)·R_x(pitch)·-Z.
        let (yaw, pitch) = world
            .try_resource::<InputState>()
            .map(|i| (i.yaw, i.pitch))
            .unwrap_or((0.0, 0.0));
        let cy = yaw.cos();
        let sy = yaw.sin();
        let cp = pitch.cos();
        let sp = pitch.sin();
        // forward = R_y(yaw) * R_x(pitch) * (0,0,-1)
        // R_x(pitch) * (0,0,-1) = (0, sin(pitch), -cos(pitch))
        // R_y(yaw)   * (0, sin(pitch), -cos(pitch)) =
        //   ( -sin(yaw)·cos(pitch), sin(pitch), -cos(yaw)·cos(pitch) )
        let forward = Vec3::new(-sy * cp, sp, -cy * cp);

        // Tier 1: proper WorldBound sphere — counts as a real hit.
        // Tier 2: GlobalTransform-only fallback — many entities ship
        // with `WorldBound::default()` (zero center, zero radius) when
        // the NIF importer didn't surface a usable local sphere. We
        // still want those entities in the pick list, so we synthesise
        // a 32-unit sphere at the entity's GlobalTransform.translation
        // (1 m at FNV scale — wide enough to catch a wall the camera
        // is hugging, tight enough to avoid grabbing the whole room).
        // Synthetic hits are flagged with `~` in the radius column so
        // the operator knows they're approximate, not authored.
        const SYNTH_RADIUS: f32 = 32.0;

        let Some(gtq) = world.query::<GlobalTransform>() else {
            return CommandOutput::line(
                "GlobalTransform storage not present (no entities to test against)",
            );
        };

        // Ray r(t) = cam_pos + t · forward; sphere center c, radius R.
        // Intersect when |r(t) - c|² = R². Quadratic in t:
        //   a = forward·forward = 1
        //   b = 2 · (cam_pos - c) · forward
        //   c = |cam_pos - c|² - R²
        // disc = b² - 4·a·c. disc >= 0 → at least one real root; take
        // the smaller positive root as the hit distance.
        let mut hits: Vec<(f32, EntityId, Vec3, f32, bool)> = Vec::new();
        for (entity, gt) in gtq.iter() {
            if entity == cam_entity {
                continue;
            }
            // Prefer authored WorldBound when present + non-degenerate.
            let (center, radius, synthetic) = match world.get::<WorldBound>(entity) {
                Some(wb) if wb.radius > 0.0 => (wb.center, wb.radius, false),
                _ => (gt.translation, SYNTH_RADIUS, true),
            };
            let oc = cam_pos - center;
            let b = 2.0 * oc.dot(forward);
            let cc = oc.length_squared() - radius * radius;
            let disc = b * b - 4.0 * cc;
            if disc < 0.0 {
                continue;
            }
            let sqrt_disc = disc.sqrt();
            let t0 = (-b - sqrt_disc) * 0.5;
            let t1 = (-b + sqrt_disc) * 0.5;
            // Pick the closer non-negative root. If both negative, the
            // sphere is entirely behind the camera — skip.
            let t = if t0 >= 0.0 {
                t0
            } else if t1 >= 0.0 {
                // Camera inside sphere — still counts as a hit but at
                // t=0 (we're inside it right now).
                0.0
            } else {
                continue;
            };
            hits.push((t, entity, center, radius, synthetic));
        }
        drop(gtq);
        hits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        if hits.is_empty() {
            return CommandOutput::line(format!(
                "no WorldBound spheres along ray from ({:.1},{:.1},{:.1}) dir ({:+.3},{:+.3},{:+.3})",
                cam_pos.x, cam_pos.y, cam_pos.z, forward.x, forward.y, forward.z
            ));
        }
        let take_n = count.min(hits.len()).max(1);
        let mut lines = Vec::with_capacity(take_n + 2);
        lines.push(format!(
            "ray from ({:.1},{:.1},{:.1}) dir ({:+.3},{:+.3},{:+.3}) — \
             {} hits (top {}):",
            cam_pos.x, cam_pos.y, cam_pos.z, forward.x, forward.y, forward.z,
            hits.len(), take_n
        ));
        lines.push(format!(
            "{:>7}  {:>6}  {:<28}  {:<48}  {:>7}  {}",
            "t", "id", "name", "tex/mat path", "r", "sphere center"
        ));
        for (t, entity, center, radius, synthetic) in hits.iter().take(take_n) {
            let name_str = resolve_entity_name(world, *entity).unwrap_or_else(|| "-".to_string());
            let path = world
                .get::<Material>(*entity)
                .and_then(|m| {
                    m.texture_path
                        .as_deref()
                        .or(m.material_path.as_deref())
                        .map(|s| s.to_string())
                })
                .unwrap_or_default();
            let radius_str = if *synthetic {
                format!("~{:.0}", radius)
            } else {
                format!("{:.1}", radius)
            };
            lines.push(format!(
                "{:>7.1}  {:>6}  {:<28.28}  {:<48.48}  {:>7}  ({:>+6.1},{:>+6.1},{:>+6.1})",
                t, entity, name_str, path, radius_str, center.x, center.y, center.z
            ));
        }
        CommandOutput::lines(lines)
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
            "Scheduler access report — {} systems, {} undeclared \
             ({} parallel + {} exclusive), {} known conflicts, \
             {} unknown pairs",
            report.system_count(),
            report.undeclared_count(),
            report.undeclared_parallel_count(),
            report.undeclared_exclusive_count(),
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

/// `light.atten [knee <f>] [legacy on|off]` — live tuning of the
/// point/spot light attenuation model for the REND-#1451 controlled
/// bench. With no args, prints the current state plus the effective
/// brightness at the authored LIGH radius so the bench can be read
/// numerically as well as visually.
///
/// Mutates the `LightTuning` resource; the draw loop pushes it into the
/// renderer (`VulkanContext::light_atten_knee` / `light_atten_legacy`)
/// before the next `draw_frame`, so changes take effect within a frame
/// with no rebuild. Pair with `--bench-hold` + `byro-dbg`.
struct LightAttenCommand;
impl ConsoleCommand for LightAttenCommand {
    fn name(&self) -> &str {
        "light.atten"
    }
    fn description(&self) -> &str {
        "Tune point/spot attenuation live (REND-#1451): light.atten [knee <0..1>] [legacy on|off]"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        use crate::components::LightTuning;

        if world.try_resource::<LightTuning>().is_none() {
            return CommandOutput::lines(vec![
                "light.atten: LightTuning resource not present (only available in a live render \
                 session, not the offline --cmd path)."
                    .to_string(),
            ]);
        }

        // Parse "knee <f>" / "legacy on|off" tokens. Unknown tokens are
        // reported rather than silently ignored.
        let mut errors: Vec<String> = Vec::new();
        let tokens: Vec<&str> = args.split_whitespace().collect();
        let mut i = 0;
        while i < tokens.len() {
            match tokens[i].to_ascii_lowercase().as_str() {
                "knee" => match tokens.get(i + 1).and_then(|s| s.parse::<f32>().ok()) {
                    Some(v) if (0.05..=1.0).contains(&v) => {
                        world_resource_set::<LightTuning>(world, |lt| lt.knee_frac = v);
                        i += 2;
                    }
                    _ => {
                        errors.push("knee expects a value in [0.05, 1.0]".to_string());
                        i += 2;
                    }
                },
                "legacy" => match tokens.get(i + 1).map(|s| s.to_ascii_lowercase()) {
                    Some(ref s) if s == "on" || s == "true" || s == "1" => {
                        world_resource_set::<LightTuning>(world, |lt| lt.legacy = true);
                        i += 2;
                    }
                    Some(ref s) if s == "off" || s == "false" || s == "0" => {
                        world_resource_set::<LightTuning>(world, |lt| lt.legacy = false);
                        i += 2;
                    }
                    _ => {
                        errors.push("legacy expects on|off".to_string());
                        i += 2;
                    }
                },
                other => {
                    errors.push(format!("unknown token `{other}`"));
                    i += 1;
                }
            }
        }

        // Read back the current (possibly updated) state and report.
        let (knee, legacy) = {
            let lt = world.try_resource::<LightTuning>().unwrap();
            (lt.knee_frac, lt.legacy)
        };

        let mut lines = Vec::new();
        for e in &errors {
            lines.push(format!("  ! {e}"));
        }
        lines.push("LightTuning (REND-#1451):".to_string());
        lines.push(format!("  knee_frac = {knee:.3}  (authored radius = knee × cull radius)"));
        lines.push(format!(
            "  legacy    = {}  ({})",
            legacy,
            if legacy {
                "pre-fix window-only — 75% at authored radius (the ring)"
            } else {
                "physical falloff × cull window"
            }
        ));

        // Effective brightness at the authored radius for the default
        // falloff shape — the headline number for the bench. Mirrors
        // `pointSpotAtten` in triangle.frag exactly.
        let ext = crate::render::lights::LIGHT_RANGE_EXTENSION;
        let shape = crate::render::lights::FALLOFF_EXPONENT_DEFAULT;
        let f_a = 1.0 / ext; // authored radius as a fraction of the cull radius
        let pct = if legacy {
            let w = (1.0 - f_a * f_a).clamp(0.0, 1.0);
            w.powf(shape)
        } else {
            let dn = f_a / knee;
            let phys = 1.0 / (1.0 + shape * dn * dn);
            // smoothstep(knee, 1.0, f_a)
            let t = ((f_a - knee) / (1.0 - knee)).clamp(0.0, 1.0);
            let cull = 1.0 - t * t * (3.0 - 2.0 * t);
            phys * cull
        };
        lines.push(format!(
            "  → atten at authored radius (shape {shape:.1}, ext {ext:.1}) = {:.0}%",
            pct * 100.0
        ));
        lines.push(
            "  usage: light.atten knee 0.4  |  light.atten legacy on  |  light.atten legacy off"
                .to_string(),
        );

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

/// `door.teleport <entity_id>` — fire a cell transition through a
/// door's XTEL destination.
///
/// Pipeline:
///   1. Read [`DoorTeleport`] on the entity → destination form-id +
///      Bethesda-Z-up position / rotation.
///   2. Resolve form-id → parent cell via [`LoadedCellIndex`].
///   3. Queue a [`PendingCellTransition`] for the main loop to consume
///      next frame. The orchestrator unloads the current cell, loads
///      the destination, and repositions the camera.
///
/// The queueing pattern works around the console-command system's
/// `&World`-only access — actual cell load/unload requires `&mut
/// World + &mut VulkanContext` which only the main loop has. The
/// deferred resource is consumed by `step_cell_transition` in
/// `main.rs`.
///
/// Stage 3a scope: interior-cell destinations are wired end-to-end.
/// Exterior destinations queue the request but the orchestrator
/// returns `NotImplemented` (Stage 3b will spin up the streaming
/// state). Either way the command reports its findings.
///
/// Natural usage:
///   1. `cargo run -- --esm FalloutNV.esm --cell GSDocMitchellHouse --bsa "Fallout - Meshes.bsa" --textures-bsa "Fallout - Textures.bsa" --bench-hold`
///   2. `byro-dbg` → `entities DoorTeleport` lists plumbed doors
///   3. `door.teleport <id>` on the back door (links to another interior cell)
///   4. Expect: scene re-loads, camera lands at the destination spawn point.
struct DoorTeleportCommand;
impl ConsoleCommand for DoorTeleportCommand {
    fn name(&self) -> &str {
        "door.teleport"
    }
    fn description(&self) -> &str {
        "Inspect a door's XTEL destination (usage: door.teleport <entity_id>)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let trimmed = args.trim();
        let Ok(entity_id) = trimmed.parse::<EntityId>() else {
            return CommandOutput::line(format!(
                "door.teleport: failed to parse entity id from `{trimmed}` — \
                 usage: door.teleport <entity_id>"
            ));
        };

        // 1. Look up the DoorTeleport component on the target entity.
        let door = world
            .query::<DoorTeleport>()
            .and_then(|q| q.get(entity_id).copied());
        let Some(door) = door else {
            return CommandOutput::line(format!(
                "Entity {entity_id} has no DoorTeleport component — \
                 either it's not a door REFR, or the cell wasn't loaded \
                 from an ESM that authored XTEL on it"
            ));
        };

        let mut lines = vec![
            format!("Door {entity_id} teleport payload:"),
            format!(
                "  destination FormID: {:08X}",
                door.destination_form_id
            ),
            format!(
                "  destination position (Z-up): ({:.2}, {:.2}, {:.2})",
                door.position_zup[0], door.position_zup[1], door.position_zup[2]
            ),
            format!(
                "  destination rotation (rad):  ({:.4}, {:.4}, {:.4})",
                door.rotation_zup[0], door.rotation_zup[1], door.rotation_zup[2]
            ),
        ];

        // 2. Resolve destination FormID → parent cell. Requires
        // LoadedCellIndex to be present (set by `load_cell_with_masters`
        // for interior loads; exterior streaming wiring is Stage 3b).
        let Some(index) = world.try_resource::<LoadedCellIndex>() else {
            lines.push(
                "  (no LoadedCellIndex resource — cannot resolve parent cell. \
                 Interior cell load needed; exterior wiring is Stage 3b.)"
                    .to_string(),
            );
            return CommandOutput::lines(lines);
        };
        // Materialise an owned variant so we can drop the index borrow
        // before grabbing the LoadedPluginSet read below.
        let owned_cell = match index.0.cell_for_refr_form_id(door.destination_form_id) {
            Some(c) => c.to_owned(),
            None => {
                lines.push(format!(
                    "  destination cell: NOT FOUND — destination FormID {:08X} \
                     not in any cell loaded from the current plugin set. \
                     Likely an unloaded DLC master (e.g. --master DeadMoney.esm) \
                     or an XTEL pointing at a malformed REFR.",
                    door.destination_form_id
                ));
                return CommandOutput::lines(lines);
            }
        };
        drop(index);

        // 3. Snapshot the CLI plugin config so the orchestrator can
        // call `load_cell_with_masters` for the destination.
        let Some(plugin_set) = world.try_resource::<LoadedPluginSet>() else {
            lines.push(
                "  (no LoadedPluginSet resource — engine was not booted with --esm, \
                 so no cell-load context is available. door.teleport requires an \
                 ESM-driven boot.)"
                    .to_string(),
            );
            return CommandOutput::lines(lines);
        };
        let masters = plugin_set.masters.clone();
        let esm_path = plugin_set.esm_path.clone();
        drop(plugin_set);

        // 4. Build the destination + queue the transition.
        use byroredux_plugin::esm::cell::OwnedCellRef;
        let (destination, dest_label) = match owned_cell {
            OwnedCellRef::Interior { editor_id } => {
                let label = format!("interior '{editor_id}'");
                (
                    TransitionDestination::Interior {
                        editor_id,
                        masters,
                        esm_path,
                    },
                    label,
                )
            }
            OwnedCellRef::Exterior { worldspace, grid } => {
                let label = format!("exterior '{worldspace}' ({},{})", grid.0, grid.1);
                (
                    TransitionDestination::Exterior {
                        worldspace,
                        grid,
                        masters,
                        esm_path,
                    },
                    label,
                )
            }
        };
        lines.push(format!("  destination cell: {dest_label}"));

        let Some(mut slot) = world.try_resource_mut::<PendingCellTransitionSlot>() else {
            lines.push(
                "  (no PendingCellTransitionSlot resource — engine boot did not \
                 install the slot, the transition cannot be queued.)"
                    .to_string(),
            );
            return CommandOutput::lines(lines);
        };
        slot.0 = Some(PendingCellTransition {
            destination,
            source_refr_form_id: 0, // Source REFR form-id discovery is Stage 4 work.
            destination_position_zup: door.position_zup,
            destination_rotation_zup: door.rotation_zup,
        });
        lines.push("  queued: transition will fire on the next frame's main loop tick".into());
        CommandOutput::lines(lines)
    }
}

/// `script.activate <entity_id>` — M47.0 Phase 4 emit site for
/// [`ActivateEvent`].
///
/// Inserts an `ActivateEvent { activator: player }` marker component
/// on the named entity. Consumed by every script that handles
/// `OnActivate` — today, the [`papyrus_demo::rumble_on_activate_system`]
/// is the canonical consumer, but downstream M47.0 demos
/// (quest_advance, mg07_door, …) drain the same marker.
///
/// **Why a console command instead of an input handler**: M47.0's
/// scope is "the event-hooks runtime exists and works" — the
/// canonical Bethesda use-key + raycast wiring touches M28.5 input
/// architecture (per-frame input snapshot, camera-forward raycast,
/// activation reticle UI) which is gameplay-UX scope. The console
/// command demonstrates the e2e activation flow without the UX
/// commitment; gameplay-driven activate lands as a follow-up to
/// M28.5.
///
/// Usage: `script.activate <entity_id>` — entity_id matches what
/// `entities` / `prid` print.
///
/// The `activator` field on the inserted marker is filled with the
/// [`PlayerEntity`] resource when present (so scripts that gate on
/// "activator == player" still match), or `EntityId(0)` as a benign
/// fallback when no PlayerEntity has been inserted (test fixtures).
///
/// [`ActivateEvent`]: byroredux_scripting::ActivateEvent
/// [`papyrus_demo::rumble_on_activate_system`]: byroredux_scripting::papyrus_demo::rumble_on_activate_system
struct ScriptActivateCommand;
impl ConsoleCommand for ScriptActivateCommand {
    fn name(&self) -> &str {
        "script.activate"
    }
    fn description(&self) -> &str {
        "Emit ActivateEvent on an entity (usage: script.activate <entity_id>)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let trimmed = args.trim();
        let Ok(entity_id) = trimmed.parse::<u32>() else {
            return CommandOutput::line(format!(
                "script.activate: failed to parse entity id from `{trimmed}` — \
                 usage: script.activate <entity_id>"
            ));
        };

        // Resolve `activator` — prefer the canonical PlayerEntity
        // resource when set, fall back to EntityId(0) for fixtures
        // / pre-scene-setup activations.
        let activator: byroredux_core::ecs::EntityId = world
            .try_resource::<byroredux_scripting::papyrus_demo::PlayerEntity>()
            .map(|p| p.0)
            .unwrap_or(0);

        // Insert the marker. `query_mut` returns None if the storage
        // was never registered — that would mean `scripting::register`
        // didn't run, a programming error rather than a missing
        // entity. The early-return is the same shape as `door.teleport`.
        let Some(mut q) = world.query_mut::<byroredux_scripting::ActivateEvent>() else {
            return CommandOutput::line(
                "script.activate: ActivateEvent storage not registered — \
                 scripting::register must run at engine init.",
            );
        };
        q.insert(
            entity_id,
            byroredux_scripting::ActivateEvent { activator },
        );

        CommandOutput::line(format!(
            "script.activate: ActivateEvent emitted on entity {entity_id} (activator = {activator})"
        ))
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
    registry.register(NearCommand);
    registry.register(PickCommand);
    registry.register(CamPosCommand);
    registry.register(CamTpCommand);
    registry.register(DoorTeleportCommand);
    registry.register(SysAccessesCommand);
    registry.register(SkinListCommand);
    registry.register(SkinDumpCommand);
    registry.register(MemFragCommand);
    registry.register(LightDumpCommand);
    registry.register(LightAttenCommand);
    registry.register(ScriptActivateCommand);
    registry
}


#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
