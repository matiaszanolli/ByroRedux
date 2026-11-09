//! Prototype: parallax height for static meshes that author none.
//!
//! Terrain layers already get a height map integrated from their normal map
//! (`byroredux-terrain-height`, #6195) and a parallax march on it (#6197). A wall,
//! rock face or other large static mesh runs the same march in `triangle.frag`
//! (`parallaxDisplaceUV`), but only when the material authors a height map, which
//! 98 to 100% of Oblivion, FO3, FNV and Skyrim SE meshes do not. This binds a
//! generated one into that existing slot, so no shader or GPU struct changes.
//!
//! A normal map is only a sound source when it tiles: the integration is a
//! periodic solve, and a map whose edges were authored apart (an atlas chart, a
//! trim sheet) ridges along its tile border. A mesh qualifies only when
//!
//! * it is rigid, opaque, not a decal, and a plain lit material;
//! * its UVs tile (extent above [`MIN_TILING_UV_EXTENT`]), which also keeps the
//!   march from walking off a unique chart into its neighbours;
//! * its normal map's seam score is at most [`MAX_SEAM_EXCEEDANCE`]
//!   (`HeightMeta::seam_exceedance`, measured by the generator).
//!
//! The shader slides the UV by `parallaxHeightScale` directly, so the same scale
//! is a different world depth on a mesh that tiles its texture every 50 units and
//! one that tiles it every 500. [`height_scale`] converts the setting's depth in
//! metres into a UV scale from the mesh's own world-units-per-UV.
//!
//! Generation is synchronous at spawn (the disk cache answers every repeat), so a
//! first visit to a cell pays one FFT per new normal map. Acceptable for a
//! prototype; a shipped version moves it onto the terrain service's pool.
//!
//! Off by default: `render.static_parallax_depth_m` is 0.

use std::collections::HashMap;

use byroredux_core::ecs::components::material::Material;
use byroredux_core::ecs::{Resource, World};
use byroredux_core::lighting::BETHESDA_UNITS_PER_METER;
use byroredux_core::settings::builtin::{
    STATIC_PARALLAX_DEPTH_DEFAULT_M, STATIC_PARALLAX_DEPTH_SETTING_ID,
};
use byroredux_nif::import::{ImportedMesh, MaterialTextureSet};
use byroredux_renderer::VulkanContext;
use byroredux_terrain_height as th;

use crate::asset_provider::terrain_height::{
    default_cache_dir, run_job, CachedHeight, HeightJob, LayerHeightSources,
};
use crate::asset_provider::TextureProvider;
use crate::components::{MaterialTextureHandles, MaterialTextureSource};
use crate::terrain_height::acquire_height_texture;

/// A normal map whose seam score is above this does not tile well enough for
/// the periodic integration. Terrain layers, which tile by construction, scored
/// a p90 of at most 0.062 in every game (`tileability_census`, 2026-11-08);
/// 0.05 sits just inside that. A starting point to tune by eye.
pub(crate) const MAX_SEAM_EXCEEDANCE: f32 = 0.05;

/// UV extent (max over axes of max - min) above which a mesh tiles its texture.
/// A hair over 1 so a chart mapped once onto 0..1 with rounding does not count.
pub(crate) const MIN_TILING_UV_EXTENT: f32 = 1.02;

/// Beyond this, the UVs are corrupt (one FNV mesh authors an extent of ten
/// million), not tiled.
const MAX_PLAUSIBLE_UV_EXTENT: f32 = 1000.0;

/// Largest UV-space scale, the top of the Bethesda brickwork range the shader
/// documents (`material_sampling.glsl`).
const MAX_HEIGHT_SCALE: f32 = 0.08;

/// March layer budget. The shader spends it near grazing angles and falls to
/// eight head-on. Walls are seen edge-on far more than terrain is.
const MARCH_PASSES: f32 = 16.0;

