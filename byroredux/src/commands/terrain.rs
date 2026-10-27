//! `terrain.height` — the per-layer height maps under the camera (#6195).

use super::shared::*;
use crate::asset_provider::terrain_height::LayerHeightSources;
use crate::components::{TerrainCellOrigin, TerrainLayerHeights};
use crate::terrain_height::{HeightState, TerrainHeightService};
use byroredux_core::math::coord::{EXTERIOR_CELL_UNITS, LAND_TEXTURE_TILES_PER_CELL};

/// The LAND texture UV at Y-up world `xz` in the cell whose (row 0, col 0)
/// vertex sits at `origin_xz` — the same mapping `cell_loader/terrain.rs`
/// writes per vertex (`col / 32 · tiles`, `(1 − row / 32) · tiles`), so the
/// sample here is the texel the GPU shades at that point. `None` outside the
/// cell.
pub(crate) fn land_uv_at(origin_xz: [f32; 2], xz: [f32; 2]) -> Option<[f32; 2]> {
    let fc = (xz[0] - origin_xz[0]) / EXTERIOR_CELL_UNITS;
    let fr = (origin_xz[1] - xz[1]) / EXTERIOR_CELL_UNITS;
    ((0.0..=1.0).contains(&fc) && (0.0..=1.0).contains(&fr)).then_some([
        fc * LAND_TEXTURE_TILES_PER_CELL,
        (1.0 - fr) * LAND_TEXTURE_TILES_PER_CELL,
    ])
}

/// `terrain.height` — for the terrain cell under the camera, every layer's
/// height-map source (authored / from normals / from brightness), its
/// generation state, and the raw `[0, 1]` height at the texel under the
/// camera, read back from the cached map exactly as uploaded.
///
/// `terrain.height all` instead tallies every lane of every resident cell by
/// state and names each distinct source still failing or generating — the
/// "every terrain layer in this area has a height map" check.
pub(crate) struct TerrainHeightCommand;
impl ConsoleCommand for TerrainHeightCommand {
    fn name(&self) -> &str {
        "terrain.height"
    }

    fn description(&self) -> &str {
        "Per-layer terrain height maps under the camera: source, state, raw height; `all` tallies every resident cell (#6195)"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        if args.trim() == "all" {
            return census(world);
        }
        let camera = world
            .try_resource::<ActiveCamera>()
            .map(|active| active.0)
            .and_then(|entity| world.get::<GlobalTransform>(entity).map(|t| t.translation));
        let Some(camera) = camera else {
            return CommandOutput::line("no active camera");
        };
        let xz = [camera.x, camera.z];
        let (Some(origins), Some(lanes)) = (
            world.query::<TerrainCellOrigin>(),
            world.query::<TerrainLayerHeights>(),
        ) else {
            return CommandOutput::line("no terrain loaded");
        };
        let Some((uv, lanes)) = lanes.iter().find_map(|(entity, lanes)| {
            let origin = origins.get(entity)?;
            Some((land_uv_at(origin.origin_xz, xz)?, lanes))
        }) else {
            return CommandOutput::line(format!(
                "camera ({:.0}, {:.0}) is over no resident terrain cell",
                xz[0], xz[1]
            ));
        };
        let service = world.try_resource::<TerrainHeightService>();
        let mut out = vec![format!(
            "camera ({:.0}, {:.0}) — LAND uv ({:.3}, {:.3})",
            xz[0], xz[1], uv[0], uv[1]
        )];
        let base = std::iter::once(("base".to_string(), &lanes.base));
        let layers = lanes
            .layers
            .iter()
            .enumerate()
            .map(|(i, s)| (format!("L{i}"), s));
        for (lane, sources) in base.chain(layers) {
            let Some(sources) = sources else { continue };
            out.push(describe_lane(&lane, sources, service.as_deref(), uv));
        }
        CommandOutput::lines(out)
    }
}

/// `terrain.blend [band]` — read or set the height-based layer blend's
/// transition band (#6196) for this session: the same setting the debug-UI
/// "Terrain height-blend band" slider edits, without saving it. 0 turns
/// height blending off, which is the one-binary A/B switch.
pub(crate) struct TerrainBlendCommand;
impl ConsoleCommand for TerrainBlendCommand {
    fn name(&self) -> &str {
        "terrain.blend"
    }

