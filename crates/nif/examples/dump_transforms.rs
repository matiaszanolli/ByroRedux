//! Transform-fidelity inspector for issue #1277 (NIF→engine translation layer).
//!
//! Gamebryo `NiTransform` stores rotation as a 3×3 matrix that is *supposed*
//! to be orthonormal, with a separate **scalar** scale. Any per-axis stretch
//! or shear an exporter baked into the matrix is therefore invisible to the
//! scalar-scale model — and `import/coord.rs::zup_matrix_to_yup_quat`
//! (→ `svd_repair_to_quat`) snaps the matrix back to the nearest pure rotation,
//! silently discarding it. This tool measures how often that actually happens.
//!
//! For each AV-bearing block (`NiObject::as_av_object`), we compute the rotation
//! matrix's column norms and column dot-products:
//!   - column norms ≈ 1, equal           → clean pure rotation
//!   - norms equal but ≠ 1                → UNIFORM scale baked in matrix
//!   - norms UNEQUAL                      → NON-UNIFORM scale baked in matrix  ← the hypothesis
//!   - column dot-products ≠ 0            → SHEAR baked in matrix
//!
//! Usage:
//!   cargo run -p byroredux-nif --example dump_transforms -- <archive.bsa> [path-filter] [limit]
//!   cargo run -p byroredux-nif --example dump_transforms -- <path.nif>

use byroredux_bsa::BsaArchive;
use byroredux_nif::types::NiMatrix3;

const TOL: f32 = 0.01; // 1% deviation threshold

#[derive(Default)]
struct Stats {
    files: usize,
    parse_fail: usize,
    av_blocks: usize,
    uniform_scale: usize,   // norms equal, ≠ 1
    nonuniform_scale: usize, // norms unequal
    shear: usize,           // off-orthogonal columns
    non_identity_rot: usize, // matrices that are not the identity (tool-liveness check)
    max_norm_spread: f32,    // worst (n_max - n_min) observed
    max_off_diag: f32,       // worst column dot observed
    max_scalar_scale: f32,   // worst scalar NiTransform.scale observed
    min_scalar_scale: f32,
}

/// Returns (col_norms, max_abs_col_dot). Column j = (m0j, m1j, m2j).
fn analyze(m: &NiMatrix3) -> ([f32; 3], f32) {
    let r = &m.rows;
    let col = |j: usize| [r[0][j], r[1][j], r[2][j]];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let norm = |a: [f32; 3]| dot(a, a).sqrt();
    let (c0, c1, c2) = (col(0), col(1), col(2));
    let norms = [norm(c0), norm(c1), norm(c2)];
    // Normalized column dot-products (cos of inter-axis angle): 0 = orthogonal.
    let safe = |x: f32| if x.abs() < 1e-6 { 1.0 } else { x };
    let d01 = (dot(c0, c1) / (safe(norms[0]) * safe(norms[1]))).abs();
    let d02 = (dot(c0, c2) / (safe(norms[0]) * safe(norms[2]))).abs();
    let d12 = (dot(c1, c2) / (safe(norms[1]) * safe(norms[2]))).abs();
    (norms, d01.max(d02).max(d12))
}

