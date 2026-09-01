//! Engine / world / memory introspection commands.
//!
//! `help`, `stats`, `entities`, `systems`, `sys.accesses`, `mem.frag`,
//! `ctx.scratch`, `lod.coverage`, `terrain.seams`, `cell.owners`.

use super::shared::*;

pub(crate) struct HelpCommand;
impl ConsoleCommand for HelpCommand {
    fn name(&self) -> &str {
        "help"
    }
    fn description(&self) -> &str {
        "List all available commands"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        // CONC-D3-04 / #1786 — re-enters the same `CommandRegistry` lock
        // the dispatcher already holds read-only for the duration of this
        // call. Read-read reentry is permitted by the always-on lock
        // tracker; this must stay a read (`resource`, never `resource_mut`)
        // per the contract on `ConsoleCommand::execute`.
        let registry = world.resource::<CommandRegistry>();
        let mut lines = vec!["Available commands:".to_string()];
        for (name, desc) in registry.list() {
            lines.push(format!("  {:16} {}", name, desc));
        }
        CommandOutput::lines(lines)
    }
}

pub(crate) struct StatsCommand;
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

pub(crate) struct EntitiesCommand;
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

        // Entities with CollisionShape but no MeshHandle are pure physics
        // proxies (bhk-authored or synthesized ghost entities). These carry
        // no GPU footprint — no BLAS, no TLAS instance, no render cost.
        let physics_only_count = match world.query::<CollisionShape>() {
            Some(cq) => {
                let mesh_q = world.query::<MeshHandle>();
                cq.iter()
                    .filter(|(e, _)| mesh_q.as_ref().is_none_or(|mq| !mq.contains(*e)))
                    .count()
            }
            None => 0,
        };

        let mut lines = vec![format!("Total entities spawned: {}", total)];
        lines.push(format!(
            "  Transform:           {}",
            world.count::<Transform>()
        ));
        lines.push(format!("  MeshHandle (render): {}", mesh_count));
        lines.push(format!(
            "  TextureHandle:       {}",
            world.count::<TextureHandle>()
        ));
        lines.push(format!(
            "  Camera:              {}",
            world.count::<Camera>()
        ));
        lines.push(format!("  CollisionShape:      {}", collision_count));
        lines.push(format!(
            "    physics-only (no MeshHandle): {}",
            physics_only_count
        ));
        CommandOutput::lines(lines)
    }
}

pub(crate) struct SystemsCommand;
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

/// `sdk.compat` — aggregate extender-era calls found in compiled scripts
/// actually observed by the current engine world.
pub(crate) struct SdkCompatCommand;
impl ConsoleCommand for SdkCompatCommand {
    fn name(&self) -> &str {
        "sdk.compat"
    }

    fn description(&self) -> &str {
        "Show engine-level compatibility for observed extender-era PEX calls"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        const MAX_ROWS: usize = 256;
        let Some(registry) = world.try_resource::<byroredux_scripting::CompatibilityRegistry>()
        else {
            return CommandOutput::line("CompatibilityRegistry resource not present");
        };
        let summary = registry.summary();
        let mut lines = vec![format!(
            "SDK compatibility: {} unique script(s), {} call(s), {} malformed call(s), truncated={}",
            registry.script_count(),
            registry.finding_count(),
            registry.malformed_count(),
            registry.truncated(),
        )];
        if summary.is_empty() {
            lines.push("  no extender-era calls observed in loaded compiled scripts".to_string());
            return CommandOutput::lines(lines);
        }
        for entry in summary.iter().take(MAX_ROWS) {
            lines.push(format_sdk_compat_entry(entry));
        }
        if summary.len() > MAX_ROWS {
            lines.push(format!(
                "  ... {} additional aggregate(s) omitted",
                summary.len() - MAX_ROWS
            ));
        }
        CommandOutput::lines(lines)
    }
}