    fn description(&self) -> &str {
        "Terrain height-blend transition band: terrain.blend [0..1] (0 = off; #6196)"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        use byroredux_core::settings::SettingsRegistry;
        use byroredux_debug_ui::TERRAIN_HEIGHT_BLEND_BAND_SETTING_ID as ID;
        if world.try_resource::<SettingsRegistry>().is_none() {
            return CommandOutput::line("terrain.blend: no settings registry in this session");
        }
        let arg = args.trim();
        if !arg.is_empty() {
            let Ok(band) = arg.parse::<f32>() else {
                return CommandOutput::line(format!("terrain.blend: '{arg}' is not a number"));
            };
            if let Err(error) = set_session_number(world, ID, band) {
                return CommandOutput::line(format!("terrain.blend: {error}"));
            }
        }
        let band = crate::setting_number(world, ID, f32::NAN);
        CommandOutput::line(format!(
            "terrain.blend: band={band:.3}{}",
            if band > 0.0 { "" } else { " (height blending off)" }
        ))
    }
}

/// `terrain.soften [amount]` — read or set the full-detail terrain's distance
/// softening toward the LOD look for this session: the same setting the
/// debug-UI "Terrain distance softening" slider edits, without saving it. 0
/// turns it off, which is the one-binary A/B switch.
pub(crate) struct TerrainSoftenCommand;
impl ConsoleCommand for TerrainSoftenCommand {
    fn name(&self) -> &str {
        "terrain.soften"
    }

    fn description(&self) -> &str {
        "Near-terrain distance softening toward the LOD look: terrain.soften [0..1] (0 = off)"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        use byroredux_core::settings::SettingsRegistry;
        use byroredux_debug_ui::TERRAIN_LOD_SOFTEN_SETTING_ID as ID;
        if world.try_resource::<SettingsRegistry>().is_none() {
            return CommandOutput::line("terrain.soften: no settings registry in this session");
        }
        let arg = args.trim();
        if !arg.is_empty() {
            let Ok(amount) = arg.parse::<f32>() else {
                return CommandOutput::line(format!("terrain.soften: '{arg}' is not a number"));
            };
            if let Err(error) = set_session_number(world, ID, amount) {
                return CommandOutput::line(format!("terrain.soften: {error}"));
            }
        }
        let amount = crate::setting_number(world, ID, f32::NAN);
        CommandOutput::line(format!(
            "terrain.soften: amount={amount:.2}{}",
            if amount > 0.0 { "" } else { " (softening off)" }
        ))
    }
}

/// Set a numeric setting through the universal registry for this session
/// only — the shared tail of the `terrain.*` A/B switches. #6204: `settings_io::
/// save` writes every entry that moved, so without the pin the next unrelated
/// settings save would persist this A/B value. A menu edit of the slider
/// releases it (`apply_debug_ui_outputs`).
fn set_session_number(world: &World, id: &str, value: f32) -> Result<(), String> {
    use byroredux_core::settings::{SettingValue, SettingsRegistry};
    let mut result = Ok(false);
    crate::helpers::world_resource_set::<SettingsRegistry>(world, |registry| {
        result = registry.set(id, SettingValue::Number(value));
    });
    result.map_err(|error| error.to_string())?;
    if let Some(mut persistence) = world.try_resource_mut::<crate::settings_io::SettingsPersistence>()
    {
        persistence.pin_stored(id);
    }
    Ok(())
}

/// `terrain.parallax [off | <range_m> [<depth_m>]]` — read or set the
/// near-camera terrain parallax (#6197) for this session: the same two
/// settings the debug-UI "Terrain parallax range / depth" sliders edit,
/// without saving them. `off` (or a range of 0) is the one-binary A/B
/// switch; the range is the camera distance in metres where the effect has
/// faded out, the depth the relief amplitude in metres.
pub(crate) struct TerrainParallaxCommand;
impl ConsoleCommand for TerrainParallaxCommand {
    fn name(&self) -> &str {
        "terrain.parallax"
    }