/// The green convention every measured game reads its normal maps with
/// (`asset_provider::terrain_height::normal_map_green`, 159/160 Oblivion,
/// 65/66 FO3, 73/75 FNV, 65/66 Skyrim SE, 54/56 FO4 layers). A wrong read
/// inverts the relief, so the per-map guess is not used: brick and panel maps
/// are exactly the low-decisiveness ones.
const GREEN: th::GreenConvention = th::GreenConvention::YDown;

/// Per normal map, this session: `Some` when it generated and passed the seam
/// gate, `None` when it did not (never retried).
#[derive(Default)]
pub(crate) struct SyntheticHeights {
    verdicts: HashMap<String, Option<CachedHeight>>,
    accepted: usize,
    rejected: usize,
}
impl Resource for SyntheticHeights {}

/// UV extent of `uvs`: the larger of the U and V ranges. 0 for none.
pub(crate) fn uv_extent(uvs: &[[f32; 2]]) -> f32 {
    if uvs.is_empty() {
        return 0.0;
    }
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for uv in uvs {
        for axis in 0..2 {
            lo[axis] = lo[axis].min(uv[axis]);
            hi[axis] = hi[axis].max(uv[axis]);
        }
    }
    (hi[0] - lo[0]).max(hi[1] - lo[1])
}

/// World units per UV unit: `sqrt(world area / UV area)` over the mesh's
/// triangles, `scale` applied. `None` when the UV area is degenerate.
pub(crate) fn units_per_uv(mesh: &ImportedMesh) -> Option<f32> {
    let (mut world, mut uv_area) = (0.0f64, 0.0f64);
    for tri in mesh.indices.chunks_exact(3) {
        let (Some(p), Some(t)) = (
            tri.iter()
                .map(|&i| mesh.positions.get(i as usize).copied())
                .collect::<Option<Vec<_>>>(),
            tri.iter()
                .map(|&i| mesh.uvs.get(i as usize).copied())
                .collect::<Option<Vec<_>>>(),
        ) else {
            continue;
        };
        let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
        let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
        let cross = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        world +=
            0.5 * ((cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]) as f64).sqrt();
        let (u1, u2) = (
            [t[1][0] - t[0][0], t[1][1] - t[0][1]],
            [t[2][0] - t[0][0], t[2][1] - t[0][1]],
        );
        uv_area += 0.5 * ((u1[0] * u2[1] - u1[1] * u2[0]) as f64).abs();
    }
    if uv_area <= 1.0e-9 || world <= 0.0 {
        return None;
    }
    let units = (world / uv_area).sqrt() as f32 * mesh.scale;
    (units.is_finite() && units > 0.0).then_some(units)
}

/// The UV-space `parallaxHeightScale` that gives `depth_units` of world depth
/// where one UV unit spans `units_per_uv` world units. Capped at
/// [`MAX_HEIGHT_SCALE`]: a mesh that maps one texture across a huge face would
/// otherwise ask for a slide of most of the tile.
pub(crate) fn height_scale(depth_units: f32, units_per_uv: f32) -> f32 {
    (depth_units / units_per_uv).clamp(0.0, MAX_HEIGHT_SCALE)
}

/// The geometry and material half of the gate: everything knowable before the
/// normal map is read. `normal` is the resolved normal-map path.
pub(crate) fn mesh_qualifies(
    mesh: &ImportedMesh,
    material: &Material,
    normal: Option<&str>,
) -> bool {
    let m = &mesh.material;
    normal.is_some()
        && mesh.skin.is_none()
        && !m.is_decal
        && !m.has_alpha
        && !m.alpha_test
        && !m.model_space_normals
        && material.material_kind == 0
        && {
            let extent = uv_extent(&mesh.uvs);
            extent > MIN_TILING_UV_EXTENT && extent < MAX_PLAUSIBLE_UV_EXTENT
        }
}

/// The seam half of the gate, on a generated map.
pub(crate) fn map_tiles(meta: &th::HeightMeta) -> bool {
    meta.source == th::HeightSource::Normals
        && meta
            .seam_exceedance
            .is_some_and(|seam| seam <= MAX_SEAM_EXCEEDANCE)
}

