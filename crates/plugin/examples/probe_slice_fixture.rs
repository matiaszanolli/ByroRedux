//! Derive the playable-slice smoke fixture for an interior CELL (#3039).
//!
//! The P0/P1/P2 gates in `docs/smoke-tests/` used to hard-code Skyrim's
//! Bannered Mare / Bleak Falls values inline, which made "add the reference
//! title" mean "copy three scripts". They now read a per-game fixture file
//! from `docs/smoke-tests/fixtures/`, and this probe is what produces the
//! numbers in one — every value below is read out of the plugin, never
//! guessed.
//!
//! What it emits, per requested cell:
//!   * the CELL's own FormID and REFR count (the entity-floor input),
//!   * every teleport door: its REFR FormID, authored position, and the
//!     resolved destination (exterior worldspace + grid, or interior EDID),
//!   * a camera pose aimed at each door, computed as
//!     `pos = door + normalize(interior_centroid - door) * reach`,
//!     `forward = -normalize(interior_centroid - door)` — so the door sits
//!     inside `interaction::INTERACTION_REACH_BU` along the camera ray
//!     rather than at a hand-tuned pose,
//!
//! The P2 melee half of a fixture (direct `NPC_` placements, derived Health,
//! resolved weapon leaves) comes from the existing `probe_combat_fixture`
//! example — this probe deliberately does not duplicate it.
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example probe_slice_fixture -- \
//!     <ESM> <CELL_EDID> [CELL_EDID ...]

use byroredux_plugin::esm::cell::CellRef;

/// Mirrors `byroredux::interaction::INTERACTION_REACH_BU`. The camera pose
/// this probe emits must place the door strictly inside that reach or the
/// gate's prompt assertion can never fire; kept at three quarters of it so
/// the fixture has margin against the door's own collision radius.
const INTERACTION_REACH_BU: f32 = 192.0;
const CAMERA_STANDOFF_BU: f32 = INTERACTION_REACH_BU * 0.75;

/// Eye height above the authored door origin. Door REFR positions sit at the
/// threshold (floor level); an eye-level camera is what the runtime pose is.
const CAMERA_EYE_BU: f32 = 120.0;

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len <= f32::EPSILON {
        return [0.0, 1.0, 0.0];
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let esm_path = args.next().ok_or_else(|| {
        anyhow::anyhow!("usage: probe_slice_fixture ESM CELL_EDID [CELL_EDID ...]")
    })?;
    let cell_ids: Vec<String> = args.collect();
    if cell_ids.is_empty() {
        anyhow::bail!("supply at least one interior CELL EditorID");
    }

    let bytes = std::fs::read(&esm_path)?;
    let index = byroredux_plugin::esm::parse_esm(&bytes)?;

    for cell_id in cell_ids {
        let Some(cell) = index.cells.cells.get(&cell_id.to_ascii_lowercase()) else {
            println!("MISS {cell_id}");
            continue;
        };

        println!(
            "CELL {} form={:08X} refs={}",
            cell.editor_id,
            cell.form_id,
            cell.references.len()
        );

        // Centroid of the cell's own placements — the "inside" direction a
        // camera must look back from to face a threshold door.
        let mut centroid = [0.0f64; 3];
        for placed in &cell.references {
            for axis in 0..3 {
                centroid[axis] += placed.position[axis] as f64;
            }
        }
        let count = cell.references.len().max(1) as f64;
        let centroid = [
            (centroid[0] / count) as f32,
            (centroid[1] / count) as f32,
            (centroid[2] / count) as f32,
        ];
        println!(
            "  centroid=({:.1},{:.1},{:.1})",
            centroid[0], centroid[1], centroid[2]
        );

        for placed in &cell.references {
            let Some(teleport) = placed.teleport else {
                continue;
            };
            let destination = index
                .cells
                .cell_for_refr_form_id(teleport.destination)
                .map_or_else(
                    || "UNRESOLVED".to_string(),
                    |cell_ref| match cell_ref {
                        CellRef::Interior { editor_id } => format!("interior '{editor_id}'"),
                        CellRef::Exterior { worldspace, grid } => {
                            format!("exterior '{}' ({},{})", worldspace, grid.0, grid.1)
                        }
                    },
                );

            let inward = normalize([
                centroid[0] - placed.position[0],
                centroid[1] - placed.position[1],
                0.0,
            ]);
            let camera = [
                placed.position[0] + inward[0] * CAMERA_STANDOFF_BU,
                placed.position[1] + inward[1] * CAMERA_STANDOFF_BU,
                placed.position[2] + CAMERA_EYE_BU,
            ];
            // Look back at the door panel from the standoff pose. The aim
            // point sits at the same eye height as the camera, so the
            // interaction ray travels horizontally into the panel rather
            // than down at the threshold's floor-level origin.
            let forward = normalize([
                placed.position[0] - camera[0],
                placed.position[1] - camera[1],
                0.0,
            ]);

            println!(
                "  DOOR ref={:08X} base={:08X} pos=({:.1},{:.1},{:.1}) dest_ref={:08X} \
                 arrive=({:.1},{:.1},{:.1}) dest={}",
                placed.form_id,
                placed.base_form_id,
                placed.position[0],
                placed.position[1],
                placed.position[2],
                teleport.destination,
                teleport.position[0],
                teleport.position[1],
                teleport.position[2],
                destination
            );
            // The CLI's `--camera-pos` / `--camera-forward` are renderer
            // Y-up, while REFR placements are Bethesda Z-up. Emit the
            // converted pair so the fixture value is paste-ready.
            let camera_yup = byroredux_core::math::coord::zup_to_yup_pos(camera);
            let forward_yup = byroredux_core::math::coord::zup_to_yup_pos(forward);
            println!(
                "       camera-pos {:.0},{:.0},{:.0}  camera-forward {:.3},{:.3},{:.3}  (Y-up)",
                camera_yup[0],
                camera_yup[1],
                camera_yup[2],
                forward_yup[0],
                forward_yup[1],
                forward_yup[2]
            );
            let arrive_yup = byroredux_core::math::coord::zup_to_yup_pos(teleport.position);
            println!(
                "       arrive-yup {:.0},{:.0},{:.0}",
                arrive_yup[0], arrive_yup[1], arrive_yup[2]
            );
        }
    }

    Ok(())
}
