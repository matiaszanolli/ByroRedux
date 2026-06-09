//! REFR rotation A/B sweep — for issue #1277, the post-mode-1 "walls 90° off"
//! investigation. For every REFR in the given cell, apply each of the 4 Euler
//! conversion modes (the same set the runtime dispatcher exposes via
//! `--rotation-mode N`) and surface REFRs whose computed Y-up quaternion differs
//! between modes. These are the only REFRs that can be visually affected by the
//! shipped ZYX-vs-XYZ flip.
//!
//! Modes (mirror `byroredux/src/cell_loader/euler.rs`):
//!   0: CW + XYZ — `Rx(-rx) * Rz(ry) * Ry(-rz)` (pre-2026-05-26 ship)
//!   1: CW + ZYX — `Ry(-rz) * Rz(ry) * Rx(-rx)` (current ship, OpenMW)
//!   2: CCW + ZYX — `Ry(rz) * Rz(-ry) * Rx(rx)`
//!   3: CCW + XYZ — `Rx(rx) * Rz(-ry) * Ry(rz)`
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example cell_rot_sweep -- <ESM> <CELL_EDID> [limit]

use byroredux_core::math::Quat;
use byroredux_plugin::esm;
use std::collections::HashMap;

fn mode_quat(mode: u8, rx: f32, ry: f32, rz: f32) -> Quat {
    match mode {
        0 => Quat::from_rotation_x(-rx) * Quat::from_rotation_z(ry) * Quat::from_rotation_y(-rz),
        1 => Quat::from_rotation_y(-rz) * Quat::from_rotation_z(ry) * Quat::from_rotation_x(-rx),
        2 => Quat::from_rotation_y(rz) * Quat::from_rotation_z(-ry) * Quat::from_rotation_x(rx),
        3 => Quat::from_rotation_x(rx) * Quat::from_rotation_z(-ry) * Quat::from_rotation_y(rz),
        _ => unreachable!(),
    }
}

/// Angle (radians) between two unit quaternions, ignoring sign (q == -q).
fn quat_angle(a: Quat, b: Quat) -> f32 {
    let d = a.dot(b).abs().clamp(-1.0, 1.0);
    2.0 * d.acos()
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let esm_path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: ESM CELL_EDID [limit]"))?;
    let cell_edid = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: ESM CELL_EDID [limit]"))?;
    let limit: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(40);

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

    println!(
        "# {} ({:08X}) — {} REFRs",
        cell_edid,
        cell.form_id,
        cell.references.len()
    );

    let mut multi_axis = 0usize;
    let mut mode_1_vs_0_diff = 0usize;
    let mut any_mode_diff = 0usize;
    // Per-REFR records that differ between modes 0 and 1 (the only flip the
    // shipped fix actually changed). These are the visual candidates.
    let mut diffs: Vec<(u32, u32, [f32; 3], f32)> = Vec::new();

    for refr in &cell.references {
        let r = refr.rotation;
        let nonzero =
            (r[0].abs() > 1e-5) as u8 + (r[1].abs() > 1e-5) as u8 + (r[2].abs() > 1e-5) as u8;
        if nonzero >= 2 {
            multi_axis += 1;
        }
        let q0 = mode_quat(0, r[0], r[1], r[2]);
        let q1 = mode_quat(1, r[0], r[1], r[2]);
        let q2 = mode_quat(2, r[0], r[1], r[2]);
        let q3 = mode_quat(3, r[0], r[1], r[2]);

        let a01 = quat_angle(q0, q1);
        let a02 = quat_angle(q0, q2);
        let a03 = quat_angle(q0, q3);
        let max_diff = a01.max(a02).max(a03);

        if a01 > 1e-4 {
            mode_1_vs_0_diff += 1;
            diffs.push((refr.form_id, refr.base_form_id, r, a01));
        }
        if max_diff > 1e-4 {
            any_mode_diff += 1;
        }
    }

    diffs.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));

    println!("\n=== SUMMARY ===");
    println!("total REFRs:              {}", cell.references.len());
    println!(
        "multi-axis Euler (≥2 nonzero):  {} ({:.1}%)",
        multi_axis,
        100.0 * multi_axis as f32 / cell.references.len().max(1) as f32
    );
    println!(
        "REFRs where mode 1 ≠ mode 0:    {} ({:.1}%)  <- visual candidates for the shipped fix",
        mode_1_vs_0_diff,
        100.0 * mode_1_vs_0_diff as f32 / cell.references.len().max(1) as f32
    );
    println!(
        "REFRs where any mode ≠ another: {} ({:.1}%)",
        any_mode_diff,
        100.0 * any_mode_diff as f32 / cell.references.len().max(1) as f32
    );

    println!(
        "\n=== TOP {} REFRs by mode-0-vs-mode-1 quat angle (deg) ===",
        diffs.len().min(limit)
    );
    println!(
        "{:>10} {:>10} {:>9} {:>9} {:>9} {:>8}  base_mesh",
        "form_id", "base", "rx_deg", "ry_deg", "rz_deg", "Δ_deg"
    );
    for (fid, base, r, a01) in diffs.iter().take(limit) {
        let mesh = form_to_model
            .get(base)
            .map(|s| s.as_str())
            .unwrap_or("<no STAT model>");
        println!(
            "{:>10X} {:>10X} {:>9.2} {:>9.2} {:>9.2} {:>8.2}  {}",
            fid,
            base,
            r[0].to_degrees(),
            r[1].to_degrees(),
            r[2].to_degrees(),
            a01.to_degrees(),
            mesh,
        );
    }
    Ok(())
}
