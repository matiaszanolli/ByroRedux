//! Tileability of static-mesh normal maps, for deciding which materials a
//! synthetic parallax height map can be generated for.
//!
//! [`Gradients::seam_exceedance`](byroredux_terrain_height::Gradients::seam_exceedance)
//! scores each normal map: the share of wrap-seam steps above the interior's
//! 99th percentile, ≈ 0.01 for a map that tiles and large for unique edges. The input
//! is the TSV `byroredux-nif`'s `synthetic_height_census --emit-normals`
//! writes (`key  meshes  area_m2  max_uv_extent  sample_mesh`). Output is one
//! row per map, sorted by ratio, and a summary that checks the score against
//! two groups whose answer is known without a label: terrain layers
//! (`textures\landscape\…`, which tile by construction) and `*lod*` maps
//! (baked atlases, which do not).
//!
//! ```sh
//! cargo run --release -p byroredux-terrain-height --example tileability_census -- \
//!     <normals.tsv> <texture archive>...
//! ```

use byroredux_menuxml::tex::Rgba8;
use byroredux_terrain_height as th;

struct Row {
    key: String,
    meshes: usize,
    area: f64,
    uv_extent: f32,
    ratio: f32,
    size: (u32, u32),
}

fn quantiles(v: &mut [f32]) -> String {
    if v.is_empty() {
        return "n=0".into();
    }
    v.sort_by(f32::total_cmp);
    let q = |f: f32| v[((v.len() - 1) as f32 * f) as usize];
    format!(
        "n={:<5} p10 {:>6.3}  p25 {:>6.3}  median {:>6.3}  p75 {:>6.3}  p90 {:>6.3}",
        v.len(),
        q(0.1),
        q(0.25),
        q(0.5),
        q(0.75),
        q(0.9)
    )
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let archives: Vec<byroredux_bsa::BsaArchive> = args[2..]
        .iter()
        .map(|p| byroredux_bsa::BsaArchive::open(p).expect("open bsa"))
        .collect();
    let extract = |key: &str| archives.iter().rev().find_map(|a| a.extract(key).ok());

    let mut rows: Vec<Row> = Vec::new();
    let mut missing = 0usize;
    let mut undecoded = 0usize;
    for line in std::fs::read_to_string(&args[1]).unwrap().lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 4 {
            continue;
        }
        let key = f[0].to_string();
        let Some(bytes) = extract(&key) else {
            missing += 1;
            continue;
        };
        let Some(img) = Rgba8::decode_dds(&bytes) else {
            undecoded += 1;
            continue;
        };
        let g = th::Gradients::from_normal_map(&th::Rgba {
            width: img.width as usize,
            height: img.height as usize,
            pixels: &img.pixels,
        });
        rows.push(Row {
            key,
            meshes: f[1].parse().unwrap_or(0),
            area: f[2].parse().unwrap_or(0.0),
            // A few meshes carry corrupt UVs (extent in the millions); keep
            // them scored but out of the extent buckets.
            uv_extent: f[3]
                .parse::<f32>()
                .ok()
                .filter(|e| *e < 1000.0)
                .unwrap_or(f32::NAN),
            ratio: g.seam_exceedance(),
            size: (img.width, img.height),
        });
    }
    rows.sort_by(|a, b| a.ratio.total_cmp(&b.ratio));
    for r in &rows {
        println!(
            "{:>6.3}  {}x{}  meshes {:>4}  area {:>9.0} m²  uv {:>6.2}  {}",
            r.ratio, r.size.0, r.size.1, r.meshes, r.area, r.uv_extent, r.key
        );
    }

    println!(
        "\n# {} maps scored, {missing} missing, {undecoded} undecoded",
        rows.len()
    );
    let group = |name: &str, pred: &dyn Fn(&Row) -> bool| {
        let mut v: Vec<f32> = rows.iter().filter(|r| pred(r)).map(|r| r.ratio).collect();
        println!("{name:<34} {}", quantiles(&mut v));
    };
    let lod = |r: &Row| r.key.contains("lod");
    group("terrain layers (tile, known)", &|r| {
        r.key.starts_with("textures\\landscape\\") && !lod(r)
    });
    group("LOD maps (atlas, known)", &lod);
    group("all, uv extent <= 1.02", &|r| {
        r.uv_extent <= 1.02 && !lod(r)
    });
    group("all, uv extent 1.02-4", &|r| {
        r.uv_extent > 1.02 && r.uv_extent <= 4.0 && !lod(r)
    });
    group("all, uv extent > 4", &|r| r.uv_extent > 4.0 && !lod(r));

    // How much of the candidate surface each threshold keeps.
    let area_total: f64 = rows.iter().filter(|r| !lod(r)).map(|r| r.area).sum();
    println!("\nthreshold  maps kept  area kept (non-LOD)");
    for t in [0.02f32, 0.05, 0.10, 0.25] {
        let kept: Vec<&Row> = rows.iter().filter(|r| !lod(r) && r.ratio <= t).collect();
        let a: f64 = kept.iter().map(|r| r.area).sum();
        let m: usize = kept.iter().map(|r| r.meshes).sum();
        println!(
            "<= {t:<5}   {:>5} maps ({m} meshes)   {:>5.1}%",
            kept.len(),
            100.0 * a / area_total.max(1.0)
        );
    }
}