    fn description(&self) -> &str {
        "Terrain parallax: terrain.parallax [off | <range_m> [<depth_m>]] (0 range = off; #6197)"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        use byroredux_core::settings::SettingsRegistry;
        use byroredux_debug_ui::{
            TERRAIN_PARALLAX_DEPTH_SETTING_ID as DEPTH, TERRAIN_PARALLAX_RANGE_SETTING_ID as RANGE,
        };
        if world.try_resource::<SettingsRegistry>().is_none() {
            return CommandOutput::line("terrain.parallax: no settings registry in this session");
        }
        let words: Vec<&str> = args.split_whitespace().collect();
        let usage = "usage: terrain.parallax [off | <range_m> [<depth_m>]]";
        let parse = |word: &str| word.parse::<f32>().ok().filter(|v| v.is_finite() && *v >= 0.0);
        let (range, depth) = match words.as_slice() {
            [] => (None, None),
            ["off"] => (Some(0.0), None),
            [range] => match parse(range) {
                Some(range) => (Some(range), None),
                None => return CommandOutput::line(format!("terrain.parallax: bad range\n{usage}")),
            },
            [range, depth] => match (parse(range), parse(depth)) {
                (Some(range), Some(depth)) => (Some(range), Some(depth)),
                _ => return CommandOutput::line(format!("terrain.parallax: bad number\n{usage}")),
            },
            _ => return CommandOutput::line(usage),
        };
        for (id, value) in [(RANGE, range), (DEPTH, depth)] {
            if let Some(value) = value {
                if let Err(error) = set_session_number(world, id, value) {
                    return CommandOutput::line(format!("terrain.parallax: {error}"));
                }
            }
        }
        let range = crate::setting_number(world, RANGE, f32::NAN);
        let depth = crate::setting_number(world, DEPTH, f32::NAN);
        CommandOutput::line(format!(
            "terrain.parallax: range={range:.1} m depth={depth:.3} m{}",
            if range > 0.0 && depth > 0.0 { "" } else { " (off)" }
        ))
    }
}

/// Every lane of every resident terrain cell, by generation state.
fn census(world: &World) -> CommandOutput {
    let Some(lanes) = world.query::<TerrainLayerHeights>() else {
        return CommandOutput::line("no terrain loaded");
    };
    let service = world.try_resource::<TerrainHeightService>();
    let (mut cells, mut ready, mut pending, mut failed, mut missing) = (0, 0, 0, 0, 0);
    let mut by_source: std::collections::BTreeMap<String, usize> = Default::default();
    let mut problems: std::collections::BTreeSet<String> = Default::default();
    for (_, cell) in lanes.iter() {
        cells += 1;
        for sources in std::iter::once(&cell.base)
            .chain(cell.layers.iter())
            .flatten()
        {
            match service.as_deref().and_then(|s| s.state(sources)) {
                Some(HeightState::Ready(cached)) => {
                    ready += 1;
                    *by_source
                        .entry(format!("{:?}", cached.meta.source))
                        .or_default() += 1;
                }
                Some(HeightState::Pending) => {
                    pending += 1;
                    problems.insert(format!("  generating: {sources:?}"));
                }
                Some(HeightState::Failed(reason)) => {
                    failed += 1;
                    problems.insert(format!("  FAILED: {reason}"));
                }
                None => {
                    missing += 1;
                    problems.insert(format!("  not requested: {sources:?}"));
                }
            }
        }
    }
    let mut out = vec![format!(
        "terrain.height all: {cells} cells, lanes ready={ready} generating={pending} \
         failed={failed} not-requested={missing}; ready by source {by_source:?}"
    )];
    out.extend(problems);
    CommandOutput::lines(out)
}

