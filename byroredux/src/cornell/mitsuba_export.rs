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

/// CPU copy of one uploaded Cornell mesh (PTX-003).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CornellMeshSource {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
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
    pub camera: ExportCamera,
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
            shapes.push(ExportShape {
                bsdf: export_bsdf(&name, material)?,
                name,
                mesh: mesh.0,
                to_world: transform.to_matrix(),
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
    if emitters.is_empty() {
        return Err("a reference rung needs at least one emitter".into());
    }
    Ok(ExportScene {
        shapes,
        emitters,
        camera,
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

/// Write `DIR/scene.xml`, `DIR/meshes/*.ply` and `DIR/export.tsv`.
pub(crate) fn export_mitsuba(
    scene: &ExportScene,
    sources: &CornellMeshSources,
    dir: &Path,
) -> Result<ExportReport, String> {
    let meshes_dir = dir.join("meshes");
    std::fs::create_dir_all(&meshes_dir).map_err(|e| format!("create {}: {e}", meshes_dir.display()))?;

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
        let bytes = ply_bytes(source);
        let file = format!("meshes/{:016x}.ply", fnv1a64(&bytes));
        std::fs::write(dir.join(&file), &bytes).map_err(|e| format!("write {file}: {e}"))?;
        mesh_files.insert(shape.mesh, file);
    }

    let xml = scene_xml(scene, &mesh_files);
    let scene_xml = dir.join("scene.xml");
    std::fs::write(&scene_xml, xml).map_err(|e| format!("write {}: {e}", scene_xml.display()))?;
    std::fs::write(dir.join("export.tsv"), export_tsv(scene))
        .map_err(|e| format!("write export.tsv: {e}"))?;
    Ok(ExportReport {
        scene_xml,
        shapes: scene.shapes.len(),
        emitters: scene.emitters.len(),
    })
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

pub(crate) fn scene_xml(scene: &ExportScene, mesh_files: &BTreeMap<u32, String>) -> String {
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
    let _ = writeln!(out, "</scene>");
    out
}

fn export_tsv(scene: &ExportScene) -> String {
    let mut out = String::from("emitter\tkind\tk_exp\tintensity_engine\tradius_m\n");
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

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Binary little-endian PLY with positions, normals and triangle faces.
pub(crate) fn ply_bytes(source: &CornellMeshSource) -> Vec<u8> {
    let faces = source.indices.len() / 3;
    let header = format!(
        "ply\nformat binary_little_endian 1.0\nelement vertex {}\n\
         property float x\nproperty float y\nproperty float z\n\
         property float nx\nproperty float ny\nproperty float nz\n\
         element face {faces}\nproperty list uchar uint vertex_indices\nend_header\n",
        source.positions.len()
    );
    let mut out = header.into_bytes();
    for (p, n) in source.positions.iter().zip(&source.normals) {
        for v in p.iter().chain(n) {
            out.extend_from_slice(&v.to_le_bytes());
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
            }],
            emitters: vec![ExportEmitter {
                name: "key".into(),
                position: Vec3::new(0.0, 105.0, 0.0),
                intensity: [2.0, 2.0, 2.0],
                source_radius_m: 0.02,
            }],
            camera: ExportCamera {
                origin: Vec3::new(0.0, 70.0, 140.0),
                forward: Vec3::NEG_Z,
                up: Vec3::Y,
                fov_y_radians: 1.0,
                width: 64,
                height: 32,
            },
        }
    }

    #[test]
    fn xml_is_deterministic_and_in_metres() {
        let files: BTreeMap<u32, String> = [(7, "meshes/x.ply".to_string())].into();
        let a = scene_xml(&scene(), &files);
        assert_eq!(a, scene_xml(&scene(), &files), "export must be byte-stable");
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
        let a = scene_xml(&scene(), &[(7, "m.ply".to_string())].into());
        // radiance = π · 2 / (π · 0.02²) = 5000
        assert!(a.contains(r#"<rgb name="radiance" value="5000, 5000, 5000"/>"#), "{a}");
        assert!(a.contains(r#"<float name="radius" value="0.02"/>"#), "{a}");
        let tsv = export_tsv(&scene());
        assert!(tsv.contains("key\tpoint\t3.1415927\t2, 2, 2\t0.02"), "{tsv}");
    }

    #[test]
    fn ply_header_and_payload_sizes_agree() {
        let bytes = ply_bytes(&quad());
        let text_end = bytes.windows(11).position(|w| w == b"end_header\n").unwrap() + 11;
        let header = std::str::from_utf8(&bytes[..text_end]).unwrap();
        assert!(header.contains("element vertex 4") && header.contains("element face 2"));
        assert_eq!(bytes.len() - text_end, 4 * 6 * 4 + 2 * (1 + 3 * 4));
    }

    #[test]
    fn export_refuses_a_mesh_without_a_recorded_source() {
        let dir = std::env::temp_dir().join(format!("byro-mi-{}", std::process::id()));
        let err = export_mitsuba(&scene(), &CornellMeshSources::default(), &dir).unwrap_err();
        assert!(err.contains("no recorded CPU source"), "{err}");
        let mut sources = CornellMeshSources::default();
        sources.0.insert(7, quad());
        let report = export_mitsuba(&scene(), &sources, &dir).unwrap();
        assert_eq!((report.shapes, report.emitters), (1, 1));
        let first = std::fs::read(dir.join("scene.xml")).unwrap();
        export_mitsuba(&scene(), &sources, &dir).unwrap();
        assert_eq!(first, std::fs::read(dir.join("scene.xml")).unwrap());
        let _ = std::fs::remove_dir_all(dir);
    }
}
