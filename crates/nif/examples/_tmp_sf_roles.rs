//! Throwaway (SF audit 2026-08-12, texture-roles-deep): measure per-role
//! `MaterialTextureSet` fill rate on real Starfield content, split by
//! material-authoring path (`.mat` external reference vs inline NIF).
use byroredux_bsa::Ba2Archive;
use byroredux_core::string::StringPool;
use byroredux_nif::import::MeshResolver;

struct ChainResolver {
    archives: Vec<Ba2Archive>,
}
impl MeshResolver for ChainResolver {
    fn resolve(&self, mesh_name: &str) -> Option<Vec<u8>> {
        let candidates = [
            mesh_name.to_string(),
            mesh_name.replace('/', "\\"),
            mesh_name.replace('\\', "/"),
        ];
        self.archives
            .iter()
            .find_map(|a| candidates.iter().find_map(|c| a.extract(c).ok()))
    }
}

const ROLES: [&str; 18] = [
    "base_color",
    "normal",
    "emissive",
    "detail",
    "smooth_spec",
    "dark",
    "height",
    "environment",
    "environment_mask",
    "tint",
    "inner_layer",
    "specular",
    "lighting",
    "flow",
    "wrinkle",
    "greyscale_lut",
    "reflectance",
    "emittance_gradient",
];

fn main() {
    let mut limit = usize::MAX;
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(pos) = args.iter().position(|a| a == "--limit") {
        limit = args[pos + 1].parse().unwrap();
        args.drain(pos..pos + 2);
    }

    let mut n_meshes = 0u64;
    let mut n_mat = 0u64; // material_path ends .mat
    let mut n_bgsm = 0u64;
    let mut n_nopath = 0u64;
    // per-role counts, [all, mat-path, no-path]
    let mut fill_all = [0u64; 18];
    let mut fill_mat = [0u64; 18];
    let mut fill_nopath = [0u64; 18];
    let mut decals_all = 0u64;
    let mut mat_any_role = 0u64;
    let mut nopath_any_role = 0u64;

    let resolver = ChainResolver {
        archives: args
            .iter()
            .filter_map(|p| Ba2Archive::open(p).ok())
            .collect(),
    };

    for path in &args {
        let Ok(archive) = Ba2Archive::open(path) else {
            eprintln!("skip {path}");
            continue;
        };
        let names: Vec<String> = archive
            .list_files()
            .into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
            .map(|s| s.to_string())
            .take(limit)
            .collect();
        eprintln!("{path}: {} nifs", names.len());
        for name in names {
            let Ok(bytes) = archive.extract(&name) else {
                continue;
            };
            let Ok(scene) = byroredux_nif::parse_nif(&bytes) else {
                continue;
            };
            let mut pool = StringPool::new();
            let meshes =
                byroredux_nif::import::import_nif_with_resolver(&scene, &mut pool, Some(&resolver));
            for m in &meshes {
                n_meshes += 1;
                let mp = m
                    .material
                    .material_path
                    .and_then(|s| pool.resolve(s).map(|x| x.to_string()));
                let kind = match mp.as_deref() {
                    Some(p) if p.ends_with(".mat") => 1,
                    Some(_) => 2,
                    None => 0,
                };
                match kind {
                    1 => n_mat += 1,
                    2 => n_bgsm += 1,
                    _ => n_nopath += 1,
                }
                let t = &m.material.textures;
                let slots = [
                    &t.base_color,
                    &t.normal,
                    &t.emissive,
                    &t.detail,
                    &t.smooth_spec,
                    &t.dark,
                    &t.height,
                    &t.environment,
                    &t.environment_mask,
                    &t.tint,
                    &t.inner_layer,
                    &t.specular,
                    &t.lighting,
                    &t.flow,
                    &t.wrinkle,
                    &t.greyscale_lut,
                    &t.reflectance,
                    &t.emittance_gradient,
                ];
                let mut any = false;
                for (i, s) in slots.iter().enumerate() {
                    if s.is_some() {
                        fill_all[i] += 1;
                        any = true;
                        if kind == 1 {
                            fill_mat[i] += 1;
                        }
                        if kind == 0 {
                            fill_nopath[i] += 1;
                        }
                    }
                }
                if t.decals.iter().any(|d| d.is_some()) {
                    decals_all += 1;
                }
                if any && kind == 1 {
                    mat_any_role += 1;
                }
                if any && kind == 0 {
                    nopath_any_role += 1;
                }
            }
        }
    }

    println!("meshes={n_meshes} mat={n_mat} bgsm_bgem={n_bgsm} nopath={n_nopath}");
    println!("mat meshes with ANY role filled: {mat_any_role}");
    println!("nopath meshes with ANY role filled: {nopath_any_role}");
    println!("decals any: {decals_all}");
    println!(
        "{:<20} {:>10} {:>10} {:>10}",
        "role", "all", "mat", "nopath"
    );
    for i in 0..18 {
        println!(
            "{:<20} {:>10} {:>10} {:>10}",
            ROLES[i], fill_all[i], fill_mat[i], fill_nopath[i]
        );
    }
}
