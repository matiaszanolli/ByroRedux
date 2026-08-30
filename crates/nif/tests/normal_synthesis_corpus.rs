//! #3541 corpus gate — geometry that authors no normals must reach the
//! importer with normals DERIVED from face geometry, not a constant
//! `[0, 1, 0]` world-up.
//!
//! Two independent checks, both against real archives:
//!
//! 1. **Correctness of the derivation.** For meshes that DO author normals,
//!    synthesize from positions + triangles and compare. This is what
//!    settles the winding convention empirically instead of assuming it —
//!    an inverted convention would show as a strongly negative mean dot.
//! 2. **Coverage.** For the classes the finding measured — Oblivion
//!    `meshes\landscape\lod\*`, FO4 `.bto`/`.btr`, Skyrim LOD/FaceGen — no
//!    imported mesh may come out as a uniform world-up field.
//!
//! `#[ignore]` by default per this crate's convention; opt in with
//! `cargo test -p byroredux-nif --test normal_synthesis_corpus -- --ignored --nocapture`.

mod common;

use common::{open_all_mesh_archives, Game};

/// Mean dot product between derived and authored normals, over every mesh in
/// the sampled corpus that ships a real normal lane.
///
/// A correct winding convention gives a strongly positive mean. This is the
/// check that makes the whole pass trustworthy: the derivation runs in
/// renderer Y-up space, and the Z-up -> Y-up transform is a proper rotation
/// (det = +1) so winding is preserved — but "preserved" is a claim about the
/// data, and this measures it.
#[test]
#[ignore = "needs game data on disk"]
fn derived_normals_agree_with_authored_ones() {
    let mut any = false;
    for game in [Game::Oblivion, Game::FalloutNV, Game::SkyrimSE] {
        let Some(archives) = open_all_mesh_archives(game) else {
            eprintln!("[{}] skip: no archives", game.label());
            continue;
        };
        let mut shapes = 0usize;
        let mut sign_agree = 0usize;
        let mut dot_sum = 0.0f64;
        let mut verts = 0usize;
        for (_, archive) in &archives {
            for path in archive
                .list_files()
                .into_iter()
                .filter(|p| p.ends_with(".nif"))
                .take(2000)
            {
                let Ok(bytes) = archive.extract(&path) else {
                    continue;
                };
                let Ok(scene) = byroredux_nif::parse_nif(&bytes) else {
                    continue;
                };
                let mut pool = byroredux_core::string::StringPool::new();
                for mesh in byroredux_nif::import::import_nif(&scene, &mut pool) {
                    // Only meshes with a real authored lane can be graded.
                    if mesh.normals.len() != mesh.positions.len()
                        || mesh.positions.len() < 3
                        || mesh.normals.iter().all(|n| *n == [0.0, 1.0, 0.0])
                    {
                        continue;
                    }
                    let (sum, n) = mean_dot_against_derived(&mesh);
                    if n == 0 {
                        continue;
                    }
                    shapes += 1;
                    verts += n;
                    dot_sum += sum;
                    if sum > 0.0 {
                        sign_agree += 1;
                    }
                }
            }
        }
        if shapes == 0 {
            continue;
        }
        any = true;
        let mean = dot_sum / verts as f64;
        eprintln!(
            "[{}] {shapes} shapes / {verts} verts: mean dot {mean:.4}, \
             sign agrees on {sign_agree}",
            game.label()
        );
        assert!(
            mean > 0.8,
            "[{}] derived normals disagree with authored ones (mean dot {mean:.4}) \
             — the winding convention is wrong, not merely imprecise",
            game.label()
        );
        assert!(
            sign_agree * 100 >= shapes * 99,
            "[{}] only {sign_agree} of {shapes} shapes agree in sign",
            game.label()
        );
    }
    assert!(any, "no game data available — nothing was checked");
}