fn inspect(name: &str, bytes: &[u8], stats: &mut Stats, verbose: bool) {
    let scene = match byroredux_nif::parse_nif(bytes) {
        Ok(s) => s,
        Err(_) => {
            stats.parse_fail += 1;
            return;
        }
    };
    stats.files += 1;
    for block in &scene.blocks {
        let Some(av) = block.as_av_object() else { continue };
        stats.av_blocks += 1;
        let t = av.transform();
        let (norms, max_dot) = analyze(&t.rotation);
        let n_min = norms[0].min(norms[1]).min(norms[2]);
        let n_max = norms[0].max(norms[1]).max(norms[2]);
        // Tool-liveness + distribution tracking.
        let r = &t.rotation.rows;
        let is_identity = (r[0][0] - 1.0).abs() < 1e-4
            && (r[1][1] - 1.0).abs() < 1e-4
            && (r[2][2] - 1.0).abs() < 1e-4
            && r[0][1].abs() < 1e-4
            && r[1][0].abs() < 1e-4;
        if !is_identity {
            stats.non_identity_rot += 1;
        }
        stats.max_norm_spread = stats.max_norm_spread.max(n_max - n_min);
        stats.max_off_diag = stats.max_off_diag.max(max_dot);
        stats.max_scalar_scale = stats.max_scalar_scale.max(t.scale);
        stats.min_scalar_scale = if stats.av_blocks == 1 {
            t.scale
        } else {
            stats.min_scalar_scale.min(t.scale)
        };
        let unequal = (n_max - n_min) > TOL;
        let off_one = (n_max - 1.0).abs() > TOL || (n_min - 1.0).abs() > TOL;
        let sheared = max_dot > TOL;

        if unequal {
            stats.nonuniform_scale += 1;
        } else if off_one {
            stats.uniform_scale += 1;
        }
        if sheared {
            stats.shear += 1;
        }

        if verbose && (unequal || sheared) {
            let bname = block.block_type_name();
            println!(
                "  {:<40.40} {:<18} colnorms=[{:.3} {:.3} {:.3}] maxColDot={:.3} scalar_scale={:.3}{}{}",
                name,
                bname,
                norms[0], norms[1], norms[2],
                max_dot,
                t.scale,
                if unequal { "  [NON-UNIFORM]" } else { "" },
                if sheared { "  [SHEAR]" } else { "" },
            );
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let target = args.first().expect(
        "usage: dump_transforms <archive.bsa|path.nif> [path-filter] [limit]",
    );
    let filter = args.get(1).map(|s| s.to_lowercase());
    let limit: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);

    let mut stats = Stats::default();

    if target.to_lowercase().ends_with(".nif") {
        let bytes = std::fs::read(target).expect("read nif");
        println!("# {}", target);
        inspect(target, &bytes, &mut stats, true);
    } else {
        let archive = BsaArchive::open(target).expect("open archive");
        let all = archive.list_files();
        let matched: Vec<String> = all
            .iter()
            .filter(|p| {
                let pl = p.to_lowercase();
                pl.ends_with(".nif")
                    && filter.as_ref().map(|f| pl.contains(f)).unwrap_or(true)
            })
            .take(limit)
            .map(|s| s.to_string())
            .collect();
        println!(
            "# {} — {} NIFs match filter {:?} (of {} files)",
            target,
            matched.len(),
            filter,
            all.len()
        );
        println!("# blocks flagged [NON-UNIFORM] / [SHEAR] below:");
        for p in &matched {
            match archive.extract(p) {
                Ok(bytes) => inspect(p, &bytes, &mut stats, true),
                Err(_) => stats.parse_fail += 1,
            }
        }
    }

    println!("\n=== SUMMARY ===");
    println!("files parsed:          {}", stats.files);
    println!("parse failures:        {}", stats.parse_fail);
    println!("AV blocks examined:    {}", stats.av_blocks);
    println!(
        "uniform-scale matrices:    {} ({:.2}%)",
        stats.uniform_scale,
        pct(stats.uniform_scale, stats.av_blocks)
    );
    println!(
        "NON-UNIFORM-scale matrices: {} ({:.2}%)  <- hypothesis: these are lost by svd_repair_to_quat",
        stats.nonuniform_scale,
        pct(stats.nonuniform_scale, stats.av_blocks)
    );
    println!(
        "sheared matrices:          {} ({:.2}%)",
        stats.shear,
        pct(stats.shear, stats.av_blocks)
    );
    println!("--- tool-liveness / distribution ---");
    println!(
        "non-identity rotations:    {} ({:.2}%)  <- proves the tool reads real matrices",
        stats.non_identity_rot,
        pct(stats.non_identity_rot, stats.av_blocks)
    );
    println!("max column-norm spread:    {:.5}  (TOL={})", stats.max_norm_spread, TOL);
    println!("max column off-diagonal:   {:.5}", stats.max_off_diag);
    println!(
        "scalar NiTransform.scale:  [{:.3} .. {:.3}]",
        stats.min_scalar_scale, stats.max_scalar_scale
    );
}

fn pct(n: usize, d: usize) -> f32 {
    if d == 0 {
        0.0
    } else {
        100.0 * n as f32 / d as f32
    }
}
