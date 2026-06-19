//! Scene / lighting / material / script-state commands.
//!
//! `light.dump`, `light.atten`, `door.teleport`, `script.activate`, `mat.list`, `mat.set`, `ragdoll`.

use super::shared::*;

/// Dump the active scene lighting resources — cell ambient / directional,
/// sky / sun, current game time. Companion to `tex.missing` for diagnosing
/// "scene is too dark" symptoms without grepping logs (#890 followup —
/// see Markarth investigation 2026-05-10). The output flags the
/// resource-not-present case explicitly so it's obvious whether the
/// engine is on the procedural fallback, a resolved WTHR / CLMT, or a
/// per-cell XCLL/LGTM override.
pub(crate) struct LightDumpCommand;
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
pub(crate) struct LightAttenCommand;
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
        lines.push(format!(
            "  knee_frac = {knee:.3}  (authored radius = knee × cull radius)"
        ));
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
pub(crate) struct DoorTeleportCommand;
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
            format!("  destination FormID: {:08X}", door.destination_form_id),
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
pub(crate) struct ScriptActivateCommand;
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
        q.insert(entity_id, byroredux_scripting::ActivateEvent { activator });

        CommandOutput::line(format!(
            "script.activate: ActivateEvent emitted on entity {entity_id} (activator = {activator})"
        ))
    }
}
/// `mat.list` — tabulate every entity carrying a [`Material`], with the
/// RT-relevant scalars at a glance. The companion to `mat.set`: it gives
/// you the entity ids to sweep. Built for the Cornell-box harness
/// (`--cornell`, see [`crate::cornell`]) but works on any loaded scene.
pub(crate) struct MatListCommand;
impl ConsoleCommand for MatListCommand {
    fn name(&self) -> &str {
        "mat.list"
    }
    fn description(&self) -> &str {
        "List entities with a Material + their PBR scalars (companion to mat.set)"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(q) = world.query::<Material>() else {
            return CommandOutput::line("No Material components in the world.");
        };
        let mut rows: Vec<(EntityId, &Material)> = q.iter().collect();
        rows.sort_by_key(|(e, _)| *e);
        if rows.is_empty() {
            return CommandOutput::line("No Material components in the world.");
        }
        let mut lines = vec![format!(
            "{:>5}  {:<20} {:>5} {:>5} {:>5} {:>5} {:>4}  {:<14}",
            "id", "name", "metal", "rough", "alpha", "emul", "kind", "diffuse(rgb)"
        )];
        for (e, m) in rows {
            let name = resolve_entity_name(world, e).unwrap_or_else(|| "-".to_string());
            let name: String = name.chars().take(20).collect();
            lines.push(format!(
                "{:>5}  {:<20} {:>5.2} {:>5.2} {:>5.2} {:>5.2} {:>4}  {:.2},{:.2},{:.2}",
                e,
                name,
                m.metalness,
                m.roughness,
                m.alpha,
                m.emissive_mult,
                m.material_kind,
                m.diffuse_color[0],
                m.diffuse_color[1],
                m.diffuse_color[2],
            ));
        }
        CommandOutput::lines(lines)
    }
}
/// `mat.set <entity_id> <field> <value...>` — live-mutate one field of an
/// entity's [`Material`]. Because [`crate::render::static_meshes`] reads
/// `Material` fresh every frame, the change shows up on the next rendered
/// frame — the core of the Cornell-box material-sweep workflow.
///
/// Scalar fields take one value; `*_color` fields take three (r g b).
/// `material_kind` takes an integer (see `MATERIAL_KIND_*` in the
/// renderer). `color` is an alias for `diffuse_color`.
pub(crate) struct MatSetCommand;
impl MatSetCommand {
    /// Parse `n` whitespace-separated floats from `parts`, erroring if the
    /// count or any parse is wrong.
    fn floats(parts: &[&str], n: usize) -> Result<Vec<f32>, String> {
        if parts.len() != n {
            return Err(format!("expected {n} value(s), got {}", parts.len()));
        }
        parts
            .iter()
            .map(|s| {
                s.parse::<f32>()
                    .map_err(|_| format!("`{s}` is not a number"))
            })
            .collect()
    }
}
impl ConsoleCommand for MatSetCommand {
    fn name(&self) -> &str {
        "mat.set"
    }
    fn description(&self) -> &str {
        "Live-edit a Material field: mat.set <entity_id> <field> <value...>"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        const USAGE: &str = "usage: mat.set <entity_id> <field> <value...>\n  \
            fields: metalness|roughness|alpha|glossiness|emissive_mult|specular_strength|\
            env_map_scale (1 value), color|diffuse_color|emissive_color|specular_color \
            (3 values), material_kind (1 int)";
        let mut parts = args.split_whitespace();
        let Some(id_str) = parts.next() else {
            return CommandOutput::line(USAGE);
        };
        let Some(field) = parts.next() else {
            return CommandOutput::line(USAGE);
        };
        let Ok(id) = id_str.parse::<u32>() else {
            return CommandOutput::line(format!("mat.set: bad entity id `{id_str}`\n{USAGE}"));
        };
        let vals: Vec<&str> = parts.collect();

        let Some(mut q) = world.query_mut::<Material>() else {
            return CommandOutput::line("mat.set: no Material storage in the world.");
        };
        let Some(m) = q.get_mut(id) else {
            return CommandOutput::line(format!("mat.set: entity {id} has no Material."));
        };

        // Each arm validates value arity via `floats`, mutates, and reports
        // the new value. Aliases: `color` -> diffuse_color, `*` short forms.
        let set_scalar = |slot: &mut f32, vals: &[&str]| -> Result<String, String> {
            let v = MatSetCommand::floats(vals, 1)?;
            *slot = v[0];
            Ok(format!("{:.4}", v[0]))
        };
        let set_vec3 = |slot: &mut [f32; 3], vals: &[&str]| -> Result<String, String> {
            let v = MatSetCommand::floats(vals, 3)?;
            *slot = [v[0], v[1], v[2]];
            Ok(format!("{:.3},{:.3},{:.3}", v[0], v[1], v[2]))
        };

        let result = match field.to_ascii_lowercase().as_str() {
            "metalness" | "metal" => set_scalar(&mut m.metalness, &vals),
            "roughness" | "rough" => set_scalar(&mut m.roughness, &vals),
            "alpha" => set_scalar(&mut m.alpha, &vals),
            "glossiness" | "gloss" => set_scalar(&mut m.glossiness, &vals),
            "emissive_mult" | "emult" => set_scalar(&mut m.emissive_mult, &vals),
            "specular_strength" | "spec" => set_scalar(&mut m.specular_strength, &vals),
            "env_map_scale" | "env" => set_scalar(&mut m.env_map_scale, &vals),
            "color" | "diffuse_color" | "diffuse" => set_vec3(&mut m.diffuse_color, &vals),
            "emissive_color" => set_vec3(&mut m.emissive_color, &vals),
            "specular_color" => set_vec3(&mut m.specular_color, &vals),
            "material_kind" | "kind" => {
                if vals.len() != 1 {
                    Err(format!("expected 1 value, got {}", vals.len()))
                } else {
                    match vals[0].parse::<u32>() {
                        Ok(k) => {
                            m.material_kind = k;
                            Ok(k.to_string())
                        }
                        Err(_) => Err(format!("`{}` is not an integer", vals[0])),
                    }
                }
            }
            other => Err(format!("unknown field `{other}`")),
        };

        match result {
            Ok(shown) => CommandOutput::line(format!("mat.set: entity {id} {field} = {shown}")),
            Err(e) => CommandOutput::line(format!("mat.set: {e}\n{USAGE}")),
        }
    }
}
/// `ragdoll <entity_id>` — flip an actor from bind-pose to a live Havok
/// ragdoll simulated on our Rapier solver (M41.x). The entity is the
/// actor placement root carrying a `RagdollTemplate` (attached at NPC
/// spawn from the skeleton's parsed Havok articulation). Seeds each body
/// from the bone's current pose, builds the multibody, and tags the actor
/// `RagdollActive`; the writeback system then crumples the skinned mesh.
pub(crate) struct RagdollCommand;
impl ConsoleCommand for RagdollCommand {
    fn name(&self) -> &str {
        "ragdoll"
    }
    fn description(&self) -> &str {
        "Ragdoll an actor (usage: ragdoll <entity_id>)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let trimmed = args.trim();
        let Ok(actor) = trimmed.parse::<EntityId>() else {
            return CommandOutput::line(format!(
                "ragdoll: failed to parse entity id from `{trimmed}` — usage: ragdoll <entity_id>"
            ));
        };
        match crate::ragdoll::activate_ragdoll(world, actor) {
            Ok(n) => CommandOutput::line(format!(
                "ragdoll: entity {actor} now simulating {n} bodies on Rapier"
            )),
            Err(e) => CommandOutput::line(format!("ragdoll: {e}")),
        }
    }
}
