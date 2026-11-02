//! Cornell scene → Mitsuba 3 exporter (M51 PTR-VAL-004, #6600; design in
//! `docs/engine/path-tracing.md` §1, `PTX-`).
//!
//! Writes the scene a `--cornell-reference` run spawned, from the live ECS
//! entities, so the external reference can never drift from a hand-kept
//! copy. The output is deterministic (PTX-002): shapes in `Name` order,
//! floats in Rust's shortest round-trip form, PLY files named by a hash of
//! their bytes. Positions are exported in metres (PTX-004) because the
//! engine's inverse-square law divides by distance in metres.
//!
//! Every emitter's intensity is multiplied by its expected factor
//! *k*<sub>exp</sub> (RLT-REF-010, RLT-EXP-005; derivation in the design's
//! §2.1) and the factors are written into `scene.xml` and `export.tsv`
//! (PTX-006). No factor is ever applied to an image.
//!
//! The exporter refuses what reference rungs exclude (PTX-005,
//! RLT-REF-009) instead of approximating it.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use byroredux_core::ecs::{
    Camera, EmitterKind, GlobalTransform, LightSource, Material, MeshHandle,
    Resource, World,
};
use byroredux_core::lighting::{AttenuationModel, BETHESDA_UNITS_PER_METER};
use byroredux_core::math::{Mat4, Vec3};

/// *k*<sub>exp</sub> for point emitters: the engine's direct lobe is the
/// legacy non-/π Lambert (`diffuseBrdf = kD * albedo`, `include/lighting.glsl`)
/// and the reference's is ρ ⁄ π, so the reference emitter carries π× the
/// engine's radiant intensity (design §2.1).
pub(crate) const K_EXP_POINT: f32 = std::f32::consts::PI;

/// *k*<sub>exp</sub> for the sun: the same non-/π lobe against the
/// directional light's irradiance (design §2.1).
pub(crate) const K_EXP_SUN: f32 = std::f32::consts::PI;

/// Distance, in metres, of the sphere that stands in for the sun (RLT-EXP-004,
/// V1): far enough that its parallax across a Cornell rung is nil, close
/// enough that f32 keeps its radius exact. Its radius is D · sin θ, so it
/// subtends exactly the engine's angular radius θ.
const SUN_DISTANCE_M: f32 = 10_000.0;

/// CPU copy of one uploaded Cornell mesh (PTX-003).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CornellMeshSource {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Exported only for meshes a textured shape uses (RLT-EXP-002).
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

/// CPU copy of one RGBA8 texture a Cornell rung registered (RLT-EXP-002).
/// `srgb`: the GPU format decodes the colour channels from sRGB before
/// filtering (`register_rgba` uploads `R8G8B8A8_SRGB`); alpha is linear.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CornellTextureSource {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub srgb: bool,
}

/// Texture sources keyed by bindless texture handle.
#[derive(Default, Debug)]
pub(crate) struct CornellTextureSources(pub HashMap<u32, CornellTextureSource>);
impl Resource for CornellTextureSources {}

/// An alpha-tested shape's coverage (RLT-CON-008): the shadow ray's test in
/// `include/ray_hit.glsl` (`rayHitHasCoverage`), base-texture alpha at mip 0
/// times the material alpha, compared by `alphaComparePass`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExportCutout {
    pub texture: u32,
    pub threshold: f32,
    /// Gamebyro test function: 0 always … 6 greater-or-equal, 7 never.
    pub func: u8,
    pub material_alpha: f32,
}

/// Mesh sources keyed by mesh handle, recorded by `MeshBuilder` for the
/// scenes the exporter can serialise.
#[derive(Default, Debug)]
pub(crate) struct CornellMeshSources(pub HashMap<u32, CornellMeshSource>);
impl Resource for CornellMeshSources {}