/// The verdict for one normal map: generate (or load) its height map and apply
/// the seam gate. Synchronous.
fn verdict(
    world: &mut World,
    tex_provider: &TextureProvider,
    normal: &str,
) -> Option<CachedHeight> {
    let sources = LayerHeightSources::new(None, Some(normal), None)?;
    let key = sources.normal.clone()?;
    if world.try_resource::<SyntheticHeights>().is_none() {
        world.insert_resource(SyntheticHeights::default());
    }
    if let Some(known) = world.resource::<SyntheticHeights>().verdicts.get(&key) {
        return known.clone();
    }
    let started = std::time::Instant::now();
    let job = HeightJob {
        sources,
        green: Some(GREEN),
        archives: tex_provider.texture_archive_snapshot(),
        cache_dir: default_cache_dir(),
    };
    let result = run_job(&job);
    let accepted = match &result {
        Ok(cached) if map_tiles(&cached.meta) => Some(cached.clone()),
        _ => None,
    };
    match &result {
        Ok(cached) => log::info!(
            target: "engine::static_height",
            "{key}: seam {:.3} -> {} ({} ms)",
            cached.meta.seam_exceedance.unwrap_or(f32::NAN),
            if accepted.is_some() { "height bound" } else { "rejected, stays flat" },
            started.elapsed().as_millis(),
        ),
        Err(reason) => log::warn!(target: "engine::static_height", "{key}: {reason}"),
    }
    let mut state = world.resource_mut::<SyntheticHeights>();
    if accepted.is_some() {
        state.accepted += 1;
    } else {
        state.rejected += 1;
    }
    state.verdicts.insert(key, accepted.clone());
    accepted
}

