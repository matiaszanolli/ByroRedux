//! Texture / mesh / skin diagnostic commands.
//!
//! `tex.missing`, `tex.loaded`, `mesh.info`, `mesh.cache`, `skin.coverage`, `skin.list`, `skin.dump`.

use super::shared::*;


pub(crate) struct TexMissingCommand;
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
        sorted.sort_by_key(|e| std::cmp::Reverse(e.1 .0));

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

pub(crate) struct TexLoadedCommand;
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
        sorted.sort_by_key(|e| std::cmp::Reverse(e.1));
        for (path, count) in sorted.iter().take(30) {
            lines.push(format!("  {:4}x  {}", count, path));
        }
        CommandOutput::lines(lines)
    }
}

pub(crate) struct MeshInfoCommand;
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
                        lines.push("  REFR FormID:       <unresolved in FormIdPool>".to_string());
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
                mat.emissive_color[0],
                mat.emissive_color[1],
                mat.emissive_color[2],
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
            markers.push(format!(
                "AlphaBlend(src={}, dst={})",
                ab.src_blend, ab.dst_blend
            ));
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
pub(crate) struct MeshCacheCommand;
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
pub(crate) struct SkinCoverageCommand;
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
        lines.push(format!("  gpu_taa_ms             = {:.3}", cov.gpu_taa_ms,));
        if cov.dispatches_total == 0 {
            lines.push("  coverage: n/a (no skinned entities this frame)".to_string());
        } else if cov.fully_covered() {
            lines.push("  coverage: full".to_string());
        } else {
            let missed = cov.dispatches_total.saturating_sub(cov.refits_succeeded);
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
/// `skin.list` — enumerate every entity carrying [`SkinnedMesh`].
///
/// Companion to [`SkinDumpCommand`] (#841): operators run `skin.list`
/// to find the entity_id of the actor whose body is misrendering, then
/// `skin.dump <id>` to inspect its palette.
pub(crate) struct SkinListCommand;
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
pub(crate) struct SkinDumpCommand;
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
