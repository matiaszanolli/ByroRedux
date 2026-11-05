//! The colour of the grass a worldspace's records grow, read from their own
//! textures (EXAL §12.3).
//!
//! Every `GRAS` record names a model of alpha-cut cards painted with the
//! grass's real colour. The procedural blade tier replaces those cards in the
//! near field, so its colour is taken from them rather than from a hand-built
//! palette: the mean of each record's diffuse texture over the pixels the
//! alpha test keeps (the visible leaf), averaged in linear light — the
//! physically right way to average reflectance.
//!
//! One-time CPU work when a worldspace is entered: a mesh parse and one DDS
//! decode per record (27 on Skyrim). Nothing runs per frame.

use crate::asset_provider::TextureProvider;
use crate::cell_loader::nif_import_registry::canonical_model_path_key;
use crate::cell_loader::references::parse_and_import_nif_pub;
use byroredux_core::ecs::components::groundcover::AuthoredCover;
use byroredux_core::ecs::World;
use byroredux_menuxml::tex::Rgba8;

/// Alpha at or above which a texel counts as leaf. The cards are alpha-tested;
/// 0.5 is the cut-out the shipped materials use.
const ALPHA_CUT: u8 = 128;
/// Opaque (at least this much of the texture survives the cut): a rock or
/// other solid mesh, not a plant — it carries no grass colour.
const OPAQUE_COVERAGE: f64 = 0.995;

/// sRGB byte → linear light.
fn srgb_to_linear(byte: u8) -> f32 {
    let c = f32::from(byte) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Mean linear-RGB colour of the texels the alpha test keeps, or `None` for an
/// empty or fully opaque image.
pub(crate) fn mean_leaf_colour(image: &Rgba8) -> Option<[f32; 3]> {
    let lut: [f32; 256] = std::array::from_fn(|i| srgb_to_linear(i as u8));
    let (mut sum, mut kept) = ([0.0f64; 3], 0u64);
    for px in image.pixels.chunks_exact(4) {
        if px[3] >= ALPHA_CUT {
            sum[0] += f64::from(lut[px[0] as usize]);
            sum[1] += f64::from(lut[px[1] as usize]);
            sum[2] += f64::from(lut[px[2] as usize]);
            kept += 1;
        }
    }
    let total = (image.pixels.len() / 4) as f64;
    if kept == 0 || kept as f64 / total >= OPAQUE_COVERAGE {
        return None;
    }
    Some(sum.map(|s| (s / kept as f64) as f32))
}

/// Fill every record's `mean_colour` from its model's diffuse texture.
pub(crate) fn fill_mean_colours(
    world: &mut World,
    tex_provider: &TextureProvider,
    cover: &mut AuthoredCover,
) {
    let mut coloured = 0usize;
    for record in &mut cover.records {
        record.mean_colour = record_colour(world, tex_provider, &record.model_path);
        coloured += usize::from(record.mean_colour.is_some());
    }
    log::info!(
        target: "engine::groundcover",
        "authored ground cover: {coloured}/{} GRAS records carry a texture colour",
        cover.records.len(),
    );
}

fn record_colour(
    world: &mut World,
    tex_provider: &TextureProvider,
    model_path: &str,
) -> Option<[f32; 3]> {
    let cache_key = canonical_model_path_key(model_path);
    let bytes = tex_provider.extract_mesh(&cache_key)?;
    let cached = {
        let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
        parse_and_import_nif_pub(
            &bytes,
            &cache_key,
            None,
            &mut pool,
            Some(tex_provider),
            &|p| tex_provider.has_texture(p),
            false,
        )
    }?;
    let texture = {
        let pool = world.resource::<byroredux_core::string::StringPool>();
        cached.meshes.iter().find_map(|mesh| {
            mesh.material
                .textures
                .base_color
                .and_then(|sym| pool.resolve(sym))
                .map(str::to_owned)
        })
    }?;
    let image = Rgba8::decode_dds(&tex_provider.extract(&texture)?)?;
    mean_leaf_colour(&image)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(pixels: &[[u8; 4]]) -> Rgba8 {
        Rgba8 {
            width: pixels.len() as u32,
            height: 1,
            pixels: pixels.iter().flatten().copied().collect(),
        }
    }

    #[test]
    fn only_alpha_kept_texels_contribute() {
        // A leaf texel and a cut-out texel whose RGB must not leak in.
        let c = mean_leaf_colour(&image(&[[255, 255, 255, 255], [0, 0, 0, 0]])).unwrap();
        assert!(c.iter().all(|v| (v - 1.0).abs() < 1e-6), "{c:?}");
    }

    #[test]
    fn the_mean_is_taken_in_linear_light() {
        // Mean of sRGB 0 and 255 is linear 0.5, not the linear value of sRGB 128 (~0.216).
        let c = mean_leaf_colour(&image(&[
            [0, 0, 0, 255],
            [255, 255, 255, 255],
            [0, 0, 0, 0],
            [0, 0, 0, 0],
        ]))
        .unwrap();
        assert!((c[0] - 0.5).abs() < 1e-6, "{c:?}");
    }

    #[test]
    fn an_opaque_mesh_has_no_grass_colour() {
        assert_eq!(mean_leaf_colour(&image(&[[10, 200, 30, 255]; 4])), None);
        assert_eq!(mean_leaf_colour(&image(&[[10, 200, 30, 0]; 4])), None);
    }
}