pub(crate) fn format_sdk_compat_entry(
    entry: &byroredux_scripting::CompatibilitySummaryEntry,
) -> String {
    let disposition = match entry.compatibility.disposition {
        byroredux_scripting::CompatibilityDisposition::Native => "native",
        byroredux_scripting::CompatibilityDisposition::Mapped => "mapped",
        byroredux_scripting::CompatibilityDisposition::Unsupported => "unsupported",
    };
    let service = entry.compatibility.service.unwrap_or("none");
    let mut line = format!(
        "  {disposition:<11} {}.{}: {} occurrence(s) in {} script(s), service={service}",
        entry.provider, entry.function, entry.occurrences, entry.scripts,
    );
    if let Some(alias) = byroredux_scripting::source_alias(&entry.provider, &entry.function) {
        line.push_str(&format!(
            ", alias={}<{}> ({})",
            alias.operation, alias.value_kind, alias.constraint
        ));
    }
    line
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
pub(crate) struct CtxScratchCommand;
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
/// `ctx.upscaler` — print the active render-to-output reconstruction path.
///
/// Names the selected mode, the render/output extents, and, when FSR is
/// dispatching, the provider version plus the GPU memory the SDK reserved for
/// its own resources. That reservation is made by the official Vulkan backend,
/// not by `gpu-allocator`, so `ctx.memory` and `mem.frag` cannot see it — this
/// is the only place the total is observable. A latched dispatch failure
/// (which silently degrades the frame graph to the native blit) reports here
/// too.
pub(crate) struct CtxUpscalerCommand;
impl ConsoleCommand for CtxUpscalerCommand {
    fn name(&self) -> &str {
        "ctx.upscaler"
    }
    fn description(&self) -> &str {
        "Show the active upscaler, its extents, SDK version and working memory"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(telemetry) = world.try_resource::<byroredux_core::ecs::UpscalerTelemetry>() else {
            return CommandOutput::line("UpscalerTelemetry resource not present");
        };
        if telemetry.summary.is_empty() {
            return CommandOutput::line("Upscaler not initialized yet (no frame drawn)");
        }
        CommandOutput::lines(vec![
            format!("  {}", telemetry.summary),
            // #2821 — `n/a` when the upscale bracket did not run on the
            // snapshot frame (no GPU timers at all, or the reconstruction
            // path never dispatched); a printed number means it ran.
            format!(
                "  gpu_upscale_ms = {}",
                format_gpu_bracket_ms(telemetry.gpu_ms, telemetry.gpu_ms_active, 3),
            ),
        ])
    }
}

/// `r.upscaler <taa|fsr3> [quality]` — switch the reconstruction path live.
///
/// Staged rather than applied: switching rebuilds every render-resolution
/// target and needs `&mut VulkanContext` plus the window size, neither of
/// which a console command can reach. The main loop drains the request at the
/// next frame boundary. With no argument, reports what is active.
pub(crate) struct UpscalerSwitchCommand;
impl ConsoleCommand for UpscalerSwitchCommand {
    fn name(&self) -> &str {
        "r.upscaler"
    }
    fn description(&self) -> &str {
        "Switch upscaler live: r.upscaler taa | fsr3 [native-aa|quality|balanced|performance]"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let spec = args.trim();
        if spec.is_empty() {
            let active = world
                .try_resource::<byroredux_core::ecs::UpscalerTelemetry>()
                .filter(|telemetry| !telemetry.summary.is_empty())
                .map(|telemetry| telemetry.summary.clone())
                .unwrap_or_else(|| "not initialized".to_string());
            return CommandOutput::lines(vec![
                format!("  active: {active}"),
                "  usage: r.upscaler taa | fsr3 [native-aa|quality|balanced|performance]"
                    .to_string(),
            ]);
        }
        // Validate here so a typo reports at the prompt rather than as a log
        // line one frame later, when the operator has stopped looking.
        if let Err(error) = crate::cli_args::parse_upscaler_spec(spec) {
            return CommandOutput::line(format!("rejected: {error}"));
        }
        let Some(mut slot) = world.try_resource_mut::<byroredux_core::ecs::PendingUpscalerSwitch>()
        else {
            return CommandOutput::line("PendingUpscalerSwitch resource not present");
        };
        slot.request(spec);
        CommandOutput::line(format!(
            "queued upscaler switch to '{spec}' (applies next frame)"
        ))
    }
}

/// `sys.accesses` — print the scheduler's declared-access report.
///
/// For each stage, lists every system + its declared (or undeclared)
/// access pattern, then any inter-system conflict pairs (Conflict for
/// known disagreements between two declared systems, Unknown when at
/// least one side hasn't declared). Operator tool for R7 — the static
/// view of "what will serialize when M27 turns on parallel dispatch."
pub(crate) struct SysAccessesCommand;
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
/// `r.health` — pre-tonemap image-health counters (EX-05 / #2736).
///
/// Reports non-finite pixels observed in the linear-HDR scene *before* ACES.
/// This is the only place the check is meaningful: everything after the tone
/// mapper is clamped to `[0,1]`, which is why the smoke gate's PNG mean/stddev
/// statistics cannot observe an HDR NaN at all.
///
/// The running total is the number to gate on — a NaN is usually transient,
/// present only while a bad material or degenerate light is on screen, so a
/// check that sampled only the current frame would routinely miss it.
pub(crate) struct RenderHealthCommand;
impl ConsoleCommand for RenderHealthCommand {
    fn name(&self) -> &str {
        "r.health"
    }
    fn description(&self) -> &str {
        "Pre-tonemap non-finite pixel counters (#2736)"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(health) = world.try_resource::<ImageHealth>() else {
            return CommandOutput::line("ImageHealth resource not present");
        };
        let verdict = if health.is_clean() {
            "CLEAN"
        } else {
            "NON-FINITE PIXELS DETECTED"
        };
        CommandOutput::lines(vec![
            format!("image health: {verdict}"),
            format!(
                "  last frame:    rgb={} alpha={}",
                health.last_non_finite_rgb, health.last_non_finite_alpha
            ),
            format!(
                "  since startup: rgb={} alpha={}",
                health.total_non_finite_rgb, health.total_non_finite_alpha
            ),
        ])
    }
}

/// `rt.integrity` — one-line cross-layer RT lighting precondition.
pub(crate) struct RtIntegrityCommand;
impl ConsoleCommand for RtIntegrityCommand {
    fn name(&self) -> &str {
        "rt.integrity"
    }

    fn description(&self) -> &str {
        "RT flag, TLAS membership, and cluster-overflow correctness snapshot"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(snapshot) = world.try_resource::<RtIntegrityStats>() else {
            return CommandOutput::line("RtIntegrityStats resource not present");
        };
        CommandOutput::line(snapshot.machine_line())
    }
}

/// `lod.coverage` — exterior distant-LOD residency audit (EX-10/11 /
/// #2371): resident-quad overlap, LOD-vs-full-detail overlap, streaming
/// settledness, and per-scheme churn.
pub(crate) struct LodCoverageCommand;
impl ConsoleCommand for LodCoverageCommand {
    fn name(&self) -> &str {
        "lod.coverage"
    }

    fn description(&self) -> &str {
        "Distant-LOD residency overlap/hole/thrash audit (#2371)"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(snapshot) = world.try_resource::<byroredux_core::ecs::LodCoverageStats>() else {
            return CommandOutput::line("LodCoverageStats resource not present");
        };
        CommandOutput::line(snapshot.machine_line())
    }
}

/// `terrain.seams` — adjacent-loaded-cell terrain-seam agreement audit
/// (EX-10/11 item 7 / #2371): shared-edge height/normal disagreement
/// between resident exterior tiles.
pub(crate) struct TerrainSeamsCommand;
impl ConsoleCommand for TerrainSeamsCommand {
    fn name(&self) -> &str {
        "terrain.seams"
    }

    fn description(&self) -> &str {
        "Adjacent-cell LAND shared-edge height/normal agreement audit (#2371)"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(snapshot) = world.try_resource::<byroredux_core::ecs::TerrainSeamStats>() else {
            return CommandOutput::line("TerrainSeamStats resource not present");
        };
        CommandOutput::line(snapshot.machine_line())
    }
}

/// Resolve the grid cell `cell.owners` reports on: `args` (`"<gx> <gy>"`)
/// when given, else `current` (the player's own cell). Pure so the parsing
/// itself is unit-testable without a `World`.
fn parse_cell_owners_target(
    args: &str,
    current: Option<(i32, i32)>,
) -> Result<(i32, i32), String> {
    match args.split_whitespace().collect::<Vec<_>>().as_slice() {
        [] => current.ok_or_else(|| {
            "no active exterior session — pass `<gx> <gy>` explicitly".to_string()
        }),
        [gx, gy] => {
            let parse_axis = |value: &str, axis: &str| {
                value
                    .parse::<i32>()
                    .map_err(|_| format!("{axis} must be an integer, got '{value}'"))
            };
            Ok((parse_axis(gx, "gx")?, parse_axis(gy, "gy")?))
        }
        _ => Err("expected `cell.owners` or `cell.owners <gx> <gy>`".to_string()),
    }
}

fn format_form_id(form_id: Option<u32>) -> String {
    form_id.map_or_else(|| "none".to_string(), |f| format!("0x{f:08X}"))
}

/// `cell.owners [gx gy]` — REGN/NAVM/audio/AI ownership report for one
/// resident exterior cell (EX-16 acceptance criterion 5 / #2372, split
/// into #3805).
///
/// With no arguments, reports the player's current cell
/// ([`crate::cell_loader::CurrentExteriorContext::grid`]). With
/// `<gx> <gy>`, reports that grid cell instead — useful for auditing a
/// neighboring resident cell without walking there.
///
/// Scope, stated rather than glossed over — this reports what the engine
/// actually tracks *today*:
/// - **REGN**: [`crate::components::RegionAmbientRes`] only ever holds the
///   directive resolved for the player's OWN current cell (its per-grid-
///   cell cache, #3679) — a non-current target cell reports "not resolved
///   for this cell" instead of a fabricated answer.
/// - **NAVM**: counted directly from resident
///   [`crate::components::NavmeshTile`] entities whose `grid` matches the
///   target — accurate for any resident exterior cell, not only the
///   current one. Reads 0 tiles on Oblivion, which authors no NAVM at all
///   (`docs/engine/navmesh-pathfinding.md`) — that's the correct answer,
///   not a bug in this command.
/// - **Audio**: only the single global REGN music channel exists today
///   (#3804 is the open per-region-emitter follow-up); attributed to the
///   target cell only when it's also the player's current cell,
///   best-effort.
/// - **AI**: every resident actor with an
///   [`crate::components::AmbientPackageRuntime`] whose `GlobalTransform`
///   resolves (via [`crate::streaming::world_pos_to_grid`]) to the target
///   cell.
pub(crate) struct CellOwnersCommand;
impl ConsoleCommand for CellOwnersCommand {
    fn name(&self) -> &str {
        "cell.owners"
    }

    fn description(&self) -> &str {
        "REGN/NAVM/audio/AI ownership report for one resident exterior cell: cell.owners [gx gy] (#2372/#3805)"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let current_grid = world
            .try_resource::<crate::cell_loader::CurrentExteriorContext>()
            .map(|ctx| ctx.grid);
        let target = match parse_cell_owners_target(args, current_grid) {
            Ok(target) => target,
            Err(e) => return CommandOutput::error(e),
        };

        let mut lines = vec![format!("cell.owners ({}, {})", target.0, target.1)];

        // REGN — only ever resolved for the player's own current cell.
        if Some(target) == current_grid {
            match world.try_resource::<crate::components::RegionAmbientRes>() {
                Some(ambient) => lines.push(format!(
                    "  REGN: music={} incidental={}",
                    format_form_id(ambient.music_form),
                    format_form_id(ambient.incidental_form),
                )),
                None => lines.push("  REGN: RegionAmbientRes resource not present".to_string()),
            }
        } else {
            lines.push(
                "  REGN: not resolved for this cell (only the player's current cell is cached)"
                    .to_string(),
            );
        }

        // NAVM — counted directly, accurate for any resident exterior cell.
        let (mut navm_tiles, mut navm_vertices, mut navm_triangles) = (0u32, 0usize, 0usize);
        if let Some(q) = world.query::<crate::components::NavmeshTile>() {
            for (_, tile) in q.iter() {
                if tile.0.grid == Some(target) {
                    navm_tiles += 1;
                    navm_vertices += tile.0.vertices.len();
                    navm_triangles += tile.0.triangles.len();
                }
            }
        }
        lines.push(format!(
            "  NAVM: {navm_tiles} tile(s), {navm_vertices} vertices, {navm_triangles} triangles"
        ));

        // Audio — one global channel today; attributed only to the current cell.
        let music_active = world
            .try_resource::<byroredux_audio::AudioWorld>()
            .map(|audio| audio.is_music_active())
            .unwrap_or(false);
        lines.push(match (music_active, Some(target) == current_grid) {
            (true, true) => "  Audio: 1 active channel (global REGN music, this cell)".to_string(),
            (true, false) => {
                "  Audio: 1 active channel (global REGN music, elsewhere)".to_string()
            }
            (false, _) => "  Audio: no active channel".to_string(),
        });

        // AI — actors whose live position resolves to the target grid cell.
        let mut owners: Vec<(u32, Option<u32>)> = Vec::new();
        if let (Some(q_pkg), Some(q_gt)) = (
            world.query::<crate::components::AmbientPackageRuntime>(),
            world.query::<GlobalTransform>(),
        ) {
            for (entity, runtime) in q_pkg.iter() {
                let Some(transform) = q_gt.get(entity) else {
                    continue;
                };
                let grid = crate::streaming::world_pos_to_grid(
                    transform.translation.x,
                    transform.translation.z,
                );
                if grid == target {
                    owners.push((runtime.actor_form_id, runtime.active_package_form_id));
                }
            }
        }
        lines.push(format!("  AI: {} package owner(s)", owners.len()));
        for (actor_form, package_form) in owners {
            lines.push(format!(
                "    actor=0x{actor_form:08X} package={}",
                format_form_id(package_form)
            ));
        }

        CommandOutput::lines(lines)
    }
}

/// `render.debug <mode> [x y]` — select a named correctness view and
/// optionally queue one bounded selected-light visibility-ray capture.
pub(crate) struct RenderDebugCommand;
impl ConsoleCommand for RenderDebugCommand {
    fn name(&self) -> &str {
        "render.debug"
    }

    fn description(&self) -> &str {
        "Named render view / selected-ray probe: render.debug <mode> [x y]"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(mut control) = world.try_resource_mut::<crate::components::RenderDebugControl>()
        else {
            return CommandOutput::line("RenderDebugControl resource not present");
        };
        let words = args.split_whitespace().collect::<Vec<_>>();
        if words.is_empty() {
            let mut lines = vec![
                format!("render debug mode: {}", control.active_mode),
                format!(
                    "  pending mode: {}",
                    control.pending_mode.map_or("none", |mode| mode.as_str())
                ),
                format!(
                    "  pending probe: {}",
                    control.pending_probe_pixel.map_or_else(
                        || "none".to_string(),
                        |pixel| format!("({}, {})", pixel[0], pixel[1])
                    )
                ),
                format!(
                    "  usage: render.debug <{}> [x y]",
                    byroredux_renderer::RenderDebugMode::user_mode_names()
                ),
                "         render.debug probe <x> <y>".to_string(),
            ];
            if let Some(error) = &control.last_error {
                lines.push(format!("  last error: {error}"));
            }
            if let Some(probe) = control.last_probe {
                lines.extend(format_selected_ray_probe(probe));
            }
            return CommandOutput::lines(lines);
        }

        let (mode, pixel_words) = if words[0].eq_ignore_ascii_case("probe") {
            (None, &words[1..])
        } else {
            let mode = match words[0].parse::<byroredux_renderer::RenderDebugMode>() {
                Ok(mode) => mode,
                Err(error) => {
                    static LOG_UNKNOWN_MODE_ONCE: std::sync::Once = std::sync::Once::new();
                    LOG_UNKNOWN_MODE_ONCE.call_once(|| log::warn!("{error}"));
                    return CommandOutput::line(format!("rejected: {error}"));
                }
            };
            (Some(mode), &words[1..])
        };

        let pixel = match pixel_words {
            [] => None,
            [x, y] => {
                let parse = |value: &str, axis: &str| {
                    value
                        .parse::<u32>()
                        .map_err(|_| format!("{axis} pixel must be a non-negative integer"))
                };
                let x = match parse(x, "x") {
                    Ok(value) => value,
                    Err(error) => return CommandOutput::line(format!("rejected: {error}")),
                };
                let y = match parse(y, "y") {
                    Ok(value) => value,
                    Err(error) => return CommandOutput::line(format!("rejected: {error}")),
                };
                Some([x, y])
            }
            _ => return CommandOutput::line(
                "rejected: expected `render.debug <mode> [x y]` or `render.debug probe <x> <y>`",
            ),
        };
        if mode.is_none() && pixel.is_none() {
            return CommandOutput::line("rejected: probe requires x and y pixels");
        }

        if let Some(mode) = mode {
            control.pending_mode = Some(mode);
        }
        if let Some(pixel) = pixel {
            control.pending_probe_pixel = Some(pixel);
            control.pending_probe_generation = None;
        }
        control.last_error = None;

        let mut queued = Vec::new();
        if let Some(mode) = mode {
            queued.push(format!("mode={mode}"));
        }
        if let Some(pixel) = pixel {
            queued.push(format!("probe=({}, {})", pixel[0], pixel[1]));
        }
        CommandOutput::line(format!("queued {} (applies next frame)", queued.join(" ")))
    }
}

fn format_selected_ray_probe(probe: byroredux_renderer::SelectedRayProbeResult) -> Vec<String> {
    let mut lines = vec![format!(
        "  probe generation={} pixel=({}, {}) fragment={} ray={}",
        probe.generation, probe.pixel[0], probe.pixel[1], probe.fragment_captured, probe.ray_valid,
    )];
    if !probe.fragment_captured {
        lines.push("    no eligible main-pass fragment reached the probe site".to_string());
        return lines;
    }
    lines.push(format!(
        "    selected_light={} visibility_mask=0x{:02x}",
        probe
            .selected_light_index
            .map_or_else(|| "none".to_string(), |index| index.to_string()),
        probe.visibility_mask,
    ));
    lines.push(format!(
        "    ray origin=({:.6},{:.6},{:.6}) tMin={:.6} direction=({:.7},{:.7},{:.7}) tMax={:.6}",
        probe.ray_origin[0],
        probe.ray_origin[1],
        probe.ray_origin[2],
        probe.ray_t_min,
        probe.ray_direction[0],
        probe.ray_direction[1],
        probe.ray_direction[2],
        probe.ray_t_max,
    ));
    lines.push(format!(
        "    committed_hit={} distance={} visibility=({:.6},{:.6},{:.6})",
        probe
            .committed_hit_instance
            .map_or_else(|| "none".to_string(), |index| index.to_string()),
        probe
            .committed_hit_distance
            .map_or_else(|| "none".to_string(), |distance| format!("{distance:.6}")),
        probe.averaged_visibility[0],
        probe.averaged_visibility[1],
        probe.averaged_visibility[2],
    ));
    if let Some(index) = probe.selected_light_index {
        lines.push(format!(
            "    GpuLight[{index}] (exact uploaded light.dump record):"
        ));
        lines.push(format!(
            "      position_radius={:?}",
            probe.light_position_radius
        ));
        lines.push(format!("      color_type={:?}", probe.light_color_type));
        lines.push(format!(
            "      direction_angle={:?}",
            probe.light_direction_angle
        ));
        lines.push(format!("      params={:?}", probe.light_params));
    }
    lines
}

/// `world.owners` — cross-subsystem ownership accounting for the EX-08
/// exterior soak (#2374).
///
/// Subcommands drive one soak run:
///
/// - *(no args)* — print the current snapshot, one line per owner class.
/// - `baseline` — record the pre-entry state. Take this after the first cell
///   has loaded *and* unloaded, so one-time bootstrap allocations (worldspace
///   weather textures, the fallback checkerboard, the reverb send track) sit
///   inside the baseline instead of being reported as leaks.
/// - `cycle` — record one completed out-and-back traversal.
/// - `report` — print baseline / final / high-water per class plus the verdict.
/// - `reset` — discard baseline and history.
///
/// The verdict rules live in `byroredux_core`'s `OwnershipTracker` so they are
/// unit-tested without a device; this command is only the operator surface.
pub(crate) struct WorldOwnersCommand;
impl ConsoleCommand for WorldOwnersCommand {
    fn name(&self) -> &str {
        "world.owners"
    }
    fn description(&self) -> &str {
        "Ownership soak accounting: [baseline|cycle|report|reset] (#2374)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let snapshot = crate::ownership_sample::fresh_snapshot(world);
        match args.trim() {
            "baseline" => {
                let mut tracker = world.resource_mut::<OwnershipTracker>();
                *tracker = OwnershipTracker::new();
                tracker.set_baseline(snapshot);
                CommandOutput::line("ownership: baseline recorded")
            }
            "cycle" => {
                let mut tracker = world.resource_mut::<OwnershipTracker>();
                if tracker.baseline().is_none() {
                    return CommandOutput::line(
                        "ownership: no baseline — run `world.owners baseline` first",
                    );
                }
                tracker.record_cycle(snapshot);
                let n = tracker.cycles().len();
                CommandOutput::line(format!("ownership: cycle {} recorded", n))
            }
            "report" => {
                let tracker = world.resource::<OwnershipTracker>();
                CommandOutput::lines(tracker.report())
            }
            "reset" => {
                let mut tracker = world.resource_mut::<OwnershipTracker>();
                *tracker = OwnershipTracker::new();
                CommandOutput::line("ownership: tracker reset")
            }
            "" => {
                let mut lines = vec![format!(
                    "{:<26} {:>10}  {}",
                    "class", "value", "reclaim-policy"
                )];
                for class in snapshot.classes() {
                    let policy = match class.policy {
                        ReclaimPolicy::Exact => "exact",
                        ReclaimPolicy::Bounded => "bounded",
                        ReclaimPolicy::Monotonic => "monotonic",
                    };
                    lines.push(format!(
                        "{:<26} {:>10}  {}",
                        class.name, class.value, policy
                    ));
                }
                CommandOutput::lines(lines)
            }
            other => CommandOutput::line(format!(
                "unknown subcommand `{other}` — use baseline|cycle|report|reset or no argument"
            )),
        }
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
pub(crate) struct MemFragCommand;
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

#[cfg(test)]
mod cell_owners_tests {
    use super::*;
    use crate::cell_loader::CurrentExteriorContext;
    use crate::components::{spawn_navmesh_tiles, AmbientPackageRuntime, RegionAmbientRes};
    use byroredux_plugin::esm::records::NavmRecord;

    // ── parse_cell_owners_target (pure) ──────────────────────────────

    #[test]
    fn no_args_falls_back_to_current_grid() {
        assert_eq!(parse_cell_owners_target("", Some((3, -2))), Ok((3, -2)));
    }

    #[test]
    fn no_args_and_no_current_grid_is_rejected() {
        assert!(parse_cell_owners_target("", None).is_err());
    }

    #[test]
    fn explicit_args_override_current_grid() {
        assert_eq!(
            parse_cell_owners_target("5 -7", Some((0, 0))),
            Ok((5, -7))
        );
    }

    #[test]
    fn malformed_or_wrong_arity_args_are_rejected() {
        assert!(parse_cell_owners_target("notanumber 1", Some((0, 0))).is_err());
        assert!(parse_cell_owners_target("1", Some((0, 0))).is_err());
        assert!(parse_cell_owners_target("1 2 3", Some((0, 0))).is_err());
    }

    // ── CellOwnersCommand (integration) ──────────────────────────────

    fn navm(grid: (i32, i32), vertices: usize, triangles: usize) -> NavmRecord {
        NavmRecord {
            grid: Some(grid),
            vertices: vec![[0.0, 0.0, 0.0]; vertices],
            triangles: vec![
                byroredux_plugin::esm::records::NavmTriangle {
                    vertices: [0, 0, 0],
                    edge_neighbours: [None, None, None],
                    flags: 0,
                };
                triangles
            ],
            ..Default::default()
        }
    }

    fn spawn_actor(world: &mut World, grid: (i32, i32), actor_form: u32, package: Option<u32>) {
        let entity = world.spawn();
        // #3679 world_pos_to_grid: gx = floor(x/4096), gy = floor(-z/4096) —
        // cell-center offsets so a fractional 4096 boundary never rounds
        // into the neighbor.
        let x = grid.0 as f32 * 4096.0 + 100.0;
        let z = -(grid.1 as f32 * 4096.0 + 100.0);
        world.insert(
            entity,
            GlobalTransform::new(Vec3::new(x, 0.0, z), Quat::IDENTITY, 1.0),
        );
        world.insert(
            entity,
            AmbientPackageRuntime {
                package_candidates: Vec::new(),
                active_package_form_id: package,
                actor_form_id: actor_form,
                last_evaluated_game_minute: None,
            },
        );
    }

    /// The common case: no args, player's own cell, with a resolved REGN
    /// directive, one resident NAVM tile, and one AI package owner.
    #[test]
    fn reports_current_cell_by_default() {
        let mut world = World::new();
        world.insert_resource(CurrentExteriorContext {
            grid: (0, 0),
            ..Default::default()
        });
        world.insert_resource(RegionAmbientRes {
            music_form: Some(0x10),
            incidental_form: None,
        });
        spawn_navmesh_tiles(&mut world, &[navm((0, 0), 3, 1), navm((1, 1), 3, 1)]);
        spawn_actor(&mut world, (0, 0), 0x1001, Some(0x2001));
        spawn_actor(&mut world, (1, 1), 0x1002, None);

        let out = CellOwnersCommand.execute(&world, "");
        let joined = out.lines.join("\n");
        assert!(joined.contains("cell.owners (0, 0)"), "{joined}");
        assert!(joined.contains("music=0x00000010"), "{joined}");
        assert!(joined.contains("NAVM: 1 tile(s), 3 vertices, 1 triangles"), "{joined}");
        assert!(joined.contains("AI: 1 package owner(s)"), "{joined}");
        assert!(joined.contains("actor=0x00001001 package=0x00002001"), "{joined}");
    }

    /// An explicit non-current target: NAVM and AI still resolve correctly
    /// (their data isn't scoped to the player's cell), but REGN correctly
    /// declines to fabricate an answer it doesn't have.
    #[test]
    fn reports_explicit_non_current_cell_without_fabricating_regn() {
        let mut world = World::new();
        world.insert_resource(CurrentExteriorContext {
            grid: (0, 0),
            ..Default::default()
        });
        world.insert_resource(RegionAmbientRes {
            music_form: Some(0x10),
            incidental_form: None,
        });
        spawn_navmesh_tiles(&mut world, &[navm((0, 0), 3, 1), navm((1, 1), 6, 2)]);
        spawn_actor(&mut world, (1, 1), 0x1002, Some(0x2002));

        let out = CellOwnersCommand.execute(&world, "1 1");
        let joined = out.lines.join("\n");
        assert!(joined.contains("cell.owners (1, 1)"), "{joined}");
        assert!(joined.contains("not resolved for this cell"), "{joined}");
        assert!(joined.contains("NAVM: 1 tile(s), 6 vertices, 2 triangles"), "{joined}");
        assert!(joined.contains("actor=0x00001002 package=0x00002002"), "{joined}");
    }

    /// Oblivion authors zero NAVM records at all
    /// (`docs/engine/navmesh-pathfinding.md`) — a cell with no resident
    /// tiles must report a clean 0, not error.
    #[test]
    fn zero_navm_tiles_reports_cleanly() {
        let mut world = World::new();
        world.insert_resource(CurrentExteriorContext {
            grid: (0, 0),
            ..Default::default()
        });
        let out = CellOwnersCommand.execute(&world, "");
        let joined = out.lines.join("\n");
        assert!(joined.contains("NAVM: 0 tile(s), 0 vertices, 0 triangles"), "{joined}");
        assert!(joined.contains("AI: 0 package owner(s)"), "{joined}");
    }
}