/// Which BSDF a shape exports as (design §2.2).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ExportBsdf {
    /// `MAT_FLAG_LAMBERTIAN_REFERENCE` material → Mitsuba `diffuse`.
    Diffuse { reflectance: [f32; 3] },
    /// Any other dielectric or conductor → Mitsuba `principled`.
    Principled {
        base_color: [f32; 3],
        metallic: f32,
        roughness: f32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExportShape {
    pub name: String,
    pub mesh: u32,
    /// World units, as `GlobalTransform` holds it; scaled to metres on write.
    pub to_world: Mat4,
    pub bsdf: ExportBsdf,
    /// Tangent-space normal map, by texture handle (RLT-EXP-002; the
    /// convention is the design's §2.2 normal-map row).
    pub normal_map: Option<u32>,
    pub cutout: Option<ExportCutout>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExportEmitter {
    pub name: String,
    /// World units.
    pub position: Vec3,
    /// Engine radiant intensity (scene W·sr⁻¹), before *k*<sub>exp</sub>.
    pub intensity: [f32; 3],
    /// Metres; exported as a sphere of this radius (RLT-REF-002).
    pub source_radius_m: f32,
}

/// The directional light as the renderer uploads it (RLT-EXP-004).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExportSun {
    /// Unit vector toward the sun.
    pub direction: Vec3,
    /// Engine irradiance at normal incidence (`compute_directional_upload`).
    pub irradiance: [f32; 3],
    /// Angular radius (rad) the shader's disk sampling uses (`skyTint.w`).
    pub angular_radius: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExportCamera {
    /// World units.
    pub origin: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
    pub fov_y_radians: f32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExportScene {
    pub shapes: Vec<ExportShape>,
    pub emitters: Vec<ExportEmitter>,
    pub sun: Option<ExportSun>,
    pub camera: ExportCamera,
    /// The rung's measurement geometry, written as `rung.tsv`.
    pub measurements: super::reference::RungMeasurements,
}

/// What [`export_mitsuba`] wrote.
#[derive(Debug)]
pub(crate) struct ExportReport {
    pub scene_xml: PathBuf,
    pub shapes: usize,
    pub emitters: usize,
}

/// Gather the scene from the ECS, refusing anything a reference rung must
/// not contain (PTX-005). `extent` is the render extent the engine draws.
pub(crate) fn gather_export_scene(world: &World, extent: (u32, u32)) -> Result<ExportScene, String> {
    let names = entity_names(world);
    let name_of = |entity: byroredux_core::ecs::EntityId| -> String {
        names
            .get(&entity)
            .cloned()
            .unwrap_or_else(|| format!("entity_{entity}"))
    };

    if world.query::<byroredux_core::ecs::FogVolume>().is_some_and(|q| q.iter().next().is_some()) {
        return Err("reference rungs carry no fog volume (RLT-REF-009)".into());
    }

    let mut shapes = Vec::new();
    let texture_roles = world.query::<crate::components::MaterialTextureHandles>();
    let base_textures = world.query::<byroredux_core::ecs::TextureHandle>();
    let alpha_blended = world.query::<crate::components::AlphaBlend>();
    if let (Some(meshes), Some(transforms), Some(materials)) = (
        world.query::<MeshHandle>(),
        world.query::<GlobalTransform>(),
        world.query::<Material>(),
    ) {
        for (entity, mesh) in meshes.iter() {
            let name = name_of(entity);
            let Some(transform) = transforms.get(entity) else {
                return Err(format!("{name}: mesh without GlobalTransform"));
            };
            let Some(material) = materials.get(entity) else {
                return Err(format!("{name}: mesh without Material"));
            };
            let normal_map = texture_roles
                .as_ref()
                .and_then(|q| q.get(entity))
                .map(|handles| handles.textures.normal)
                .filter(|&handle| handle != 0);
            if alpha_blended.as_ref().is_some_and(|q| q.get(entity).is_some()) {
                return Err(format!("{name}: alpha-blended materials are excluded (RLT-REF-009)"));
            }
            let cutout = if material.alpha_test && material.alpha_threshold > 0.0 {
                let texture = base_textures
                    .as_ref()
                    .and_then(|q| q.get(entity))
                    .map(|handle| handle.0)
                    .ok_or_else(|| format!("{name}: an alpha-tested shape needs a base texture"))?;
                Some(ExportCutout {
                    texture,
                    threshold: material.alpha_threshold,
                    func: material.alpha_test_func,
                    material_alpha: material.alpha,
                })
            } else {
                None
            };
            shapes.push(ExportShape {
                bsdf: export_bsdf(&name, material)?,
                name,
                mesh: mesh.0,
                to_world: transform.to_matrix(),
                normal_map,
                cutout,
            });
        }
    }

    let mut emitters = Vec::new();
    if let (Some(lights), Some(transforms)) =
        (world.query::<LightSource>(), world.query::<GlobalTransform>())
    {
        for (entity, light) in lights.iter() {
            let name = name_of(entity);
            let emitter = light.emitter;
            match (emitter.kind, emitter.attenuation) {
                (EmitterKind::Point, AttenuationModel::InverseSquare) => {}
                (EmitterKind::Point, AttenuationModel::LegacySoftRange) => {
                    return Err(format!(
                        "{name}: LegacySoftRange point emitters have no external counterpart (RLT-REF-002)"
                    ))
                }
                (kind, _) => {
                    return Err(format!(
                        "{name}: {kind:?} emitters are not exported in this phase (sun: RLT-EXP-004)"
                    ))
                }
            }
            let Some(transform) = transforms.get(entity) else {
                return Err(format!("{name}: light without GlobalTransform"));
            };
            let scale = light.dimmer * light.intensity;
            emitters.push(ExportEmitter {
                name,
                position: transform.translation,
                intensity: emitter.radiant_intensity.scaled(scale),
                source_radius_m: emitter.source_radius.get(),
            });
        }
    }

    let camera = {
        let cameras = world
            .query::<Camera>()
            .ok_or("no Camera component in the world")?;
        let transforms = world
            .query::<GlobalTransform>()
            .ok_or("no GlobalTransform storage")?;
        let mut found = None;
        for (entity, camera) in cameras.iter() {
            if let Some(transform) = transforms.get(entity) {
                found = Some((camera.fov_y, *transform));
                break;
            }
        }
        let (fov_y, transform) = found.ok_or("the Camera entity has no GlobalTransform")?;
        ExportCamera {
            origin: transform.translation,
            forward: transform.rotation * Vec3::NEG_Z,
            up: transform.rotation * Vec3::Y,
            fov_y_radians: fov_y,
            width: extent.0,
            height: extent.1,
        }
    };

    shapes.sort_by(|a, b| a.name.cmp(&b.name).then(a.mesh.cmp(&b.mesh)));
    emitters.sort_by(|a, b| a.name.cmp(&b.name));
    let sun = gather_sun(world);
    if emitters.is_empty() && sun.is_none() {
        return Err("a reference rung needs at least one emitter".into());
    }
    Ok(ExportScene {
        shapes,
        emitters,
        sun,
        camera,
        measurements: world
            .try_resource::<super::reference::RungMeasurements>()
            .map(|m| m.clone())
            .unwrap_or_default(),
    })
}

/// The directional light exactly as `collect_lights` builds it: the cell's
/// directional colour through `compute_directional_upload`, the angular
/// radius `build_sky_params` hands the shader. The two resources are read
/// one at a time, in the order the renderer reads them (#1410 lock order).
fn gather_sun(world: &World) -> Option<ExportSun> {
    use crate::components::{CellLightingRes, SkyParamsRes};
    let sun_intensity = world
        .try_resource::<SkyParamsRes>()
        .map(|sky| sky.sun_intensity)
        .unwrap_or(crate::env_translate::SUN_INTENSITY_PEAK);
    let (irradiance, direction) = {
        let cell = world.try_resource::<CellLightingRes>()?;
        let irradiance = crate::render::compute_directional_upload(
            &cell.directional_color,
            cell.is_interior,
            sun_intensity,
            cell.directional_fade,
        );
        (irradiance, Vec3::from_array(cell.directional_dir))
    };
    if !irradiance.iter().any(|c| *c > 0.0) || direction.length_squared() < 1e-12 {
        return None;
    }
    let angular_radius = crate::render::sky::build_sky_params(world).sun_angular_radius;
    Some(ExportSun {
        direction: direction.normalize(),
        irradiance,
        angular_radius,
    })
}

fn entity_names(world: &World) -> HashMap<byroredux_core::ecs::EntityId, String> {
    let mut names = HashMap::new();
    let (Some(query), Some(pool)) = (
        world.query::<byroredux_core::ecs::components::Name>(),
        world.try_resource::<byroredux_core::string::StringPool>(),
    ) else {
        return names;
    };
    for (entity, name) in query.iter() {
        if let Some(text) = pool.resolve(name.0) {
            names.insert(entity, text.to_string());
        }
    }
    names
}

fn export_bsdf(name: &str, material: &Material) -> Result<ExportBsdf, String> {
    use byroredux_core::ecs::components::material::EmissiveSource;
    let emissive = material.emissive_mult
        * material
            .emissive_color
            .iter()
            .fold(0.0f32, |acc, c| acc.max(*c));
    if emissive > 0.0 && !matches!(material.emissive_source, EmissiveSource::None) {
        return Err(format!("{name}: emissive materials are excluded (RLT-REF-009, RLT-NG-007)"));
    }
    if material.material_kind != 0 {
        return Err(format!(
            "{name}: material kind {} (glass, eye, effect, …) is excluded (RLT-REF-009)",
            material.material_kind
        ));
    }
    let lambertian = material.effect_shader_flags
        & byroredux_renderer::vulkan::material::material_flag::LAMBERTIAN_REFERENCE
        != 0;
    Ok(if lambertian {
        ExportBsdf::Diffuse {
            reflectance: material.diffuse_color.map(|c| c * (1.0 - material.metalness)),
        }
    } else {
        ExportBsdf::Principled {
            base_color: material.diffuse_color,
            metallic: material.metalness,
            roughness: material.roughness,
        }
    })
}

/// Texture files written for one shape (RLT-EXP-002), relative to the
/// export directory.
#[derive(Default, Debug, Clone, PartialEq)]
pub(crate) struct ShapeTextureFiles {
    pub normal_map: Option<String>,
    pub opacity: Option<String>,
}

/// Write `DIR/scene.xml`, `DIR/meshes/*.ply`, `DIR/textures/*.exr`,
/// `DIR/export.tsv` and `DIR/rung.tsv`.
pub(crate) fn export_mitsuba(
    scene: &ExportScene,
    sources: &CornellMeshSources,
    textures: &CornellTextureSources,
    dir: &Path,
) -> Result<ExportReport, String> {
    let meshes_dir = dir.join("meshes");
    std::fs::create_dir_all(&meshes_dir).map_err(|e| format!("create {}: {e}", meshes_dir.display()))?;

    // Texture coordinates go only into meshes a textured shape draws, so an
    // untextured rung's PLY bytes, and with them its reference cache keys
    // (PTX-008), do not change.
    let textured_meshes: std::collections::BTreeSet<u32> = scene
        .shapes
        .iter()
        .filter(|shape| shape.normal_map.is_some() || shape.cutout.is_some())
        .map(|shape| shape.mesh)
        .collect();
    // One PLY per distinct mesh, named by a hash of its bytes (PTX-002).
    let mut mesh_files: BTreeMap<u32, String> = BTreeMap::new();
    for shape in &scene.shapes {
        if mesh_files.contains_key(&shape.mesh) {
            continue;
        }
        let source = sources.0.get(&shape.mesh).ok_or_else(|| {
            format!(
                "{}: mesh {} has no recorded CPU source; only scenes built with \
                 MeshBuilder::finish_recording_sources can be exported (PTX-003)",
                shape.name, shape.mesh
            )
        })?;
        let bytes = ply_bytes(source, textured_meshes.contains(&shape.mesh));
        let file = format!("meshes/{:016x}.ply", fnv1a64(&bytes));
        std::fs::write(dir.join(&file), &bytes).map_err(|e| format!("write {file}: {e}"))?;
        mesh_files.insert(shape.mesh, file);
    }

    let mut texture_files: BTreeMap<String, ShapeTextureFiles> = BTreeMap::new();
    for shape in &scene.shapes {
        let source_of = |handle: u32| {
            textures.0.get(&handle).ok_or_else(|| {
                format!(
                    "{}: texture {handle} has no recorded CPU source; register rung \
                     textures through MeshBuilder::register_texture (RLT-EXP-002)",
                    shape.name
                )
            })
        };
        let mut files = ShapeTextureFiles::default();
        if let Some(handle) = shape.normal_map {
            let (w, h, rgb) = bake_normal_map(source_of(handle)?);
            files.normal_map = Some(write_texture(dir, w, h, &rgb)?);
        }
        if let Some(cutout) = &shape.cutout {
            let (w, h, rgb) = bake_opacity(source_of(cutout.texture)?, cutout);
            files.opacity = Some(write_texture(dir, w, h, &rgb)?);
        }
        if files != ShapeTextureFiles::default() {
            texture_files.insert(shape.name.clone(), files);
        }
    }

    let xml = scene_xml(scene, &mesh_files, &texture_files);
    let scene_xml = dir.join("scene.xml");
    std::fs::write(&scene_xml, xml).map_err(|e| format!("write {}: {e}", scene_xml.display()))?;
    std::fs::write(dir.join("export.tsv"), export_tsv(scene))
        .map_err(|e| format!("write export.tsv: {e}"))?;
    std::fs::write(dir.join("rung.tsv"), rung_tsv(&scene.measurements))
        .map_err(|e| format!("write rung.tsv: {e}"))?;
    Ok(ExportReport {
        scene_xml,
        shapes: scene.shapes.len(),
        emitters: scene.emitters.len(),
    })
}

/// Texels each source texel is baked into, per axis. The engine filters a
/// texture bilinearly and then decodes or tests it; Mitsuba filters what it
/// is given. Baking the engine's filtered-then-decoded value on a grid this
/// much finer leaves Mitsuba's own interpolation an O(1/S²) error.
const TEXTURE_BAKE_SCALE: u32 = 16;

/// The engine's sample of one channel: `R8G8B8A8` texels (colour decoded
/// from sRGB first when the format is sRGB, as the GPU does before
/// filtering), bilinear, repeat addressing, texel centres at +0.5, mip 0.
fn sample_engine(tex: &CornellTextureSource, u: f32, v: f32, channel: usize) -> f32 {
    let texel = |x: i64, y: i64| {
        let x = x.rem_euclid(i64::from(tex.width)) as usize;
        let y = y.rem_euclid(i64::from(tex.height)) as usize;
        let byte = f32::from(tex.rgba[(y * tex.width as usize + x) * 4 + channel]) / 255.0;
        if tex.srgb && channel < 3 {
            srgb_to_linear(byte)
        } else {
            byte
        }
    };
    let x = u * tex.width as f32 - 0.5;
    let y = v * tex.height as f32 - 0.5;
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (x0, y0) = (x0 as i64, y0 as i64);
    let top = texel(x0, y0) * (1.0 - fx) + texel(x0 + 1, y0) * fx;
    let bottom = texel(x0, y0 + 1) * (1.0 - fx) + texel(x0 + 1, y0 + 1) * fx;
    top * (1.0 - fy) + bottom * fy
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Bake `f(u, v)` at the centres of a grid `TEXTURE_BAKE_SCALE` × finer.
fn bake(tex: &CornellTextureSource, f: impl Fn(f32, f32) -> [f32; 3]) -> (u32, u32, Vec<f32>) {
    let (w, h) = (tex.width * TEXTURE_BAKE_SCALE, tex.height * TEXTURE_BAKE_SCALE);
    let mut out = Vec::with_capacity((w * h * 3) as usize);
    for j in 0..h {
        for i in 0..w {
            let value = f((i as f32 + 0.5) / w as f32, (j as f32 + 0.5) / h as f32);
            out.extend_from_slice(&value);
        }
    }
    (w, h, out)
}

/// The normal map as the engine decodes it (`perturbNormalGrad`,
/// `include/material_sampling.glsl`): xy = 2·rg − 1 after filtering, z
/// rebuilt as √(1 − x² − y²) (the stored blue is ignored), re-encoded so
/// Mitsuba's `normalmap` (n = 2·rgb − 1) reads exactly that vector.
fn bake_normal_map(tex: &CornellTextureSource) -> (u32, u32, Vec<f32>) {
    bake(tex, |u, v| {
        let x = sample_engine(tex, u, v, 0) * 2.0 - 1.0;
        let y = sample_engine(tex, u, v, 1) * 2.0 - 1.0;
        let z = (1.0 - x * x - y * y).max(0.0).sqrt();
        [x * 0.5 + 0.5, y * 0.5 + 0.5, z * 0.5 + 0.5]
    })
}

/// Binary coverage as the shadow ray tests it (`rayHitHasCoverage`).
fn bake_opacity(tex: &CornellTextureSource, cutout: &ExportCutout) -> (u32, u32, Vec<f32>) {
    bake(tex, |u, v| {
        let alpha = sample_engine(tex, u, v, 3) * cutout.material_alpha;
        let pass = alpha_compare_pass(alpha, cutout.threshold, cutout.func);
        [f32::from(u8::from(pass)); 3]
    })
}

/// `alphaComparePass` (`include/ray_hit.glsl`).
fn alpha_compare_pass(alpha: f32, threshold: f32, func: u8) -> bool {
    if threshold <= 0.0 || func == 0 {
        return true;
    }
    match func {
        1 => alpha < threshold,
        2 => (alpha - threshold).abs() < 1.0 / 255.0,
        3 => alpha <= threshold,
        4 => alpha > threshold,
        5 => (alpha - threshold).abs() >= 1.0 / 255.0,
        6 => alpha >= threshold,
        _ => false,
    }
}

/// Write `textures/<hash>.exr` (32-bit float RGB, M51 PTR-OUT-001's writer),
/// named by a hash of its texels so identical bakes share a file (PTX-002).
fn write_texture(dir: &Path, width: u32, height: u32, rgb: &[f32]) -> Result<String, String> {
    let bytes: Vec<u8> = rgb.iter().flat_map(|v| v.to_le_bytes()).collect();
    let file = format!("textures/{:016x}.exr", fnv1a64(&bytes));
    byroredux_renderer::vulkan::context::write_linear_exr(&dir.join(&file), width, height, rgb)?;
    Ok(file)
}

/// Shortest round-trip float text; `-0` is written as `0` so a sign-only
/// difference cannot make two identical scenes diff.
fn num(v: f32) -> String {
    if v == 0.0 {
        "0".to_owned()
    } else {
        format!("{v}")
    }
}

fn rgb(v: [f32; 3]) -> String {
    format!("{}, {}, {}", num(v[0]), num(v[1]), num(v[2]))
}

fn to_metres(v: Vec3) -> Vec3 {
    v / BETHESDA_UNITS_PER_METER
}

pub(crate) fn scene_xml(
    scene: &ExportScene,
    mesh_files: &BTreeMap<u32, String>,
    texture_files: &BTreeMap<String, ShapeTextureFiles>,
) -> String {
    let mut out = String::new();
    let c = &scene.camera;
    let origin = to_metres(c.origin);
    let target = origin + c.forward;
    let _ = writeln!(out, r#"<scene version="3.0.0">"#);
    let _ = writeln!(
        out,
        "    <!-- ByroRedux Cornell export (PTR-VAL-004). Units: metres (world units / {}). -->",
        num(BETHESDA_UNITS_PER_METER)
    );
    let _ = writeln!(out, r#"    <default name="spp" value="64"/>"#);
    let _ = writeln!(out, r#"    <default name="max_depth" value="-1"/>"#);
    let _ = writeln!(out, r#"    <integrator type="path">"#);
    let _ = writeln!(out, r#"        <integer name="max_depth" value="$max_depth"/>"#);
    let _ = writeln!(out, r#"        <boolean name="hide_emitters" value="true"/>"#);
    let _ = writeln!(out, "    </integrator>");
    let _ = writeln!(out, r#"    <sensor type="perspective">"#);
    let _ = writeln!(out, r#"        <string name="fov_axis" value="y"/>"#);
    let _ = writeln!(out, r#"        <float name="fov" value="{}"/>"#, num(c.fov_y_radians.to_degrees()));
    let _ = writeln!(out, r#"        <transform name="to_world">"#);
    let _ = writeln!(
        out,
        r#"            <lookat origin="{}" target="{}" up="{}"/>"#,
        rgb(origin.to_array()),
        rgb(target.to_array()),
        rgb(c.up.to_array())
    );
    let _ = writeln!(out, "        </transform>");
    let _ = writeln!(out, r#"        <sampler type="independent">"#);
    let _ = writeln!(out, r#"            <integer name="sample_count" value="$spp"/>"#);
    let _ = writeln!(out, "        </sampler>");
    let _ = writeln!(out, r#"        <film type="hdrfilm">"#);
    let _ = writeln!(out, r#"            <integer name="width" value="{}"/>"#, c.width);
    let _ = writeln!(out, r#"            <integer name="height" value="{}"/>"#, c.height);
    let _ = writeln!(out, r#"            <string name="pixel_format" value="rgb"/>"#);
    let _ = writeln!(out, r#"            <string name="component_format" value="float32"/>"#);
    let _ = writeln!(out, r#"            <rfilter type="box"/>"#);
    let _ = writeln!(out, "        </film>");
    let _ = writeln!(out, "    </sensor>");

    for shape in &scene.shapes {
        let file = &mesh_files[&shape.mesh];
        // Metres: scale the whole world-units matrix down, translation included.
        let m = Mat4::from_scale(Vec3::splat(1.0 / BETHESDA_UNITS_PER_METER)) * shape.to_world;
        let rows: Vec<String> = (0..4)
            .map(|r| {
                let row = m.row(r);
                format!("{} {} {} {}", num(row.x), num(row.y), num(row.z), num(row.w))
            })
            .collect();
        let _ = writeln!(out, r#"    <shape type="ply" id="{}">"#, xml_escape(&shape.name));
        let _ = writeln!(out, r#"        <string name="filename" value="{file}"/>"#);
        let _ = writeln!(out, r#"        <boolean name="face_normals" value="false"/>"#);
        let _ = writeln!(out, r#"        <transform name="to_world">"#);
        let _ = writeln!(out, r#"            <matrix value="{}"/>"#, rows.join("  "));
        let _ = writeln!(out, "        </transform>");
        // RLT-EXP-002: `mask` (the alpha test's binary coverage, nearest)
        // around `normalmap` (the engine-decoded normals) around the lobe.
        let files = texture_files.get(&shape.name).cloned().unwrap_or_default();
        let bitmap = |out: &mut String, name: &str, file: &str, filter: &str| {
            let _ = writeln!(out, r#"            <texture type="bitmap" name="{name}">"#);
            let _ = writeln!(out, r#"                <string name="filename" value="{file}"/>"#);
            let _ = writeln!(out, r#"                <boolean name="raw" value="true"/>"#);
            let _ = writeln!(out, r#"                <string name="filter_type" value="{filter}"/>"#);
            let _ = writeln!(out, "            </texture>");
        };
        if let Some(file) = &files.opacity {
            let _ = writeln!(out, r#"        <bsdf type="mask">"#);
            bitmap(&mut out, "opacity", file, "nearest");
        }
        if let Some(file) = &files.normal_map {
            let _ = writeln!(out, r#"        <bsdf type="normalmap">"#);
            bitmap(&mut out, "normalmap", file, "bilinear");
        }
        match &shape.bsdf {
            ExportBsdf::Diffuse { reflectance } => {
                let _ = writeln!(out, r#"        <bsdf type="diffuse">"#);
                let _ = writeln!(out, r#"            <rgb name="reflectance" value="{}"/>"#, rgb(*reflectance));
            }
            ExportBsdf::Principled {
                base_color,
                metallic,
                roughness,
            } => {
                let _ = writeln!(out, r#"        <bsdf type="principled">"#);
                let _ = writeln!(out, r#"            <rgb name="base_color" value="{}"/>"#, rgb(*base_color));
                let _ = writeln!(out, r#"            <float name="metallic" value="{}"/>"#, num(*metallic));
                let _ = writeln!(out, r#"            <float name="roughness" value="{}"/>"#, num(*roughness));
                let _ = writeln!(out, r#"            <float name="specular" value="0.5"/>"#);
            }
        }
        let _ = writeln!(out, "        </bsdf>");
        for _ in files.normal_map.iter().chain(&files.opacity) {
            let _ = writeln!(out, "        </bsdf>");
        }
        let _ = writeln!(out, "    </shape>");
    }

    for emitter in &scene.emitters {
        let r = emitter.source_radius_m.max(1e-4);
        // Sphere of radius r with radiance k·I / (π r²): a facing receiver at
        // distance d gets irradiance k·I / d² (design §2.1, RLT-REF-002).
        let radiance = emitter
            .intensity
            .map(|i| K_EXP_POINT * i / (std::f32::consts::PI * r * r));
        let _ = writeln!(
            out,
            "    <!-- {}: engine intensity {} W/sr, k_exp {} (point), radius {} m -->",
            xml_escape(&emitter.name),
            rgb(emitter.intensity),
            num(K_EXP_POINT),
            num(r)
        );
        let _ = writeln!(out, r#"    <shape type="sphere" id="{}">"#, xml_escape(&emitter.name));
        let _ = writeln!(out, r#"        <point name="center" value="{}"/>"#, rgb(to_metres(emitter.position).to_array()));
        let _ = writeln!(out, r#"        <float name="radius" value="{}"/>"#, num(r));
        let _ = writeln!(out, r#"        <bsdf type="diffuse"><rgb name="reflectance" value="0, 0, 0"/></bsdf>"#);
        let _ = writeln!(out, r#"        <emitter type="area">"#);
        let _ = writeln!(out, r#"            <rgb name="radiance" value="{}"/>"#, rgb(radiance));
        let _ = writeln!(out, "        </emitter>");
        let _ = writeln!(out, "    </shape>");
    }
    if let Some(sun) = &scene.sun {
        // A sphere of radius D·sin θ at distance D subtends exactly θ; its
        // radiance L gives a facing receiver π·L·sin²θ, which must equal
        // k_exp · E (design §2.1, RLT-EXP-004).
        let sin_t = sun.angular_radius.sin().max(1e-6);
        let radius = SUN_DISTANCE_M * sin_t;
        let radiance = sun
            .irradiance
            .map(|e| K_EXP_SUN * e / (std::f32::consts::PI * sin_t * sin_t));
        let center = sun.direction * SUN_DISTANCE_M;
        let _ = writeln!(
            out,
            "    <!-- sun: engine irradiance {} , k_exp {} (sun), angular radius {} rad -->",
            rgb(sun.irradiance),
            num(K_EXP_SUN),
            num(sun.angular_radius)
        );
        let _ = writeln!(out, r#"    <shape type="sphere" id="sun">"#);
        let _ = writeln!(out, r#"        <point name="center" value="{}"/>"#, rgb(center.to_array()));
        let _ = writeln!(out, r#"        <float name="radius" value="{}"/>"#, num(radius));
        let _ = writeln!(out, r#"        <bsdf type="diffuse"><rgb name="reflectance" value="0, 0, 0"/></bsdf>"#);
        let _ = writeln!(out, r#"        <emitter type="area">"#);
        let _ = writeln!(out, r#"            <rgb name="radiance" value="{}"/>"#, rgb(radiance));
        let _ = writeln!(out, "        </emitter>");
        let _ = writeln!(out, "    </shape>");
    }
    let _ = writeln!(out, "</scene>");
    out
}

fn export_tsv(scene: &ExportScene) -> String {
    let mut out = String::from("emitter\tkind\tk_exp\tintensity_engine\tradius_m\n");
    if let Some(sun) = &scene.sun {
        let _ = writeln!(
            out,
            "sun\tdirectional\t{}\t{}\t{} rad",
            num(K_EXP_SUN),
            rgb(sun.irradiance),
            num(sun.angular_radius)
        );
    }
    for e in &scene.emitters {
        let _ = writeln!(
            out,
            "{}\tpoint\t{}\t{}\t{}",
            e.name,
            num(K_EXP_POINT),
            rgb(e.intensity),
            num(e.source_radius_m)
        );
    }
    out
}

/// `rung.tsv`: the rung's measurement geometry, world units, one record per
/// line with its kind first.
fn rung_tsv(m: &super::reference::RungMeasurements) -> String {
    let mut out = String::from("# rung measurements (scripts/m-light/gates.py); world units\n");
    if !m.step_form.is_empty() {
        let _ = writeln!(out, "step_form\t{}", m.step_form);
    }
    let _ = writeln!(out, "min_unshadowed\t{}", num(m.min_unshadowed));
    for caster in &m.casters {
        let _ = writeln!(out, "caster\t{caster}");
    }
    let v = |p: Vec3| format!("{}\t{}\t{}", num(p.x), num(p.y), num(p.z));
    for line in &m.profile_lines {
        let _ = writeln!(
            out,
            "profile\t{}\t{}\t{}\t{}\t{}",
            line.name,
            line.receiver,
            v(line.start),
            v(line.end),
            num(line.half_width)
        );
    }
    for band in &m.bands {
        let _ = writeln!(
            out,
            "band\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            band.name,
            band.receiver,
            num(band.centre.x),
            num(band.centre.z),
            num(band.half_extents[0]),
            num(band.half_extents[1]),
            num(band.yaw),
            num(band.width)
        );
    }
    for region in &m.regions {
        let _ = writeln!(
            out,
            "region\t{}\t{}\t{}\t{}",
            region.name,
            region.receiver,
            v(region.min),
            v(region.max)
        );
    }
    out
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Binary little-endian PLY with positions, normals and triangle faces.
pub(crate) fn ply_bytes(source: &CornellMeshSource, with_uv: bool) -> Vec<u8> {
    let faces = source.indices.len() / 3;
    let uv_header = if with_uv { "property float u\nproperty float v\n" } else { "" };
    let header = format!(
        "ply\nformat binary_little_endian 1.0\nelement vertex {}\n\
         property float x\nproperty float y\nproperty float z\n\
         property float nx\nproperty float ny\nproperty float nz\n{uv_header}\
         element face {faces}\nproperty list uchar uint vertex_indices\nend_header\n",
        source.positions.len()
    );
    let mut out = header.into_bytes();
    for (i, (p, n)) in source.positions.iter().zip(&source.normals).enumerate() {
        for v in p.iter().chain(n) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        if with_uv {
            for v in source.uvs.get(i).copied().unwrap_or([0.0; 2]) {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
    }
    for tri in source.indices.chunks_exact(3) {
        out.push(3);
        for i in tri {
            out.extend_from_slice(&i.to_le_bytes());
        }
    }
    out
}

/// FNV-1a 64: a stable content hash for PLY file names. Not cryptographic;
/// the reference cache key (PTX-008) hashes the files themselves.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::math::Quat;

    fn quad() -> CornellMeshSource {
        CornellMeshSource {
            positions: vec![[-1.0, 0.0, -1.0], [1.0, 0.0, -1.0], [1.0, 0.0, 1.0], [-1.0, 0.0, 1.0]],
            normals: vec![[0.0, 1.0, 0.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            indices: vec![0, 2, 1, 0, 3, 2],
        }
    }

    fn scene() -> ExportScene {
        ExportScene {
            shapes: vec![ExportShape {
                name: "floor".into(),
                mesh: 7,
                to_world: Mat4::from_scale_rotation_translation(
                    Vec3::ONE,
                    Quat::IDENTITY,
                    Vec3::new(0.0, 0.0, -70.0),
                ),
                bsdf: ExportBsdf::Diffuse {
                    reflectance: [0.8, 0.8, 0.8],
                },
                normal_map: None,
                cutout: None,
            }],
            emitters: vec![ExportEmitter {
                name: "key".into(),
                position: Vec3::new(0.0, 105.0, 0.0),
                intensity: [2.0, 2.0, 2.0],
                source_radius_m: 0.02,
            }],
            sun: None,
            camera: ExportCamera {
                origin: Vec3::new(0.0, 70.0, 140.0),
                forward: Vec3::NEG_Z,
                up: Vec3::Y,
                fov_y_radians: 1.0,
                width: 64,
                height: 32,
            },
            measurements: Default::default(),
        }
    }

    #[test]
    fn rung_tsv_lists_casters_profiles_and_regions() {
        use super::super::reference::{MeasuredRegion, ProfileLine, RungMeasurements};
        let tsv = rung_tsv(&RungMeasurements {
            profile_lines: vec![ProfileLine {
                name: "edge".into(),
                receiver: "floor".into(),
                start: Vec3::new(1.0, 0.0, 0.0),
                end: Vec3::new(2.0, 0.0, 0.0),
                half_width: 0.5,
            }],
            regions: vec![MeasuredRegion {
                name: "rod".into(),
                receiver: "floor".into(),
                min: Vec3::splat(-1.0),
                max: Vec3::splat(1.0),
            }],
            bands: Vec::new(),
            casters: vec!["plate".into()],
            step_form: "disk_segment",
            min_unshadowed: 0.05,
        });
        assert!(tsv.contains("step_form\tdisk_segment\n"), "{tsv}");
        assert!(tsv.contains("caster\tplate\n"), "{tsv}");
        assert!(tsv.contains("profile\tedge\tfloor\t1\t0\t0\t2\t0\t0\t0.5\n"), "{tsv}");
        assert!(tsv.contains("region\trod\tfloor\t-1\t-1\t-1\t1\t1\t1\n"), "{tsv}");
    }

    #[test]
    fn xml_is_deterministic_and_in_metres() {
        let files: BTreeMap<u32, String> = [(7, "meshes/x.ply".to_string())].into();
        let a = scene_xml(&scene(), &files, &BTreeMap::new());
        assert_eq!(a, scene_xml(&scene(), &files, &BTreeMap::new()), "export must be byte-stable");
        // Camera origin (0,70,140) wu → (0,1,2) m; the shape's −70 wu z → −1 m.
        assert!(a.contains(r#"origin="0, 1, 2""#), "{a}");
        // PLY vertices stay in world units, so the 1/70 scale is folded into
        // the whole matrix, translation included.
        assert!(
            a.contains("0.014285714 0 0 0  0 0.014285714 0 0  0 0 0.014285714 -1  0 0 0 1"),
            "{a}"
        );
        assert!(a.contains(r#"<float name="fov" value="57.29578"/>"#), "{a}");
    }

    #[test]
    fn point_emitter_is_a_sphere_scaled_by_k_exp() {
        let a = scene_xml(&scene(), &[(7, "m.ply".to_string())].into(), &BTreeMap::new());
        // radiance = π · 2 / (π · 0.02²) = 5000
        assert!(a.contains(r#"<rgb name="radiance" value="5000, 5000, 5000"/>"#), "{a}");
        assert!(a.contains(r#"<float name="radius" value="0.02"/>"#), "{a}");
        let tsv = export_tsv(&scene());
        assert!(tsv.contains("key\tpoint\t3.1415927\t2, 2, 2\t0.02"), "{tsv}");
    }

    #[test]
    fn sun_is_a_distant_sphere_subtending_its_angular_radius() {
        let mut sc = scene();
        sc.sun = Some(ExportSun {
            direction: Vec3::Y,
            irradiance: [1.0; 3],
            angular_radius: 0.02,
        });
        let xml = scene_xml(&sc, &[(7, "m.ply".to_string())].into(), &BTreeMap::new());
        let sin_t = 0.02f32.sin();
        assert!(xml.contains(&format!(r#"<float name="radius" value="{}"/>"#, SUN_DISTANCE_M * sin_t)), "{xml}");
        // pi * L * sin^2 = k_exp * E  =>  L = E / sin^2 for k_exp = pi.
        let radiance = K_EXP_SUN / (std::f32::consts::PI * sin_t * sin_t);
        assert!(xml.contains(&format!("value=\"{}, {}, {}\"", radiance, radiance, radiance)), "{xml}");
        assert!(export_tsv(&sc).contains("sun\tdirectional"));
    }

    #[test]
    fn ply_header_and_payload_sizes_agree() {
        for (with_uv, floats) in [(false, 6), (true, 8)] {
            let bytes = ply_bytes(&quad(), with_uv);
            let text_end = bytes.windows(11).position(|w| w == b"end_header\n").unwrap() + 11;
            let header = std::str::from_utf8(&bytes[..text_end]).unwrap();
            assert!(header.contains("element vertex 4") && header.contains("element face 2"));
            assert_eq!(header.contains("property float u\n"), with_uv);
            assert_eq!(bytes.len() - text_end, 4 * floats * 4 + 2 * (1 + 3 * 4));
        }
    }

    fn texture(width: u32, height: u32, texel: impl Fn(u32, u32) -> [u8; 4], srgb: bool) -> CornellTextureSource {
        let mut rgba = Vec::new();
        for y in 0..height {
            for x in 0..width {
                rgba.extend_from_slice(&texel(x, y));
            }
        }
        CornellTextureSource { width, height, rgba, srgb }
    }

    #[test]
    fn engine_sampling_hits_texel_centres_and_wraps() {
        let tex = texture(2, 1, |x, _| if x == 0 { [0, 0, 0, 0] } else { [255; 4] }, false);
        assert_eq!(sample_engine(&tex, 0.25, 0.5, 3), 0.0);
        assert_eq!(sample_engine(&tex, 0.75, 0.5, 3), 1.0);
        assert!((sample_engine(&tex, 0.5, 0.5, 3) - 0.5).abs() < 1e-6);
        // Repeat addressing: u = 0 is halfway between texel 1 (wrapped) and 0.
        assert!((sample_engine(&tex, 0.0, 0.5, 3) - 0.5).abs() < 1e-6);
        // sRGB decodes colour, never alpha.
        let srgb = texture(1, 1, |_, _| [128, 128, 128, 128], true);
        assert!((sample_engine(&srgb, 0.5, 0.5, 0) - srgb_to_linear(128.0 / 255.0)).abs() < 1e-6);
        assert!((sample_engine(&srgb, 0.5, 0.5, 3) - 128.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn baked_normal_map_reads_back_as_the_engine_decode() {
        // A flat +Z texel in a linear texture decodes to (0, 0, 1) on both sides.
        let tex = texture(2, 2, |_, _| [128, 128, 255, 255], false);
        let (w, h, rgb) = bake_normal_map(&tex);
        assert_eq!((w, h), (2 * TEXTURE_BAKE_SCALE, 2 * TEXTURE_BAKE_SCALE));
        let n: Vec<f32> = rgb[..3].iter().map(|c| c * 2.0 - 1.0).collect();
        let x = 128.0 / 255.0 * 2.0 - 1.0;
        assert!((n[0] - x).abs() < 1e-6 && (n[1] - x).abs() < 1e-6, "{n:?}");
        assert!((n[2] - (1.0 - 2.0 * x * x).sqrt()).abs() < 1e-6, "z is rebuilt, blue ignored: {n:?}");
    }

    #[test]
    fn baked_opacity_is_the_alpha_test_coverage() {
        let tex = texture(2, 1, |x, _| [255, 255, 255, if x == 0 { 0 } else { 255 }], false);
        let cutout = ExportCutout { texture: 1, threshold: 0.5, func: 6, material_alpha: 1.0 };
        let (w, _, rgb) = bake_opacity(&tex, &cutout);
        let row: Vec<f32> = rgb.chunks(3).take(w as usize).map(|c| c[0]).collect();
        // Texel 0 (u < 0.5) is a hole, texel 1 solid; the >= 0.5 edges fall
        // midway between the texel centres, at u = 0 and u = 0.5.
        assert_eq!(row[w as usize / 4], 0.0);
        assert_eq!(row[3 * w as usize / 4], 1.0);
        assert!(!alpha_compare_pass(0.2, 0.5, 6) && alpha_compare_pass(0.2, 0.0, 6));
    }

    #[test]
    fn textured_shapes_wrap_the_lobe_in_mask_and_normalmap() {
        let files = ShapeTextureFiles {
            normal_map: Some("textures/n.exr".into()),
            opacity: Some("textures/o.exr".into()),
        };
        let xml = scene_xml(
            &scene(),
            &[(7, "m.ply".to_string())].into(),
            &[("floor".to_string(), files)].into(),
        );
        let mask = xml.find(r#"<bsdf type="mask">"#).expect("mask");
        let normal = xml.find(r#"<bsdf type="normalmap">"#).expect("normalmap");
        let diffuse = xml.find(r#"<bsdf type="diffuse">"#).expect("diffuse");
        assert!(mask < normal && normal < diffuse, "{xml}");
        assert_eq!(xml.matches("<bsdf ").count(), xml.matches("</bsdf>").count(), "{xml}");
    }

    #[test]
    fn export_refuses_a_mesh_without_a_recorded_source() {
        let dir = std::env::temp_dir().join(format!("byro-mi-{}", std::process::id()));
        let textures = CornellTextureSources::default();
        let err = export_mitsuba(&scene(), &CornellMeshSources::default(), &textures, &dir).unwrap_err();
        assert!(err.contains("no recorded CPU source"), "{err}");
        let mut sources = CornellMeshSources::default();
        sources.0.insert(7, quad());
        let report = export_mitsuba(&scene(), &sources, &textures, &dir).unwrap();
        assert_eq!((report.shapes, report.emitters), (1, 1));
        let first = std::fs::read(dir.join("scene.xml")).unwrap();
        export_mitsuba(&scene(), &sources, &textures, &dir).unwrap();
        assert_eq!(first, std::fs::read(dir.join("scene.xml")).unwrap());
        let _ = std::fs::remove_dir_all(dir);
    }
}
