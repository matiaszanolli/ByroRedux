//! Furniture marker extraction tests (`BSFurnitureMarker`,
//! `FurniturePosition`) — M41.5 Phase B.

use super::super::*;

use super::{make_tri_shape_data, scene_from_blocks};

// ── M41.5 Phase B — furniture marker extraction ───────────────────────

#[test]
fn imported_furniture_marker_modern_keeps_heading_and_anim_type() {
    use crate::blocks::extra_data::{FurniturePosition, FurniturePositionData};
    let pos = FurniturePosition {
        offset: [1.0, 2.0, 3.0],
        data: FurniturePositionData::Modern {
            heading: std::f32::consts::FRAC_PI_2,
            animation_type: 1, // Sit
            entry_properties: 0,
        },
    };
    let m = imported_furniture_marker(&pos);
    // zup_to_yup_pos([x, y, z]) = [x, z, -y].
    assert_eq!(m.offset, [1.0, 3.0, -2.0]);
    assert_eq!(m.heading_z_radians, Some(std::f32::consts::FRAC_PI_2));
    assert_eq!(m.animation_type, 1);
}

#[test]
fn imported_furniture_marker_legacy_defers_heading() {
    use crate::blocks::extra_data::{FurniturePosition, FurniturePositionData};
    let pos = FurniturePosition {
        offset: [0.0, -4.0, 0.0],
        data: FurniturePositionData::Legacy {
            orientation: 3,
            position_ref_1: 1,
            position_ref_2: 1,
        },
    };
    let m = imported_furniture_marker(&pos);
    assert_eq!(m.offset, [0.0, 0.0, 4.0]);
    // Legacy ushort orientation has no verified radian mapping → deferred.
    assert_eq!(m.heading_z_radians, None);
    assert_eq!(m.animation_type, 0);
}

#[test]
fn extract_furniture_markers_scans_block_and_lifts_all_positions() {
    use crate::blocks::extra_data::{BsFurnitureMarker, FurniturePosition, FurniturePositionData};
    let marker = BsFurnitureMarker {
        type_name: "BSFurnitureMarker",
        name: None,
        positions: vec![
            FurniturePosition {
                offset: [1.0, 0.0, 0.0],
                data: FurniturePositionData::Modern {
                    heading: 0.0,
                    animation_type: 1,
                    entry_properties: 0,
                },
            },
            FurniturePosition {
                offset: [0.0, 1.0, 0.0],
                data: FurniturePositionData::Modern {
                    heading: 1.0,
                    animation_type: 2,
                    entry_properties: 0,
                },
            },
        ],
    };
    let scene = scene_from_blocks(vec![Box::new(marker)]);
    let out = extract_furniture_markers(&scene);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].animation_type, 1);
    assert_eq!(out[1].animation_type, 2);
    // Second position's Y-up offset: [0,1,0] → [0,0,-1].
    assert_eq!(out[1].offset, [0.0, 0.0, -1.0]);
}

#[test]
fn extract_furniture_markers_empty_for_non_furniture_scene() {
    let scene = scene_from_blocks(vec![Box::new(make_tri_shape_data())]);
    assert!(extract_furniture_markers(&scene).is_empty());
}
