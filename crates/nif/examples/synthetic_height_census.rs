//! Census for synthetic parallax height on static meshes.
//!
//! Question: how much shipped static-mesh content has a normal map, no
//! authored height/parallax map, and tiling UVs — the population a
//! normal-integrated synthetic height map (the `byroredux-terrain-height`
//! generator) could displace — versus content whose UVs are a unique atlas,
//! where normal integration is not trustworthy.
//!
//! Per imported mesh (rigid only; skinned, decal, alpha-blended and
//! normal-less meshes are tallied separately) it records:
//!   * whether the material carries a `height` role (authored parallax),
//!   * the UV extent (max over axes of max-min), bucketed — extent > 1 means
//!     the mesh tiles its texture, the signal for "tileable material",
//!   * the mesh's world surface area (sum of triangle areas × scale²),
//!     a proxy for how much visible surface the class covers,
//!   * the normal map's directory class.
//!
//! Counts are per *NIF file mesh*, not per placed reference: a wall kit
//! piece placed 500 times weighs the same as a one-off. Treat the numbers
//! as population shape, not screen share.
//!
//! Oblivion/FO3/FNV bind most normal maps by the `<base>_n.dds` filename
//! convention, not a NIF slot (`derive_present_normal_map_path`), so pass the
//! game's texture archives after `--tex` and a mesh with a base texture but
//! no normal role counts as normal-mapped when that sibling is present.
//!
//! `--emit-normals <file>` writes one TSV row per distinct candidate normal map
//! (`key  meshes  area_m2  max_uv_extent  sample_mesh`) for the
//! `terrain-height` `tileability_census` example to score.
//!
//! Usage:
//!   cargo run --release -p byroredux-nif --example synthetic_height_census -- \
//!       <mesh archive>... [--tex <texture archive>...] [--emit-normals <file>]
//! BA2 (FO4+) archives are skipped: their textures come from BGSM files that
//! `import_nif` alone does not resolve, so the roles would read empty.

use std::collections::{BTreeMap, HashSet};

#[derive(Default)]
struct Bucket {
    meshes: usize,
    area: f64,
}

impl Bucket {
    fn add(&mut self, area: f64) {
        self.meshes += 1;
        // Bethesda units² → m² (≈70 units/m); non-finite meshes add nothing.
        if area.is_finite() {
            self.area += area / 4900.0;
        }
    }
}

fn uv_extent(uvs: &[[f32; 2]]) -> f32 {
    if uvs.is_empty() {
        return 0.0;
    }
    let (mut min, mut max) = ([f32::MAX; 2], [f32::MIN; 2]);
    for uv in uvs {
        for k in 0..2 {
            min[k] = min[k].min(uv[k]);
            max[k] = max[k].max(uv[k]);
        }
    }
    (max[0] - min[0]).max(max[1] - min[1])
}

fn surface_area(positions: &[[f32; 3]], indices: &[u32], scale: f32) -> f64 {
    let mut a = 0.0f64;
    for t in indices.chunks_exact(3) {
        let (p0, p1, p2) = (
            positions.get(t[0] as usize),
            positions.get(t[1] as usize),
            positions.get(t[2] as usize),
        );
        if let (Some(p0), Some(p1), Some(p2)) = (p0, p1, p2) {
            let u = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let v = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let c = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            a += 0.5 * ((c[0] * c[0] + c[1] * c[1] + c[2] * c[2]) as f64).sqrt();
        }
    }
    a * (scale as f64) * (scale as f64)
}

fn extent_label(e: f32) -> &'static str {
    if e <= 1.02 {
        "uv<=1 (atlas/unique)"
    } else if e <= 2.0 {
        "uv 1-2"
    } else if e <= 4.0 {
        "uv 2-4"
    } else {
        "uv>4"
    }
}

/// Bethesda units: ~70 per metre. Bound radius, scaled.
fn size_label(r: f32) -> &'static str {
    if r < 35.0 {
        "<0.5m radius (clutter)"
    } else if r < 140.0 {
        "0.5-2m"
    } else if r < 560.0 {
        "2-8m (walls/rocks)"
    } else {
        ">8m (large)"
    }
}

fn dir_class(path: &str) -> String {
    let p = path.replace('\\', "/").to_ascii_lowercase();
    let p = p.strip_prefix("data/").unwrap_or(&p).to_string();
    let mut it = p.split('/');
    let first = it.next().unwrap_or("");
    let second = it.next().unwrap_or("");
    if first == "textures" {
        format!("textures/{second}")
    } else {
        first.to_string()
    }
}

