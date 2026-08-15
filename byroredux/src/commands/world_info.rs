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
            format!("  gpu_upscale_ms = {:.3}", telemetry.gpu_ms),
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
