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
    ((0.0..=1.0).contains(&fc) && (0.0..=1.0).contains(&fr)).then(|| {
        [
            fc * LAND_TEXTURE_TILES_PER_CELL,
            (1.0 - fr) * LAND_TEXTURE_TILES_PER_CELL,
        ]
    })
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
                .and_then(|dds| byroredux_terrain_height::decode_top_mip(&dds))
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
}