fn describe_lane(
    lane: &str,
    sources: &LayerHeightSources,
    service: Option<&TerrainHeightService>,
    uv: [f32; 2],
) -> String {
    let named = sources
        .diffuse
        .as_deref()
        .or(sources.normal.as_deref())
        .unwrap_or("?");
    match service.and_then(|s| s.state(sources)) {
        None => format!("  {lane:<4} not requested  {named}"),
        Some(HeightState::Pending) => format!("  {lane:<4} generating     {named}"),
        Some(HeightState::Failed(reason)) => format!("  {lane:<4} FAILED  {named}: {reason}"),
        Some(HeightState::Ready(cached)) => {
            let raw = std::fs::read(&cached.dds_path)
                .ok()
                .and_then(|dds| byroredux_menuxml::tex::Rgba8::decode_dds(&dds))
                .map(|image| {
                    byroredux_terrain_height::HeightField::from_red_channel(
                        image.width as usize,
                        image.height as usize,
                        &image.pixels,
                    )
                })
                .map(|field| {
                    let x = (uv[0].rem_euclid(1.0) * field.width as f32) as isize;
                    let y = (uv[1].rem_euclid(1.0) * field.height as f32) as isize;
                    format!("{:.3}", field.at(x, y))
                })
                .unwrap_or_else(|| "unreadable".to_string());
            let meta = &cached.meta;
            let green = meta
                .green
                .map(|g| format!(" green={g:?}"))
                .unwrap_or_default();
            format!(
                "  {lane:<4} h={raw}  source={:?}{green} relief={:.2} {}x{}  from {}",
                meta.source, meta.relief, meta.width, meta.height, cached.source_path
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The UV the debugger samples at matches the per-vertex UV terrain spawn
    /// writes: (row 0, col 0) — the cell origin — is `(0, tiles)`, the far
    /// corner `(tiles, 0)`, and a point off the cell has none.
    #[test]
    fn land_uv_matches_the_vertex_mapping() {
        let origin = [4096.0, -8192.0];
        let tiles = LAND_TEXTURE_TILES_PER_CELL;
        assert_eq!(land_uv_at(origin, origin), Some([0.0, tiles]));
        assert_eq!(
            land_uv_at(
                origin,
                [
                    origin[0] + EXTERIOR_CELL_UNITS,
                    origin[1] - EXTERIOR_CELL_UNITS
                ]
            ),
            Some([tiles, 0.0])
        );
        assert_eq!(land_uv_at(origin, [origin[0] - 1.0, origin[1]]), None);
        assert_eq!(land_uv_at(origin, [origin[0], origin[1] + 1.0]), None);
    }

    /// #6204 — `terrain.blend` is session-only: the next unrelated settings
    /// save leaves the band off disk, until a menu edit of the slider makes it
    /// an explicit choice that persists.
    #[test]
    fn terrain_blend_stays_off_disk_until_the_slider_is_edited() {
        use byroredux_core::settings::{SettingChange, SettingValue, SettingsRegistry};
        use byroredux_debug_ui::{FOV_SETTING_ID, TERRAIN_HEIGHT_BLEND_BAND_SETTING_ID as BAND};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let mut settings = SettingsRegistry::default();
        byroredux_debug_ui::register_builtin_settings(&mut settings).unwrap();
        let mut world = World::new();
        world.insert_resource(settings);
        world.insert_resource(crate::settings_io::SettingsPersistence::at(path.clone()));
        let stored = || std::fs::read_to_string(&path).unwrap_or_default();
        let menu_edit = |world: &mut World, id: &str, value: SettingValue| {
            let mut outputs = byroredux_debug_ui::PanelOutputs::default();
            outputs.setting_changes.push(SettingChange::new(id, value));
            crate::apply_debug_ui_outputs(world, outputs, &mut false, None);
        };

        TerrainBlendCommand.execute(&world, "0");
        assert_eq!(crate::setting_number(&world, BAND, f32::NAN), 0.0);
        menu_edit(&mut world, FOV_SETTING_ID, SettingValue::Number(80.0));
        assert!(
            stored().contains(FOV_SETTING_ID),
            "the unrelated edit must save"
        );
        assert!(
            !stored().contains(BAND),
            "a console A/B band must not persist:\n{}",
            stored()
        );

        menu_edit(&mut world, BAND, SettingValue::Number(0.3));
        assert!(
            stored().contains(BAND),
            "a slider edit is an explicit choice:\n{}",
            stored()
        );
    }
}
