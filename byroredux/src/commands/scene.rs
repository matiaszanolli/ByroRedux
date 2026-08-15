//! Scene / lighting / material / script-state commands.
//!
//! `light.dump`, `light.atten`, `door.teleport`, `script.activate`, `mat.list`, `mat.dump`, `mat.set`, `ragdoll`.

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
                lines.push(format!(
                    "    LTMP inherits       = {}",
                    lit.inheritance_flags
                        .map(|flags| format!("0x{flags:08x} (resolved)"))
                        .unwrap_or_else(|| "None".to_string())
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
                lines.push(format!("  day           = {}", gt.day));
                lines.push(format!(
                    "  hour          = {:.3} ({}:{:02} {})",
                    gt.hour, display_hour, minute_int, suffix
                ));
                lines.push(format!(
                    "  time_scale    = {:.1}\u{00d7} game-sec/real-sec ({})",
                    gt.time_scale,
                    if gt.is_paused() { "paused" } else { "running" }
                ));
            }
            None => {
                lines.push("GameTimeRes: <not present>".to_string());
            }
        }

        // ── Live physical emitters ───────────────────────────────────
        // Copy the rows out before consulting GlobalTransform/Name/Parent so
        // the command never nests component-storage locks in an order that
        // differs from the scheduler's TypeId ordering contract.
        let mut emitters: Vec<(EntityId, LightSource)> = world
            .query::<LightSource>()
            .map(|q| q.iter().map(|(entity, light)| (entity, *light)).collect())
            .unwrap_or_default();
        emitters.sort_by_key(|(entity, _)| *entity);
        lines.push(String::new());
        lines.push(format!("LightSource emitters: {}", emitters.len()));
        for (entity, light) in emitters {
            let emitter = light.emitter;
            let rgb = emitter.radiant_intensity.get();
            let position = world
                .get::<GlobalTransform>(entity)
                .map(|transform| transform.translation)
                .unwrap_or(Vec3::ZERO);
            let name = resolve_entity_name(world, entity).unwrap_or_else(|| "-".into());

            // Cell-spawned NIF/LIGH emitters are children of the placement
            // root carrying FormIdComponent. Walk that short chain so the
            // dump names authored provenance rather than merely an ECS id.
            let mut cursor = entity;
            let mut source_form = None;
            for _ in 0..16 {
                if let Some(form) = world.get::<FormIdComponent>(cursor) {
                    source_form = Some(format!("{:?}", form.0));
                    break;
                }
                let Some(parent) = world.get::<Parent>(cursor) else {
                    break;
                };
                cursor = parent.0;
            }
            let provenance = source_form
                .map(|form| format!("esm-refr {form}"))
                .unwrap_or_else(|| "nif/synthetic (no FormId ancestor)".into());
            lines.push(format!(
                "  entity={entity} name={name:?} kind={:?} source={provenance}",
                emitter.kind
            ));
            lines.push(format!(
                "    position=[{:.2},{:.2},{:.2}] direction=[{:.4},{:.4},{:.4}]",
                position.x,
                position.y,
                position.z,
                emitter.direction[0],
                emitter.direction[1],
                emitter.direction[2],
            ));
            lines.push(format!(
                "    radiant=[{:.4},{:.4},{:.4}] dimmer={:.3} intensity={:.3} range_m={:.3} cone_cos={:.4}",
                rgb[0],
                rgb[1],
                rgb[2],
                light.dimmer,
                light.intensity,
                emitter.range.get(),
                emitter.outer_cone_cos,
            ));
            lines.push(format!(
                "    attenuation={:?} falloff={:.3} visibility=0x{:02x} legacy_flags=0x{:08x} shadow_flags=0x{:08x}",
                emitter.attenuation,
                emitter.falloff_exponent,
                emitter.visibility.bits(),
                light.flags,
                light.shadow_flags,
            ));
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

        // Resolve + queue through the same canonical path normal E-key
        // interaction uses. The command remains a diagnostic frontend, not a
        // second transition implementation that can drift from gameplay.
        match crate::cell_loader::queue_door_transition(world, entity_id) {
            Ok(queued) => {
                lines.push(format!("  destination cell: {}", queued.destination_label));
                lines.push(
                    "  queued: transition will fire on the next frame's main loop tick".into(),
                );
            }
            Err(error) => lines.push(format!("  transition not queued: {error}")),
        }
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
/// This command is the diagnostic frontend for the same canonical
/// `ActivateEvent` consumed by normal E-key interaction.
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

        let activator = match crate::interaction::emit_activate_event(world, entity_id) {
            Ok(activator) => activator,
            Err(error) => {
                return CommandOutput::line(format!("script.activate: {error}"));
            }
        };

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

/// `mat.dump <entity|.>` — lossless material/texture inspection at the ECS
/// → renderer boundary. Unlike `mat.list`, this includes the packed shading
/// flags and every canonical texture role with its final path, provenance,
/// bindless handle, descriptor binding, and colour-space interpretation.
pub(crate) struct MatDumpCommand;
impl ConsoleCommand for MatDumpCommand {
    fn name(&self) -> &str {
        "mat.dump"
    }

    fn description(&self) -> &str {
        "Dump one material and every resolved texture role: mat.dump <entity|.>"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let token = args.trim();
        if token.is_empty() {
            return CommandOutput::line("usage: mat.dump <entity_id|.>");
        }
        let entity = match resolve_console_entity(world, token) {
            Ok(entity) => entity,
            Err(error) => return CommandOutput::line(format!("mat.dump: {error}")),
        };
        let Some(material) = world.get::<Material>(entity) else {
            return CommandOutput::line(format!("mat.dump: entity {entity} has no Material."));
        };

        use byroredux_renderer::vulkan::material::material_flag;
        let flags = material.effect_shader_flags;
        let lobe = if material.material_kind == byroredux_renderer::MATERIAL_KIND_GLASS {
            "glass"
        } else if flags & material_flag::TRANSLUCENCY != 0 {
            "translucency"
        } else if flags & material_flag::PBR_BSDF != 0 {
            "disney-pbr"
        } else {
            "legacy"
        };
        let name = resolve_entity_name(world, entity).unwrap_or_else(|| "-".to_string());
        let mut lines = vec![
            format!("Material {entity} ({name}):"),
            format!(
                "  kind={} flags=0x{flags:08x} lobe={lobe}",
                material.material_kind
            ),
            format!(
                "  flag gates: pbr={} translucency={} model_space_normals={} thin_glass={}",
                flags & material_flag::PBR_BSDF != 0,
                flags & material_flag::TRANSLUCENCY != 0,
                flags & material_flag::MODEL_SPACE_NORMALS != 0,
                flags & material_flag::THIN_GLASS != 0,
            ),
            format!(
                "  PBR: metalness={:.4} roughness={:.4} ior={:.4} glossiness={:.4}",
                material.metalness, material.roughness, material.ior, material.glossiness
            ),
            format!(
                "  alpha={:.4} test={} threshold={:.4} func={} env_scale={:.4}",
                material.alpha,
                material.alpha_test,
                material.alpha_threshold,
                material.alpha_test_func,
                material.env_map_scale,
            ),
            format!(
                "  material_path={}",
                material.material_path.as_deref().unwrap_or("<none>")
            ),
        ];

        let handles = world.get::<MaterialTextureHandles>(entity);
        let debug = world.get::<MaterialTextureDebugInfo>(entity);
        let (Some(handles), Some(debug)) = (handles, debug) else {
            lines.push(
                "  texture oracle: unavailable (synthetic/terrain material did not pass through the NIF material resolver)"
                    .to_string(),
            );
            return CommandOutput::lines(lines);
        };

        lines.push(format!(
            "  sampler clamp_mode={} normal_has_alpha={} pom_scale={:.4} pom_passes={:.1}",
            debug.clamp_mode,
            handles.normal_has_alpha,
            handles.parallax_height_scale,
            handles.parallax_max_passes,
        ));
        lines.push(
            "  textures (role | handle | set binding | color | view | source | path):".into(),
        );

        let p = &debug.paths;
        let s = &debug.sources;
        let h = &handles.textures;
        let slots: [(&str, Option<&str>, MaterialTextureSource, u32, &str, &str); 18] = [
            (
                "base_color",
                p.base_color.as_deref(),
                s.base_color,
                h.base_color,
                "sRGB",
                "2D",
            ),
            (
                "normal",
                p.normal.as_deref(),
                s.normal,
                h.normal,
                "linear",
                "2D",
            ),
            (
                "emissive",
                p.emissive.as_deref(),
                s.emissive,
                h.emissive,
                "sRGB",
                "2D",
            ),
            (
                "detail",
                p.detail.as_deref(),
                s.detail,
                h.detail,
                "sRGB",
                "2D",
            ),
            (
                "smooth_spec",
                p.smooth_spec.as_deref(),
                s.smooth_spec,
                h.smooth_spec,
                "linear",
                "2D",
            ),
            ("dark", p.dark.as_deref(), s.dark, h.dark, "sRGB", "2D"),
            (
                "height",
                p.height.as_deref(),
                s.height,
                h.height,
                "linear",
                "2D",
            ),
            (
                "environment",
                p.environment.as_deref(),
                s.environment,
                h.environment,
                "sRGB",
                "cube",
            ),
            (
                "environment_mask",
                p.environment_mask.as_deref(),
                s.environment_mask,
                h.environment_mask,
                "linear",
                "2D",
            ),
            ("tint", p.tint.as_deref(), s.tint, h.tint, "sRGB", "2D"),
            (
                "inner_layer",
                p.inner_layer.as_deref(),
                s.inner_layer,
                h.inner_layer,
                "sRGB",
                "2D",
            ),
            (
                "specular",
                p.specular.as_deref(),
                s.specular,
                h.specular,
                "linear",
                "2D",
            ),
            (
                "lighting",
                p.lighting.as_deref(),
                s.lighting,
                h.lighting,
                "linear",
                "2D",
            ),
            ("flow", p.flow.as_deref(), s.flow, h.flow, "linear", "2D"),
            (
                "wrinkle",
                p.wrinkle.as_deref(),
                s.wrinkle,
                h.wrinkle,
                "linear",
                "2D",
            ),
            (
                "greyscale_lut",
                p.greyscale_lut.as_deref(),
                s.greyscale_lut,
                h.greyscale_lut,
                "sRGB",
                "2D",
            ),
            (
                "reflectance",
                p.reflectance.as_deref(),
                s.reflectance,
                h.reflectance,
                "linear",
                "2D",
            ),
            (
                "emittance_gradient",
                p.emittance_gradient.as_deref(),
                s.emittance_gradient,
                h.emittance_gradient,
                "sRGB",
                "2D",
            ),
        ];
        for (role, path, source, handle, color, view) in slots {
            let binding = if view == "cube" { 1 } else { 0 };
            lines.push(format!(
                "    {role:<20} {handle:>5}  set=0 binding={binding}  {color:<6} {view:<4} {:<16} {}",
                source.label(),
                path.unwrap_or("<none>")
            ));
        }
        for i in 0..4 {
            lines.push(format!(
                "    decal[{i}]             {:>5}  set=0 binding=0  sRGB   2D   {:<16} {}",
                h.decals[i],
                s.decals[i].label(),
                p.decals[i].as_deref().unwrap_or("<none>")
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
    /// count or any parse is wrong, or if any value is non-finite.
    ///
    /// #2489 (NIFAL-D6-2026-08-07-03) — the non-finite rejection is the
    /// load-bearing half. `"NaN".parse::<f32>()` and `"inf".parse::<f32>()`
    /// both succeed, and `mat.set` is the only writer that reaches a resolved
    /// `Material` after `translate_material`, so a typo put a NaN straight
    /// into `GpuMaterial`. That is the exact failure #1535 exists to prevent
    /// on the translate side ("NaN GGX terms poison the lit color and stick
    /// in SVGF/TAA history") — and because it sticks in the temporal history,
    /// it outlives the correction and reads as a renderer bug rather than a
    /// console typo. Rejecting at the parse site covers every arm, colours
    /// included: a NaN `diffuse_color` poisons the frame just as well.
    fn floats(parts: &[&str], n: usize) -> Result<Vec<f32>, String> {
        if parts.len() != n {
            return Err(format!("expected {n} value(s), got {}", parts.len()));
        }
        parts
            .iter()
            .map(|s| {
                let v = s
                    .parse::<f32>()
                    .map_err(|_| format!("`{s}` is not a number"))?;
                if !v.is_finite() {
                    return Err(format!(
                        "`{s}` is not finite — a NaN/inf material scalar poisons the \
                         shading term and persists in SVGF/TAA history after the \
                         value is corrected (#1535 / #2489)"
                    ));
                }
                Ok(v)
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
            env_map_scale|ior|subsurface|sheen|sheen_tint|anisotropic|\
            translucency_transmissive_scale|translucency_turbulence (1 value), \
            color|diffuse_color|emissive_color|specular_color|translucency_subsurface_color (3 values), \
            material_kind|material_flags (1 int)";
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
            // #2489 (NIFAL-D6-2026-08-07-03) — `metalness` / `roughness`
            // carry an engine-wide invariant (`metalness ∈ [0,1]`,
            // `roughness ∈ [0.04,1]`, "fully resolved, no render-time
            // fallback") that the render path relies on by reading them
            // verbatim into `GpuMaterial`. `mat.set` is the only writer that
            // reaches them after `translate_material`, and it stored the
            // parsed value raw — so `mat.set <id> roughness 0`, a plausible
            // typo, landed below the 0.04 floor and diverged the console
            // writer from the translate writer. `resolve_pbr` is the same
            // clamp `translate_material` applies, is idempotent, and cannot
            // reach its NaN-classifier branch here because `floats` already
            // rejected non-finite input. Report the STORED value, not the
            // input, so the console shows the clamp when it fires.
            "metalness" | "metal" => match set_scalar(&mut m.metalness, &vals) {
                Ok(_) => {
                    m.resolve_pbr();
                    Ok(format!("{:.4}", m.metalness))
                }
                Err(e) => Err(e),
            },
            "roughness" | "rough" => match set_scalar(&mut m.roughness, &vals) {
                Ok(_) => {
                    m.resolve_pbr();
                    Ok(format!("{:.4}", m.roughness))
                }
                Err(e) => Err(e),
            },
            // `alpha` documents its own range (0.0 transparent → 1.0 opaque)
            // but has no `resolve_pbr` equivalent, so clamp it here.
            "alpha" => match set_scalar(&mut m.alpha, &vals) {
                Ok(_) => {
                    m.alpha = m.alpha.clamp(0.0, 1.0);
                    Ok(format!("{:.4}", m.alpha))
                }
                Err(e) => Err(e),
            },
            "glossiness" | "gloss" => set_scalar(&mut m.glossiness, &vals),
            "emissive_mult" | "emult" => set_scalar(&mut m.emissive_mult, &vals),
            "specular_strength" | "spec" => set_scalar(&mut m.specular_strength, &vals),
            "env_map_scale" | "env" => set_scalar(&mut m.env_map_scale, &vals),
            // #2249 (REN-D21-03) — `MATERIAL_KIND_FIRE_REFRACTION` overloads
            // `ior` as its authored distortion strength (see REN-D6-01 /
            // `triangle.frag`'s fire-refraction branch); without this arm
            // the Cornell harness had no way to reach the field live, so
            // every fire-refraction gap this session had to be found by
            // static code reading instead of the harness.
            // #2489 — no range clamp here, deliberately: `ior` is the one
            // scalar with two incompatible authored ranges (a physical
            // refractive index ~1.0–2.5 for the dielectric/glass path, and a
            // 0–1 heat-haze distortion strength under
            // `MATERIAL_KIND_FIRE_REFRACTION` — see #2232). `mat.set` cannot
            // know which the caller means, so clamping to either would break
            // the other. The finite check in `floats` is the guard that does
            // apply to both.
            "ior" | "distortion_strength" => set_scalar(&mut m.ior, &vals),
            // #2514 (REN-D21-2026-08-07-02) — the Cornell harness had no
            // way to reach `subsurface`/`sheen`/`sheen_tint`/`anisotropic`
            // (→ `GpuMaterial`'s Disney-BSDF-only params, gated by
            // `MAT_FLAG_PBR_BSDF`) live; no source format authors these
            // (unlike `subsurface_rolloff` etc. above, which have real
            // Skyrim+/FO4 source data), so this is their only producer.
            "subsurface" | "sss" => set_scalar(&mut m.subsurface, &vals),
            "sheen" => set_scalar(&mut m.sheen, &vals),
            "sheen_tint" => set_scalar(&mut m.sheen_tint, &vals),
            "anisotropic" | "aniso" => set_scalar(&mut m.anisotropic, &vals),
            // #2823 (REN-D21-01) — `MAT_FLAG_TRANSLUCENCY` (#1147 Phase 2b
            // SSS lobe) was flag-reachable via `material_flags` but
            // scalar-unreachable: no arm authored
            // `translucency_subsurface_color` / `_transmissive_scale` /
            // `_turbulence`, so they sat at `Material::default()` and the
            // shader's `* mat.translucencyTransmissiveScale` term was
            // always zero. Same gap #2514 closed for the Disney-BSDF
            // scalars above.
            "translucency_transmissive_scale" | "translucency_scale" => {
                set_scalar(&mut m.translucency_transmissive_scale, &vals)
            }
            "translucency_turbulence" | "translucency_turb" => {
                set_scalar(&mut m.translucency_turbulence, &vals)
            }
            "color" | "diffuse_color" | "diffuse" => set_vec3(&mut m.diffuse_color, &vals),
            "emissive_color" => set_vec3(&mut m.emissive_color, &vals),
            "specular_color" => set_vec3(&mut m.specular_color, &vals),
            "translucency_subsurface_color" | "translucency_color" => {
                set_vec3(&mut m.translucency_subsurface_color, &vals)
            }
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
            // #2477 (REN-D21-2026-08-07-01) — the Cornell harness had no
            // way to reach `effect_shader_flags` (→ `GpuMaterial.
            // material_flags`) live, so `MAT_FLAG_PBR_BSDF` and every
            // other `material_flag::*` bit (translucency, model-space
            // normals, …) could only be probed by rebuilding a scene
            // constructor, not swept from the console like every other
            // field. Raw `u32` — same convention as `material_kind`
            // above — so any bit combination in
            // `byroredux_renderer::vulkan::material::material_flag` is
            // reachable, e.g. `mat.set <id> material_flags 32` sets
            // MAT_FLAG_PBR_BSDF (1 << 5).
            "material_flags" | "flags" => {
                if vals.len() != 1 {
                    Err(format!("expected 1 value, got {}", vals.len()))
                } else {
                    match vals[0].parse::<u32>() {
                        Ok(f) => {
                            m.effect_shader_flags = f;
                            Ok(f.to_string())
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

#[cfg(test)]
mod mat_set_tests {
    use super::*;

    /// Regression for #2477 (REN-D21-2026-08-07-01): `mat.set` had no arm
    /// reaching `Material::effect_shader_flags`, so `MAT_FLAG_PBR_BSDF`
    /// (and every other `material_flag::*` bit) could only be probed by
    /// rebuilding a scene constructor, not swept live like every other
    /// field. `material_flags` (raw `u32`, same convention as
    /// `material_kind`) must write straight through.
    #[test]
    fn material_flags_arm_writes_effect_shader_flags() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Material::default());

        let out = MatSetCommand.execute(&world, &format!("{e} material_flags 32"));
        let joined = out.lines.join("\n");
        assert!(joined.contains("material_flags = 32"), "got: {joined}");

        let q = world.query::<Material>().unwrap();
        assert_eq!(
            q.get(e).unwrap().effect_shader_flags,
            32,
            "mat.set material_flags must write straight into \
             Material::effect_shader_flags (#2477)"
        );
    }

    #[test]
    fn material_flags_arm_rejects_non_integer() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Material::default());

        let out = MatSetCommand.execute(&world, &format!("{e} material_flags not_a_number"));
        let joined = out.lines.join("\n");
        assert!(joined.contains("not an integer"), "got: {joined}");
    }

    /// Regression for #2514 (REN-D21-2026-08-07-02): `mat.set` had no arm
    /// reaching `subsurface`/`sheen`/`sheen_tint`/`anisotropic` — no
    /// source format authors these Disney-BSDF-only scalars, so `mat.set`
    /// was their only possible producer and it didn't reach them either,
    /// leaving the fields dead end-to-end.
    #[test]
    fn disney_lobe_arms_write_their_material_fields() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Material::default());

        for (field, slot_check) in [
            ("subsurface", "subsurface"),
            ("sheen", "sheen"),
            ("sheen_tint", "sheen_tint"),
            ("anisotropic", "anisotropic"),
        ] {
            let out = MatSetCommand.execute(&world, &format!("{e} {field} 0.42"));
            let joined = out.lines.join("\n");
            assert!(
                joined.contains(&format!("{field} = 0.4200")),
                "field {slot_check}: got {joined}"
            );
        }

        let q = world.query::<Material>().unwrap();
        let m = q.get(e).unwrap();
        assert_eq!(m.subsurface, 0.42);
        assert_eq!(m.sheen, 0.42);
        assert_eq!(m.sheen_tint, 0.42);
        assert_eq!(m.anisotropic, 0.42);
    }

    /// Regression for #2823 (REN-D21-01): `MAT_FLAG_TRANSLUCENCY` was
    /// flag-reachable via `material_flags 64` but scalar-unreachable — no
    /// arm authored `translucency_subsurface_color` /
    /// `_transmissive_scale` / `_turbulence`, so the shader's `*
    /// mat.translucencyTransmissiveScale` term was always zero regardless
    /// of the flag.
    #[test]
    fn translucency_arms_write_their_material_fields() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Material::default());

        let out =
            MatSetCommand.execute(&world, &format!("{e} translucency_transmissive_scale 0.75"));
        let joined = out.lines.join("\n");
        assert!(
            joined.contains("translucency_transmissive_scale = 0.7500"),
            "got: {joined}"
        );

        let out = MatSetCommand.execute(&world, &format!("{e} translucency_turbulence 0.3"));
        let joined = out.lines.join("\n");
        assert!(
            joined.contains("translucency_turbulence = 0.3000"),
            "got: {joined}"
        );

        let out = MatSetCommand.execute(
            &world,
            &format!("{e} translucency_subsurface_color 0.1 0.2 0.3"),
        );
        let joined = out.lines.join("\n");
        assert!(
            joined.contains("translucency_subsurface_color = 0.100,0.200,0.300"),
            "got: {joined}"
        );

        let q = world.query::<Material>().unwrap();
        let m = q.get(e).unwrap();
        assert_eq!(m.translucency_transmissive_scale, 0.75);
        assert_eq!(m.translucency_turbulence, 0.3);
        assert_eq!(m.translucency_subsurface_color, [0.1, 0.2, 0.3]);
    }

    /// Regression for #2489 (NIFAL-D6-2026-08-07-03): non-finite input is
    /// rejected rather than silently stored. `"NaN"`/`"inf"` both parse as
    /// `f32`, and `mat.set` is the only writer reaching a resolved
    /// `Material`, so this was the one path that could put the exact value
    /// #1535 guards against on the translate side into `GpuMaterial` — where
    /// it sticks in SVGF/TAA history after the value is corrected.
    #[test]
    fn non_finite_scalars_are_rejected_not_stored() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Material::default());
        let before = world.query::<Material>().unwrap().get(e).unwrap().roughness;

        for literal in ["NaN", "nan", "inf", "-inf", "infinity"] {
            let out = MatSetCommand.execute(&world, &format!("{e} roughness {literal}"));
            let joined = out.lines.join("\n");
            assert!(
                joined.contains("not finite"),
                "`{literal}` must be reported as non-finite, got: {joined}"
            );
            let m = world.query::<Material>().unwrap();
            let after = m.get(e).unwrap().roughness;
            assert_eq!(
                after, before,
                "`{literal}` must leave roughness untouched, got {after}"
            );
        }

        // Colours go through the same parse site — a NaN tint poisons the
        // frame just as well as a NaN scalar.
        let out = MatSetCommand.execute(&world, &format!("{e} diffuse_color 1 NaN 1"));
        assert!(out.lines.join("\n").contains("not finite"));
    }

    /// Regression for #2489: the canonical PBR scalars must land inside the
    /// documented invariant ranges (`metalness ∈ [0,1]`,
    /// `roughness ∈ [0.04,1]`, `alpha ∈ [0,1]`) whether the writer is
    /// `translate_material` or the console, and the console must report the
    /// stored value so the clamp is visible.
    #[test]
    fn out_of_range_pbr_scalars_are_clamped_to_the_resolve_pbr_invariant() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Material::default());

        // (field, input, expected stored value)
        for (field, input, expected) in [
            ("roughness", "0", 0.04_f32), // the plausible typo from the issue
            ("roughness", "5", 1.0),
            ("metalness", "-1", 0.0),
            ("metalness", "3", 1.0),
            ("alpha", "-0.5", 0.0),
            ("alpha", "2", 1.0),
        ] {
            let out = MatSetCommand.execute(&world, &format!("{e} {field} {input}"));
            let joined = out.lines.join("\n");
            assert!(
                joined.contains(&format!("{field} = {expected:.4}")),
                "{field} {input} must report the clamped value {expected:.4}, got: {joined}"
            );
            let q = world.query::<Material>().unwrap();
            let stored = match field {
                "roughness" => q.get(e).unwrap().roughness,
                "metalness" => q.get(e).unwrap().metalness,
                _ => q.get(e).unwrap().alpha,
            };
            assert_eq!(stored, expected, "{field} {input} stored out of range");
        }

        // In-range values pass through untouched.
        MatSetCommand.execute(&world, &format!("{e} roughness 0.37"));
        assert_eq!(
            world.query::<Material>().unwrap().get(e).unwrap().roughness,
            0.37
        );
    }

    /// #2489 — `ior` is deliberately NOT range-clamped: it carries a
    /// physical refractive index (~1.5 for glass) on the dielectric path and
    /// a 0–1 distortion strength under `MATERIAL_KIND_FIRE_REFRACTION`
    /// (#2232). Clamping to either range would corrupt the other.
    #[test]
    fn ior_keeps_its_dual_range_but_still_rejects_non_finite() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Material::default());

        MatSetCommand.execute(&world, &format!("{e} ior 1.5"));
        assert_eq!(world.query::<Material>().unwrap().get(e).unwrap().ior, 1.5);
        MatSetCommand.execute(&world, &format!("{e} ior 0.25"));
        assert_eq!(world.query::<Material>().unwrap().get(e).unwrap().ior, 0.25);

        let out = MatSetCommand.execute(&world, &format!("{e} ior NaN"));
        assert!(out.lines.join("\n").contains("not finite"));
        assert_eq!(world.query::<Material>().unwrap().get(e).unwrap().ior, 0.25);
    }
}
