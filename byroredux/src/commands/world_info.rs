//! Engine / world / memory introspection commands.
//!
//! `help`, `stats`, `entities`, `systems`, `sys.accesses`, `mem.frag`, `ctx.scratch`.

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