/// Spawn hook: when the setting is on and the mesh qualifies, bind a generated
/// height map and its UV scale into `handles`. A no-op otherwise, and always when
/// the material already has a height map (authored, or Oblivion's alpha route).
pub(crate) fn bind_synthetic_height(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    mesh: &ImportedMesh,
    material: &Material,
    textures: &MaterialTextureSet<Option<String>>,
    handles: &mut MaterialTextureHandles,
    sources: &mut MaterialTextureSet<MaterialTextureSource>,
) {
    let depth_m = crate::setting_number(
        world,
        STATIC_PARALLAX_DEPTH_SETTING_ID,
        STATIC_PARALLAX_DEPTH_DEFAULT_M,
    );
    if depth_m <= 0.0 || handles.textures.height != 0 {
        return;
    }
    let Some(normal) = textures.normal.as_deref() else {
        return;
    };
    if !mesh_qualifies(mesh, material, Some(normal)) {
        return;
    }
    let Some(units) = units_per_uv(mesh) else {
        return;
    };
    let Some(cached) = verdict(world, tex_provider, normal) else {
        return;
    };
    let handle = acquire_height_texture(ctx, &cached);
    if handle == 0 {
        return;
    }
    handles.textures.height = handle;
    handles.parallax_height_scale = height_scale(depth_m * BETHESDA_UNITS_PER_METER, units);
    handles.parallax_max_passes = MARCH_PASSES;
    sources.height = MaterialTextureSource::SyntheticHeight;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(size: f32, uv: f32, scale: f32) -> ImportedMesh {
        let mut mesh = ImportedMesh::from_geometry(
            vec![
                [0.0, 0.0, 0.0],
                [size, 0.0, 0.0],
                [size, size, 0.0],
                [0.0, size, 0.0],
            ],
            vec![[1.0; 4]; 4],
            vec![[0.0, 0.0, 1.0]; 4],
            Vec::new(),
            vec![[0.0, 0.0], [uv, 0.0], [uv, uv], [0.0, uv]],
            vec![0, 1, 2, 0, 2, 3],
        );
        mesh.scale = scale;
        mesh
    }

    #[test]
    fn uv_extent_is_the_larger_axis_range() {
        assert_eq!(uv_extent(&[]), 0.0);
        assert_eq!(uv_extent(&[[0.0, 0.0], [4.0, 1.0]]), 4.0);
        assert_eq!(uv_extent(&[[2.0, 5.0], [3.0, 8.0]]), 3.0);
    }

    /// A 280-unit wall that repeats its texture four times: 70 units per tile.
    #[test]
    fn units_per_uv_is_the_world_span_of_one_tile() {
        let units = units_per_uv(&quad(280.0, 4.0, 1.0)).unwrap();
        assert!((units - 70.0).abs() < 0.01, "{units}");
        let scaled = units_per_uv(&quad(280.0, 4.0, 2.0)).unwrap();
        assert!((scaled - 140.0).abs() < 0.01, "{scaled}");
    }

    #[test]
    fn units_per_uv_rejects_a_degenerate_uv_map() {
        assert!(units_per_uv(&quad(280.0, 0.0, 1.0)).is_none());
    }

    /// 8 cm of relief (5.6 units) on a 70-unit tile is a slide of 0.08 of the
    /// tile, and a tile ten times larger asks for a tenth of that.
    #[test]
    fn height_scale_converts_world_depth_to_uv_slide() {
        let depth = 0.08 * BETHESDA_UNITS_PER_METER;
        assert!((height_scale(depth, 70.0) - 0.08).abs() < 1e-6);
        assert!((height_scale(depth, 700.0) - 0.008).abs() < 1e-6);
    }

    /// A one-off face mapped once across hundreds of units must not ask for the
    /// whole tile; it is capped at the brickwork range.
    #[test]
    fn height_scale_is_capped() {
        assert_eq!(height_scale(100.0, 1.0), MAX_HEIGHT_SCALE);
    }

    #[test]
    fn gate_wants_a_tiling_rigid_opaque_lit_mesh_with_a_normal_map() {
        let material = Material::default();
        let tiling = quad(280.0, 4.0, 1.0);
        assert!(mesh_qualifies(&tiling, &material, Some("n.dds")));
        assert!(!mesh_qualifies(&tiling, &material, None), "no normal map");
        assert!(
            !mesh_qualifies(&quad(280.0, 1.0, 1.0), &material, Some("n.dds")),
            "a chart mapped once is not a tiling material"
        );
        assert!(
            !mesh_qualifies(&quad(280.0, 1.0e7, 1.0), &material, Some("n.dds")),
            "corrupt UVs are not a tiling material"
        );

        let mut blended = quad(280.0, 4.0, 1.0);
        blended.material.has_alpha = true;
        assert!(!mesh_qualifies(&blended, &material, Some("n.dds")));
        let mut decal = quad(280.0, 4.0, 1.0);
        decal.material.is_decal = true;
        assert!(!mesh_qualifies(&decal, &material, Some("n.dds")));
        let mut cutout = quad(280.0, 4.0, 1.0);
        cutout.material.alpha_test = true;
        assert!(!mesh_qualifies(&cutout, &material, Some("n.dds")));

        let special = Material {
            material_kind: 11,
            ..Material::default()
        };
        assert!(!mesh_qualifies(&tiling, &special, Some("n.dds")));
    }

    #[test]
    fn seam_gate_reads_the_generator_metadata() {
        let meta = |source, seam| th::HeightMeta {
            source,
            generator_version: th::GENERATOR_VERSION,
            width: 8,
            height: 8,
            relief: 1.0,
            green: None,
            detected_green: None,
            convention_decisiveness: None,
            seam_exceedance: seam,
        };
        assert!(map_tiles(&meta(th::HeightSource::Normals, Some(0.01))));
        assert!(!map_tiles(&meta(th::HeightSource::Normals, Some(0.3))));
        assert!(
            !map_tiles(&meta(th::HeightSource::Normals, None)),
            "unscored"
        );
        assert!(!map_tiles(&meta(th::HeightSource::Brightness, Some(0.0))));
    }
}
