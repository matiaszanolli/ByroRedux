//! Static analyzer for unusual REFR data — #1281 Workstream B.
//!
//! For a given interior cell, surface REFRs that are statistical
//! outliers along three axes the geometric-defect detective work
//! cares about most:
//!
//! 1. **Scale extremes** — almost every vanilla REFR ships `scale = 1.0`.
//!    Any value materially different is unusual and worth eyeballing
//!    (a `scale = 100.0` from a corrupt/troll plugin would mis-render
//!    as a giant; a `scale = 0.0` collapses the mesh to a point).
//!
//! 2. **Non-axis-aligned rotation** — Bethesda authors most REFRs at
//!    multiples of π/2 (90°). REFRs whose Euler angles aren't snapped
//!    to that grid are the multi-axis class the #1277 epic identified
//!    as mode-sensitive in `cell_rot_sweep.rs` (180° flips between
//!    XYZ and ZYX composition). Surfacing them here lets a triage
//!    pass spot-check whether they render correctly without launching
//!    the engine.
//!
//! 3. **Position outliers** — REFRs whose `position` is > 3σ from the
//!    cell-population mean. A typical interior cell has a tight
//!    spatial distribution; outliers are often editor-marker leaks,
//!    skybox planes placed at extreme world coordinates, or
//!    misplaced architecture pieces.
//!
//! Output: per-axis top-N list with `form_id`, base mesh path,
//! position, rotation (degrees), and the outlier score. Sample-cell
//! statistics printed first so the operator can sanity-check the
//! distribution.
//!
//! Use this BEFORE launching the engine when a per-REFR placement
//! defect is suspected. The `cell_rot_sweep` example covers the
//! Euler-mode A/B; this one covers everything else.
//!
//! ## Usage
//!
//! ```sh
//! cargo run -p byroredux-plugin --example cell_refr_outliers -- \
//!     <ESM> <CELL_EDID> [top_n]
//! ```
//!
//! `top_n` defaults to 10; output is grouped by axis.

use byroredux_plugin::esm;
use std::collections::HashMap;

const AXIS_SNAP_TOLERANCE_RAD: f32 = 0.01; // ~0.57° — tight to catch any non-snap
const SCALE_EPSILON: f32 = 1e-3; // |scale - 1.0| threshold for "unusual"
const POSITION_SIGMA_CUTOFF: f32 = 3.0;

/// Smallest distance from `angle` to any multiple of π/2, in radians.
/// `0` means perfectly axis-aligned, `π/4` is the worst case.
fn axis_snap_residual(angle: f32) -> f32 {
    let quarter = std::f32::consts::FRAC_PI_2;
    let modded = angle.rem_euclid(quarter);
    modded.min(quarter - modded)
}

