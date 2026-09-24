//! Survey the geometry and material contract of shipped baked beam meshes.
//!
//! `cargo run -p byroredux-nif --example beam_geometry_census -- <meshes.bsa|meshes.ba2> [name-filter] [--vertices]`

use byroredux_bsa::{Ba2Archive, BsaArchive};

enum MeshArchive {
    Bsa(BsaArchive),
    Ba2(Ba2Archive),
}

impl MeshArchive {
    fn open(path: &str) -> std::io::Result<Self> {
        if path.to_ascii_lowercase().ends_with(".ba2") {
            Ba2Archive::open(path).map(Self::Ba2)
        } else {
            BsaArchive::open(path).map(Self::Bsa)
        }
    }

    fn list_files(&self) -> Vec<&str> {
        match self {
            Self::Bsa(archive) => archive.list_files(),
            Self::Ba2(archive) => archive.list_files(),
        }
    }

    fn extract(&self, path: &str) -> std::io::Result<Vec<u8>> {
        match self {
            Self::Bsa(archive) => archive.extract(path),
            Self::Ba2(archive) => archive.extract(path),
        }
    }
}

fn main() {
    let archive_path = std::env::args()
        .nth(1)
        .expect("usage: beam_geometry_census <meshes.bsa|meshes.ba2> [name-filter] [--vertices]");
    let options: Vec<String> = std::env::args().skip(2).collect();
    let detail_filter = options
        .iter()
        .find(|option| !option.starts_with("--"))
        .map(|option| option.to_ascii_lowercase());
    let show_vertices = options.iter().any(|option| option == "--vertices");
    let show_indices = options.iter().any(|option| option == "--indices");
    let archive = MeshArchive::open(&archive_path).expect("open archive");
    let mut pool = byroredux_core::string::StringPool::new();
    for path in archive.list_files() {
        let lower = path.to_ascii_lowercase();
        if !lower.ends_with(".nif") || !(lower.contains("lightbeam") || lower.contains("godray")) {
            continue;
        }
        if detail_filter
            .as_ref()
            .is_some_and(|filter| !lower.contains(filter))
        {
            continue;
        }
        let Ok(bytes) = archive.extract(path) else {
            continue;
        };
        let Ok(scene) = byroredux_nif::parse_nif(&bytes) else {
            continue;
        };
        let imported = byroredux_nif::import::import_nif_scene(&scene, &mut pool);
        let meshes = imported.meshes;
        if meshes.is_empty() {
            println!(
                "{path} | no imported mesh; {} particle emitters, {} nodes",
                imported.particle_emitters.len(),
                imported.nodes.len()
            );
            for emitter in &imported.particle_emitters {
                let host = emitter
                    .parent_node
                    .and_then(|index| imported.nodes.get(index))
                    .and_then(|node| node.name.as_deref());
                println!(
                    "  particle host={host:?} type={} texture={:?} blend={:?}/{:?} colors={:?} rate={:?} params={:?}",
                    emitter.original_type,
                    emitter.texture_path,
                    emitter.src_blend,
                    emitter.dst_blend,
                    emitter.color_curve,
                    emitter.emitter_rate,
                    emitter.emitter_params
                );
            }
        }
        for mesh in meshes {
            let mut min = [f32::INFINITY; 3];
            let mut max = [f32::NEG_INFINITY; 3];
            let mut peak_alpha: f32 = 0.0;
            for (index, position) in mesh.positions.iter().enumerate() {
                for axis in 0..3 {
                    min[axis] = min[axis].min(position[axis]);
                    max[axis] = max[axis].max(position[axis]);
                }
                peak_alpha = peak_alpha.max(mesh.colors.get(index).map_or(1.0, |color| color[3]));
            }
            let center_x = (min[0] + max[0]) * 0.5;
            let center_z = (min[2] + max[2]) * 0.5;
            let end_band = ((max[1] - min[1]) * 0.02).max(1.0);
            let (mut low_radius, mut high_radius) = (0.0f32, 0.0f32);
            for point in &mesh.positions {
                let radial = (point[0] - center_x).hypot(point[2] - center_z);
                if point[1] - min[1] <= end_band {
                    low_radius = low_radius.max(radial);
                }
                if max[1] - point[1] <= end_band {
                    high_radius = high_radius.max(radial);
                }
            }
            println!(
                "{path} | {} verts {} tri | kind={} alpha={} mat_alpha={:.3} peak={peak_alpha:.3} | \
                 x={:.0}..{:.0} y={:.0}..{:.0} z={:.0}..{:.0} r(low/high)={low_radius:.0}/{high_radius:.0} \
                 diffuse={:?} texture={:?} local_trs={:?}/{:?}/{:.3}",
                mesh.positions.len(),
                mesh.indices.len() / 3,
                mesh.material.material_kind,
                mesh.material.has_alpha,
                mesh.material.mat_alpha,
                min[0],
                max[0],
                min[1],
                max[1],
                min[2],
                max[2],
                mesh.material.diffuse_color,
                mesh.material
                    .textures
                    .base_color
                    .and_then(|texture| pool.resolve(texture)),
                mesh.translation,
                mesh.rotation,
                mesh.scale,
            );
            if show_vertices {
                for (index, point) in mesh.positions.iter().enumerate() {
                    println!("  {index:>2}: {point:?} {:?}", mesh.colors.get(index));
                }
            }
            if show_indices {
                for (index, triangle) in mesh.indices.chunks_exact(3).enumerate() {
                    println!("  tri {index:>2}: {triangle:?}");
                }
            }
        }
    }
}
