//! Inspect authored spot placements without constructing a renderer.
//! Usage: cargo run -p byroredux-plugin --example spot_light_census -- <ESM> [CELL_FILTER]
//! Directions are the current placement rotation applied to model +X, in Y-up.
//! This reports evidence; it does not infer a correct axis from a light's name.

use byroredux_core::math::{Vec3, coord::euler_zup_to_quat_yup_mode};
use byroredux_plugin::esm::{self, reader::GameKind};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("expected ESM path"))?;
    let filter = args.next().unwrap_or_default().to_ascii_lowercase();
    let index = esm::records::parse_esm(&std::fs::read(path)?)?;
    let mut cells: Vec<_> = index.cells.cells.values().collect();
    cells.sort_by_key(|cell| &cell.editor_id);
    println!("game={:?}; interior placements only", index.game);
    for cell in cells {
        if !cell.editor_id.to_ascii_lowercase().contains(&filter) {
            continue;
        }
        let mut rows = Vec::new();
        for reference in &cell.references {
            let Some(base) = index.cells.statics.get(&reference.base_form_id) else {
                continue;
            };
            let Some(light) = &base.light_data else {
                continue;
            };
            // Independent source-data oracle: xEdit dev-4.1.6 LIGH layouts
            // (wbDefinitionsTES4/FNV/TES5/FO4/FO76/SF1.pas), not a universal
            // 0x200 flag. Shadow Spotlight itself implies cone geometry.
            let spot = match index.game {
                GameKind::Starfield => matches!(light.starfield_light_type, 1 | 2),
                GameKind::Fallout4 | GameKind::Fallout76 => light.flags & 0x4400 != 0,
                _ => light.flags & 0x600 != 0,
            };
            if !spot {
                continue;
            }
            let [rx, ry, rz] = reference.rotation;
            let direction = euler_zup_to_quat_yup_mode(1, rx, ry, rz) * Vec3::X;
            rows.push(format!(
                "  refr={:08X} base={:08X} {} flags={:#x} pos_zup={:?} rot={:?} plus_x_yup={:?} fov={} model={:?}",
                reference.form_id, reference.base_form_id, base.editor_id,
                light.flags, reference.position, reference.rotation, direction.to_array(),
                light.fov_degrees, base.model_path,
            ));
        }
        if !rows.is_empty() {
            println!(
                "{} ({:08X}): {} spots",
                cell.editor_id,
                cell.form_id,
                rows.len()
            );
            for row in rows {
                println!("{row}");
            }
        }
    }
    Ok(())
}