/// Per-REFR "non-axis-aligned" score: sum of per-axis snap residuals,
/// in radians. 0 = all axes axis-aligned (the common case).
fn rotation_non_axis_score(rot: [f32; 3]) -> f32 {
    axis_snap_residual(rot[0]) + axis_snap_residual(rot[1]) + axis_snap_residual(rot[2])
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let esm_path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: ESM CELL_EDID [top_n]"))?;
    let cell_edid = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: ESM CELL_EDID [top_n]"))?;
    let top_n: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(10);

    let bytes = std::fs::read(&esm_path)?;
    let index = esm::records::parse_esm(&bytes)?;

    let mut form_to_model: HashMap<u32, String> = HashMap::new();
    for (fid, stat) in index.cells.statics.iter() {
        if !stat.model_path.is_empty() {
            form_to_model.insert(*fid, stat.model_path.clone());
        }
    }

    let key = cell_edid.to_ascii_lowercase();
    let cell = index
        .cells
        .cells
        .get(&key)
        .ok_or_else(|| anyhow::anyhow!("cell '{}' not in ESM", cell_edid))?;

    let n = cell.references.len();
    if n == 0 {
        println!("Cell {} ({:08X}) has 0 REFRs.", cell_edid, cell.form_id);
        return Ok(());
    }

    println!(
        "# {} ({:08X}) — {} REFRs",
        cell_edid, cell.form_id, n
    );

    // ── 1. Distribution stats ────────────────────────────────────────
    let mean_pos = {
        let mut acc = [0.0f32; 3];
        for r in &cell.references {
            for i in 0..3 {
                acc[i] += r.position[i];
            }
        }
        [acc[0] / n as f32, acc[1] / n as f32, acc[2] / n as f32]
    };
    let stddev_pos = {
        let mut var = [0.0f32; 3];
        for r in &cell.references {
            for i in 0..3 {
                let d = r.position[i] - mean_pos[i];
                var[i] += d * d;
            }
        }
        [
            (var[0] / n as f32).sqrt(),
            (var[1] / n as f32).sqrt(),
            (var[2] / n as f32).sqrt(),
        ]
    };
    let (mut scale_min, mut scale_max) = (f32::MAX, f32::MIN);
    let mut scale_eq_one = 0usize;
    let mut axis_aligned = 0usize;
    for r in &cell.references {
        scale_min = scale_min.min(r.scale);
        scale_max = scale_max.max(r.scale);
        if (r.scale - 1.0).abs() < SCALE_EPSILON {
            scale_eq_one += 1;
        }
        if rotation_non_axis_score(r.rotation) < AXIS_SNAP_TOLERANCE_RAD {
            axis_aligned += 1;
        }
    }
    println!(
        "\n=== Distribution ===\n  \
         position mean: ({:.1}, {:.1}, {:.1})\n  \
         position stddev: ({:.1}, {:.1}, {:.1})\n  \
         scale range: [{:.4} .. {:.4}]  (= 1.0 within ε on {}/{} REFRs)\n  \
         rotation: {}/{} REFRs are axis-aligned (multiples of π/2 within ~0.57°)",
        mean_pos[0], mean_pos[1], mean_pos[2],
        stddev_pos[0], stddev_pos[1], stddev_pos[2],
        scale_min, scale_max, scale_eq_one, n,
        axis_aligned, n,
    );

    // ── 2. Scale outliers ────────────────────────────────────────────
    let mut scale_outliers: Vec<(usize, f32)> = cell
        .references
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let dev = (r.scale - 1.0).abs();
            if dev >= SCALE_EPSILON {
                Some((i, dev))
            } else {
                None
            }
        })
        .collect();
    scale_outliers.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    println!(
        "\n=== Scale outliers (top {}) — |scale - 1.0| ≥ {} ===",
        scale_outliers.len().min(top_n),
        SCALE_EPSILON,
    );
    println!(
        "{:>10} {:>10} {:>8} {:>8}  {}",
        "form_id", "base", "scale", "Δ", "base_mesh"
    );
    for (idx, dev) in scale_outliers.iter().take(top_n) {
        let r = &cell.references[*idx];
        let mesh = form_to_model
            .get(&r.base_form_id)
            .map(String::as_str)
            .unwrap_or("<no STAT model>");
        println!(
            "{:>10X} {:>10X} {:>8.4} {:>8.4}  {}",
            r.form_id, r.base_form_id, r.scale, dev, mesh,
        );
    }

    // ── 3. Non-axis-aligned rotation outliers ───────────────────────
    let mut rot_outliers: Vec<(usize, f32)> = cell
        .references
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let score = rotation_non_axis_score(r.rotation);
            if score >= AXIS_SNAP_TOLERANCE_RAD {
                Some((i, score))
            } else {
                None
            }
        })
        .collect();
    rot_outliers.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    println!(
        "\n=== Non-axis-aligned rotation outliers (top {}) ===",
        rot_outliers.len().min(top_n),
    );
    println!(
        "{:>10} {:>10} {:>9} {:>9} {:>9} {:>8}  {}",
        "form_id", "base", "rx_deg", "ry_deg", "rz_deg", "score°", "base_mesh"
    );
    for (idx, score) in rot_outliers.iter().take(top_n) {
        let r = &cell.references[*idx];
        let mesh = form_to_model
            .get(&r.base_form_id)
            .map(String::as_str)
            .unwrap_or("<no STAT model>");
        println!(
            "{:>10X} {:>10X} {:>9.2} {:>9.2} {:>9.2} {:>8.2}  {}",
            r.form_id,
            r.base_form_id,
            r.rotation[0].to_degrees(),
            r.rotation[1].to_degrees(),
            r.rotation[2].to_degrees(),
            score.to_degrees(),
            mesh,
        );
    }

    // ── 4. Position outliers (> 3σ from cell mean) ──────────────────
    let mut pos_outliers: Vec<(usize, f32)> = cell
        .references
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let dx = stddev_pos[0]
                .max(1e-3)
                .recip()
                * (r.position[0] - mean_pos[0]);
            let dy = stddev_pos[1]
                .max(1e-3)
                .recip()
                * (r.position[1] - mean_pos[1]);
            let dz = stddev_pos[2]
                .max(1e-3)
                .recip()
                * (r.position[2] - mean_pos[2]);
            let sigma = (dx * dx + dy * dy + dz * dz).sqrt();
            if sigma > POSITION_SIGMA_CUTOFF {
                Some((i, sigma))
            } else {
                None
            }
        })
        .collect();
    pos_outliers.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    println!(
        "\n=== Position outliers (top {}) — > {}σ from cell mean ===",
        pos_outliers.len().min(top_n),
        POSITION_SIGMA_CUTOFF,
    );
    println!(
        "{:>10} {:>10} {:>10} {:>10} {:>10} {:>7}  {}",
        "form_id", "base", "x", "y", "z", "σ", "base_mesh"
    );
    for (idx, sigma) in pos_outliers.iter().take(top_n) {
        let r = &cell.references[*idx];
        let mesh = form_to_model
            .get(&r.base_form_id)
            .map(String::as_str)
            .unwrap_or("<no STAT model>");
        println!(
            "{:>10X} {:>10X} {:>10.1} {:>10.1} {:>10.1} {:>7.2}  {}",
            r.form_id,
            r.base_form_id,
            r.position[0],
            r.position[1],
            r.position[2],
            sigma,
            mesh,
        );
    }

    // ── 5. Aggregate counts ─────────────────────────────────────────
    println!(
        "\n=== Summary ===\n  \
         scale outliers:     {} / {} ({:.1}%)\n  \
         rotation outliers:  {} / {} ({:.1}%)\n  \
         position outliers:  {} / {} ({:.1}%) (cumulative; mean-normalised)",
        scale_outliers.len(),
        n,
        100.0 * scale_outliers.len() as f32 / n as f32,
        rot_outliers.len(),
        n,
        100.0 * rot_outliers.len() as f32 / n as f32,
        pos_outliers.len(),
        n,
        100.0 * pos_outliers.len() as f32 / n as f32,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_snap_residual_is_zero_at_quarter_pi_multiples() {
        let cases = [
            0.0,
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::PI,
            std::f32::consts::PI * 1.5,
            std::f32::consts::PI * 2.0,
            -std::f32::consts::FRAC_PI_2,
        ];
        for a in cases {
            assert!(
                axis_snap_residual(a) < 1e-5,
                "{} rad should be axis-aligned, got residual {}",
                a,
                axis_snap_residual(a)
            );
        }
    }

    #[test]
    fn axis_snap_residual_is_pi_over_4_at_45_deg() {
        // Halfway between snap angles — the worst case.
        let r = axis_snap_residual(std::f32::consts::FRAC_PI_4);
        assert!(
            (r - std::f32::consts::FRAC_PI_4).abs() < 1e-5,
            "45° should give residual π/4, got {}",
            r
        );
    }

    #[test]
    fn rotation_non_axis_score_handles_axis_aligned_triple() {
        // Pure 90° about Z, rest at zero — axis-aligned.
        let score = rotation_non_axis_score([0.0, 0.0, std::f32::consts::FRAC_PI_2]);
        assert!(score < 1e-5);
    }

    #[test]
    fn rotation_non_axis_score_flags_multi_axis_skew() {
        // 30° about each axis — none snapped.
        let thirty = std::f32::consts::PI / 6.0;
        let score = rotation_non_axis_score([thirty, thirty, thirty]);
        // Each axis contributes 30° = π/6 ≈ 0.524 rad (since 30° is
        // 30° away from the nearest snap at 0°, and < 60° to next snap).
        assert!((score - 3.0 * thirty).abs() < 1e-5);
    }
}