/// The classes the finding measured must no longer import as a uniform
/// world-up field: Oblivion distant-terrain LOD (100 of 100 affected), FO4
/// `.bto`/`.btr` (14,054 of 15,614), Skyrim LOD + FaceGen (part of 19,657 of
/// 96,123).
#[test]
#[ignore = "needs game data on disk"]
fn lod_and_facegen_geometry_no_longer_imports_flat() {
    let cases: [(Game, &dyn Fn(&str) -> bool, &str); 3] = [
        (
            Game::Oblivion,
            &|p: &str| p.contains("landscape\\lod\\") && p.ends_with(".nif"),
            "distant-terrain LOD",
        ),
        (
            Game::Fallout4,
            &|p: &str| p.ends_with(".bto") || p.ends_with(".btr"),
            "baked object/terrain LOD",
        ),
        (
            Game::SkyrimSE,
            &|p: &str| p.contains("facegen") || p.ends_with(".bto") || p.ends_with(".btr"),
            "FaceGen + baked LOD",
        ),
    ];
    let mut any = false;
    for (game, matches, label) in cases {
        let Some(archives) = open_all_mesh_archives(game) else {
            eprintln!("[{}] skip: no archives", game.label());
            continue;
        };
        let mut checked = 0usize;
        let mut flat = Vec::new();
        for (_, archive) in &archives {
            for path in archive
                .list_files()
                .into_iter()
                .filter(|p| matches(&p.to_ascii_lowercase()))
                .take(400)
            {
                let Ok(bytes) = archive.extract(&path) else {
                    continue;
                };
                let Ok(scene) = byroredux_nif::parse_nif(&bytes) else {
                    continue;
                };
                let mut pool = byroredux_core::string::StringPool::new();
                for mesh in byroredux_nif::import::import_nif(&scene, &mut pool) {
                    if mesh.positions.len() < 3 || mesh.indices.len() < 6 {
                        continue;
                    }
                    checked += 1;
                    // A genuinely horizontal surface — an LOD water plane,
                    // an additive FX card — legitimately HAS a uniform +Y
                    // normal, and the finding says so (57 of Oblivion's 252
                    // affected blocks are exactly that). So the defect is
                    // not "uniform normals"; it is "uniform normals on a
                    // surface whose own faces disagree". Measure that
                    // directly rather than reaching for a bounds heuristic.
                    if mesh.normals.iter().all(|n| *n == [0.0, 1.0, 0.0])
                        && faces_disagree_with_world_up(&mesh)
                        && flat.len() < 5
                    {
                        flat.push(path.clone());
                    }
                }
            }
        }
        if checked == 0 {
            eprintln!("[{}] skip: no {label} meshes found", game.label());
            continue;
        }
        any = true;
        eprintln!("[{}] {checked} {label} meshes checked", game.label());
        assert!(
            flat.is_empty(),
            "[{}] {label}: meshes with real relief still import as a uniform \
             world-up normal field (#3541): {flat:?}",
            game.label()
        );
    }
    assert!(any, "no game data available — nothing was checked");
}

fn mean_dot_against_derived(mesh: &byroredux_nif::import::ImportedMesh) -> (f64, usize) {
    let triangles: Vec<[u16; 3]> = mesh
        .indices
        .chunks_exact(3)
        .filter_map(|c| {
            Some([
                u16::try_from(c[0]).ok()?,
                u16::try_from(c[1]).ok()?,
                u16::try_from(c[2]).ok()?,
            ])
        })
        .collect();
    if triangles.is_empty() {
        return (0.0, 0);
    }
    let derived = derive(&mesh.positions, &triangles);
    let mut sum = 0.0f64;
    let mut n = 0usize;
    for (a, b) in derived.iter().zip(mesh.normals.iter()) {
        if *a == [0.0, 1.0, 0.0] {
            continue; // degenerate vertex — nothing was derived
        }
        sum += f64::from(a[0] * b[0] + a[1] * b[1] + a[2] * b[2]);
        n += 1;
    }
    (sum, n)
}

/// Whether the mesh's own face normals contradict a uniform world-up field.
///
/// Exact rather than heuristic: if some face tilts more than 15 degrees off
/// +Y, no uniform +Y normal field can be that surface's true shading, so a
/// mesh importing as one is the defect. A genuinely horizontal plane fails
/// this and is correctly ignored.
fn faces_disagree_with_world_up(mesh: &byroredux_nif::import::ImportedMesh) -> bool {
    const COS_15_DEG: f32 = 0.965_925_8;
    for c in mesh.indices.chunks_exact(3) {
        let (i0, i1, i2) = (c[0] as usize, c[1] as usize, c[2] as usize);
        let n = mesh.positions.len();
        if i0 >= n || i1 >= n || i2 >= n {
            continue;
        }
        let (p0, p1, p2) = (mesh.positions[i0], mesh.positions[i1], mesh.positions[i2]);
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let f = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let len = (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt();
        if len > 1e-9 && (f[1] / len).abs() < COS_15_DEG {
            return true;
        }
    }
    false
}

/// A local copy of the synthesis, so the test grades the *convention*
/// independently rather than asserting the production function against
/// itself.
fn derive(positions: &[[f32; 3]], triangles: &[[u16; 3]]) -> Vec<[f32; 3]> {
    let n = positions.len();
    let mut accum = vec![[0.0f32; 3]; n];
    for t in triangles {
        let (i0, i1, i2) = (t[0] as usize, t[1] as usize, t[2] as usize);
        if i0 >= n || i1 >= n || i2 >= n {
            continue;
        }
        let (p0, p1, p2) = (positions[i0], positions[i1], positions[i2]);
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let f = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        for &i in &[i0, i1, i2] {
            accum[i][0] += f[0];
            accum[i][1] += f[1];
            accum[i][2] += f[2];
        }
    }
    accum
        .into_iter()
        .map(|v| {
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            if l > 1e-12 {
                [v[0] / l, v[1] / l, v[2] / l]
            } else {
                [0.0, 1.0, 0.0]
            }
        })
        .collect()
}