fn main() {
    let mut archives: Vec<String> = Vec::new();
    let mut tex_archives: Vec<String> = Vec::new();
    let mut in_tex = false;
    let mut emit_normals: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--emit-normals" {
            emit_normals = args.next();
        } else if a == "--tex" {
            in_tex = true;
        } else if in_tex {
            tex_archives.push(a);
        } else {
            archives.push(a);
        }
    }
    assert!(
        !archives.is_empty(),
        "usage: synthetic_height_census <archive>... [--tex <archive>...]"
    );
    // Lowercased, backslash-separated names of every texture in the archives.
    let mut textures: HashSet<String> = HashSet::new();
    for t in &tex_archives {
        let a = byroredux_bsa::BsaArchive::open(t).expect("open texture bsa");
        for f in a.list_files() {
            textures.insert(f.replace('/', "\\").to_ascii_lowercase());
        }
    }
    let mut derived_normals = 0usize;
    // Distant-LOD terrain/building proxies: baked, unique-UV, never near the
    // camera at parallax range — a different population, tallied apart.
    let mut distant_lod = Bucket::default();
    let mut biggest: Vec<(f64, String)> = Vec::new();

    let mut files = 0usize;
    let mut skipped_skinned = 0usize;
    let mut no_normal = Bucket::default();
    let mut has_height = Bucket::default();
    let mut decal_or_blend = Bucket::default();
    let mut candidates = Bucket::default(); // normal, no height, rigid, opaque
    let mut by_extent: BTreeMap<&'static str, Bucket> = BTreeMap::new();
    let mut by_size_tiling: BTreeMap<&'static str, Bucket> = BTreeMap::new();
    let mut by_class_tiling: BTreeMap<String, Bucket> = BTreeMap::new();
    let mut by_class_atlas: BTreeMap<String, Bucket> = BTreeMap::new();
    // normal key -> (meshes, area m², max uv extent, sample mesh)
    let mut per_normal: BTreeMap<String, (usize, f64, f32, String)> = BTreeMap::new();
    let mut tiling_normals: HashSet<String> = HashSet::new();
    let mut atlas_normals: HashSet<String> = HashSet::new();
    let mut height_by_in_alpha = (0usize, 0usize);

    for path in &archives {
        if path.to_ascii_lowercase().ends_with(".ba2") {
            eprintln!("skip BA2 (BGSM-resolved textures): {path}");
            continue;
        }
        let a = byroredux_bsa::BsaArchive::open(path).expect("open bsa");
        for f in a.list_files() {
            if !byroredux_nif::corpus::is_nif_entry(f) {
                continue;
            }
            let Ok(bytes) = a.extract(f) else { continue };
            let Ok(scene) = byroredux_nif::parse_nif(&bytes) else {
                continue;
            };
            files += 1;
            let lower = f.replace('/', "\\").to_ascii_lowercase();
            let is_lod = lower.contains("\\lod\\")
                || lower.contains("_far.")
                || lower.contains("\\terrain\\")
                || lower.ends_with(".bto")
                || lower.ends_with(".btr");
            let mut pool = byroredux_core::string::StringPool::new();
            for m in byroredux_nif::import::import_nif(&scene, &mut pool) {
                if m.skin.is_some() {
                    skipped_skinned += 1;
                    continue;
                }
                let area = surface_area(&m.positions, &m.indices, m.scale);
                if is_lod {
                    distant_lod.add(area);
                    continue;
                }
                let mat = &m.material;
                if mat.textures.height.is_some() {
                    has_height.add(area);
                    if mat.parallax_height_in_alpha {
                        height_by_in_alpha.1 += 1;
                    } else {
                        height_by_in_alpha.0 += 1;
                    }
                    continue;
                }
                let explicit = mat.textures.normal.and_then(|s| pool.resolve(s));
                let derived: Option<String> = if explicit.is_none() && !textures.is_empty() {
                    mat.textures
                        .base_color
                        .and_then(|s| pool.resolve(s))
                        .and_then(|base| {
                            let b = base.replace('/', "\\").to_ascii_lowercase();
                            let b = b.strip_prefix("data\\").unwrap_or(&b).to_string();
                            let b = if b.starts_with("textures\\") {
                                b
                            } else {
                                format!("textures\\{b}")
                            };
                            let dot = b.rfind('.')?;
                            let cand = format!("{}_n{}", &b[..dot], &b[dot..]);
                            textures.contains(&cand).then_some(cand)
                        })
                } else {
                    None
                };
                if derived.is_some() {
                    derived_normals += 1;
                }
                let Some(normal) = explicit.or(derived.as_deref()) else {
                    no_normal.add(area);
                    continue;
                };
                if area.is_finite() {
                    biggest.push((area, format!("{f} :: {normal}")));
                    if biggest.len() > 64 {
                        biggest.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
                        biggest.truncate(8);
                    }
                }
                if mat.is_decal || mat.has_alpha {
                    decal_or_blend.add(area);
                    continue;
                }
                candidates.add(area);
                let ext = uv_extent(&m.uvs);
                {
                    let key = normal.replace('/', "\\").to_ascii_lowercase();
                    let key = key.strip_prefix("data\\").unwrap_or(&key).to_string();
                    let e = per_normal
                        .entry(key)
                        .or_insert((0, 0.0, 0.0, f.to_string()));
                    e.0 += 1;
                    if area.is_finite() {
                        e.1 += area / 4900.0;
                    }
                    e.2 = e.2.max(ext);
                }
                by_extent.entry(extent_label(ext)).or_default().add(area);
                let class = dir_class(normal);
                if ext > 1.02 {
                    by_size_tiling
                        .entry(size_label(m.local_bound_radius * m.scale))
                        .or_default()
                        .add(area);
                    by_class_tiling.entry(class).or_default().add(area);
                    tiling_normals.insert(normal.to_ascii_lowercase());
                } else {
                    by_class_atlas.entry(class).or_default().add(area);
                    atlas_normals.insert(normal.to_ascii_lowercase());
                }
            }
        }
    }

    let total = candidates.meshes
        + has_height.meshes
        + no_normal.meshes
        + decal_or_blend.meshes
        + distant_lod.meshes;
    println!("# {files} NIFs, {total} rigid meshes ({skipped_skinned} skinned skipped)");
    let pct = |n: usize, d: usize| 100.0 * n as f64 / d.max(1) as f64;
    let line = |name: &str, b: &Bucket| {
        println!(
            "{name:<34} {:>7} meshes ({:>5.1}%)  area {:>12.0} m²",
            b.meshes,
            pct(b.meshes, total),
            b.area
        );
    };
    line("distant LOD (excluded)", &distant_lod);
    line("authored height already", &has_height);
    println!(
        "    of which Oblivion alpha-height {}, separate map {}",
        height_by_in_alpha.1, height_by_in_alpha.0
    );
    line("no normal map", &no_normal);
    line("normal, but decal/alpha-blend", &decal_or_blend);
    line("CANDIDATES (normal, no height)", &candidates);

    println!("normal maps bound by `_n` convention: {derived_normals}");
    biggest.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    println!("\n## largest single candidates (m²)");
    for (a, n) in biggest.iter().take(6) {
        println!("{:>14.0}  {n}", a / 4900.0);
    }
    println!("\n## candidates by UV extent");
    for (k, b) in &by_extent {
        line(k, b);
    }
    println!("\n## tiling candidates by mesh size");
    for (k, b) in &by_size_tiling {
        line(k, b);
    }
    println!(
        "\nunique normal maps: tiling {}, atlas {}",
        tiling_normals.len(),
        atlas_normals.len()
    );

    let top = |m: &BTreeMap<String, Bucket>, label: &str| {
        let mut v: Vec<_> = m.iter().collect();
        v.sort_by(|a, b| b.1.area.partial_cmp(&a.1.area).unwrap());
        println!("\n## top normal-map dirs, {label} (by area)");
        for (k, b) in v.into_iter().take(12) {
            line(k, b);
        }
    };
    top(&by_class_tiling, "tiling");
    top(&by_class_atlas, "atlas/unique");

    if let Some(path) = emit_normals {
        let mut out = String::new();
        for (k, (n, a, e, sample)) in &per_normal {
            out.push_str(&format!("{k}\t{n}\t{a:.1}\t{e:.3}\t{sample}\n"));
        }
        std::fs::write(&path, out).expect("write normals tsv");
        eprintln!("wrote {} normal maps to {path}", per_normal.len());
    }
}
