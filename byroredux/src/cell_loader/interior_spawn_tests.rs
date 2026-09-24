//! Direct-interior-load spawn ladder: `COCMarkerHeading` → door arrival
//! (partner `XTEL`) → caller fallback. See `interior_spawn.rs`.

use super::interior_spawn::{authored_spawn_pose, SpawnPoseSource};
use byroredux_core::math::Vec3;
use byroredux_plugin::esm::cell::{CellData, EsmCellIndex, PlacedRef, StaticObject, TeleportDest};

const COC_BASE: u32 = 0x0000_0032;
const CHAIR_BASE: u32 = 0x0001_0000;
const DOOR_BASE: u32 = 0x0002_0000;

fn placed(form_id: u32, base_form_id: u32, position: [f32; 3], rotation: [f32; 3]) -> PlacedRef {
    PlacedRef {
        form_id,
        base_form_id,
        group_type: 0xFF,
        position,
        rotation,
        scale: 1.0,
        enable_parent: None,
        teleport: None,
        reputation_ref: None,
        primitive: None,
        linked_refs: Vec::new(),
        location_ref_types: Vec::new(),
        rooms: Vec::new(),
        portals: Vec::new(),
        radius_override: None,
        alt_texture_ref: None,
        land_texture_ref: None,
        texture_slot_swaps: Vec::new(),
        emissive_light_ref: None,
        material_swap_ref: None,
        ownership: None,
        script_instance: None,
        lock: None,
        water_velocity: None,
    }
}

fn door(form_id: u32, position: [f32; 3], destination: u32, arrival: [f32; 3]) -> PlacedRef {
    PlacedRef {
        teleport: Some(TeleportDest {
            destination,
            position: arrival,
            rotation: [0.0; 3],
        }),
        ..placed(form_id, DOOR_BASE, position, [0.0; 3])
    }
}

fn stat(form_id: u32, editor_id: &str) -> StaticObject {
    StaticObject {
        form_id,
        editor_id: editor_id.to_string(),
        model_path: String::new(),
        record_type: byroredux_plugin::record::RecordType::STAT,
        light_data: None,
        addon_data: None,
        has_script: false,
        script_instance: None,
        script_form_id: 0,
        visible_when_distant: false,
    }
}

fn cell(editor_id: &str, references: Vec<PlacedRef>) -> CellData {
    CellData {
        form_id: 0xABCD,
        editor_id: editor_id.to_string(),
        display_name: None,
        references,
        is_interior: true,
        show_sky: None,
        grid: None,
        lighting: None,
        landscape: None,
        water_height: None,
        water_height_is_explicit: false,
        image_space_form: None,
        water_type_form: None,
        acoustic_space_form: None,
        music_type_form: None,
        music_type_enum: None,
        climate_override: None,
        location_form: None,
        encounter_zone_form: None,
        regions: Vec::new(),
        regional_color_override: None,
        precombined_mesh_hashes: Vec::new(),
        absorbed_refs: std::collections::HashSet::new(),
        lighting_template_form: None,
        ownership: None,
        navmeshes: Vec::new(),
        pathgrids: Vec::new(),
        deleted_refs: Vec::new(),
    }
}

/// An index holding the far side of a door: `partner` lives in another
/// cell, and its own XTEL is the arrival pose into the cell under test.
fn index_with_partner(partner: PlacedRef) -> EsmCellIndex {
    let mut index = EsmCellIndex::default();
    index.statics.insert(COC_BASE, stat(COC_BASE, "COCMarkerHeading"));
    index.statics.insert(CHAIR_BASE, stat(CHAIR_BASE, "Chair01"));
    index
        .cells
        .insert("otherside".to_string(), cell("OtherSide", vec![partner]));
    index
}

/// The in-cell door `0x100` at the wall, linked to partner `0x900`, whose
/// XTEL lands the player at `(10, 20, 30)` Z-up inside this cell.
fn linked_pair() -> (PlacedRef, PlacedRef) {
    let this_side = door(0x100, [500.0, 600.0, 0.0], 0x900, [-4000.0, 0.0, 0.0]);
    let far_side = door(0x900, [-4000.0, 0.0, 0.0], 0x100, [10.0, 20.0, 30.0]);
    (this_side, far_side)
}

#[test]
fn coc_marker_wins_over_a_linked_door() {
    let (this_side, far_side) = linked_pair();
    let index = index_with_partner(far_side);
    let refs = vec![
        this_side,
        placed(0x200, CHAIR_BASE, [0.0; 3], [0.0; 3]),
        placed(0x300, COC_BASE, [1.0, 2.0, 3.0], [0.0; 3]),
    ];

    let pose = authored_spawn_pose(&refs, &index).expect("the cell places a COC marker");
    assert_eq!(pose.source, SpawnPoseSource::CocMarker);
    // Z-up (x, y, z) → Y-up (x, z, -y), the XTEL path's conversion.
    assert_eq!(pose.position, Vec3::new(1.0, 3.0, -2.0));
}

#[test]
fn coc_marker_editor_id_matches_case_insensitively() {
    let mut index = EsmCellIndex::default();
    index.statics.insert(COC_BASE, stat(COC_BASE, "cocmarkerheading"));
    let refs = vec![placed(0x300, COC_BASE, [1.0, 2.0, 3.0], [0.0; 3])];

    let pose = authored_spawn_pose(&refs, &index).expect("EDID lookups are case-insensitive");
    assert_eq!(pose.source, SpawnPoseSource::CocMarker);
}

/// Without a marker, the player lands where walking in through the door
/// would put them — the partner's XTEL — not in the door frame itself.
#[test]
fn door_arrival_uses_the_partner_xtel_not_the_door_placement() {
    let (this_side, far_side) = linked_pair();
    let index = index_with_partner(far_side);
    let refs = vec![placed(0x200, CHAIR_BASE, [0.0; 3], [0.0; 3]), this_side];

    let pose = authored_spawn_pose(&refs, &index).expect("the door links back into this cell");
    assert_eq!(pose.source, SpawnPoseSource::DoorArrival);
    assert_eq!(pose.position, Vec3::new(10.0, 30.0, -20.0));
}

#[test]
fn a_partner_that_does_not_link_back_is_not_an_arrival_pose() {
    let this_side = door(0x100, [500.0, 600.0, 0.0], 0x900, [-4000.0, 0.0, 0.0]);
    // One-way: the partner's XTEL leads to some third door, not 0x100.
    let far_side = door(0x900, [-4000.0, 0.0, 0.0], 0x777, [10.0, 20.0, 30.0]);
    let index = index_with_partner(far_side);

    assert_eq!(authored_spawn_pose(&[this_side], &index), None);
}

#[test]
fn an_unresolvable_partner_leaves_the_fallback_to_the_caller() {
    let this_side = door(0x100, [500.0, 600.0, 0.0], 0x900, [-4000.0, 0.0, 0.0]);
    let index = EsmCellIndex::default();

    assert_eq!(authored_spawn_pose(&[this_side], &index), None);
}

/// Bethesda headings are compass angles, clockwise from north (+Y): a
/// heading of 0 faces north, 90° faces east (+X). North is Y-up `-Z`;
/// east stays `+X`.
#[test]
fn forward_follows_the_bethesda_compass_heading() {
    let mut index = EsmCellIndex::default();
    index.statics.insert(COC_BASE, stat(COC_BASE, "COCMarkerHeading"));

    let north = [placed(0x300, COC_BASE, [0.0; 3], [0.0; 3])];
    let forward = authored_spawn_pose(&north, &index).unwrap().forward();
    assert!(forward.abs_diff_eq(-Vec3::Z, 1e-5), "heading 0 → {forward:?}");

    let east = [placed(0x300, COC_BASE, [0.0; 3], [0.0, 0.0, std::f32::consts::FRAC_PI_2])];
    let forward = authored_spawn_pose(&east, &index).unwrap().forward();
    assert!(forward.abs_diff_eq(Vec3::X, 1e-5), "heading 90° → {forward:?}");

    // FO4 `DmndDugoutInn01`'s COCMarkerHeading as authored: rz = 4.783 rad
    // (274°), beside the entrance door, with the room centroid at bearing
    // 269° — it faces west, into the inn.
    let west = [placed(0x300, COC_BASE, [0.0; 3], [0.0, 0.0, 4.783])];
    let forward = authored_spawn_pose(&west, &index).unwrap().forward();
    assert!(forward.x < -0.99, "heading 274° must face west (-X) → {forward:?}");
}

/// Only the heading carries over: an authored tilt must not start the camera
/// pitched into the floor or ceiling.
#[test]
fn forward_is_horizontal_for_a_tilted_marker() {
    let mut index = EsmCellIndex::default();
    index.statics.insert(COC_BASE, stat(COC_BASE, "COCMarkerHeading"));
    let tilted = [placed(0x300, COC_BASE, [0.0; 3], [0.3, 0.0, 1.0])];

    let forward = authored_spawn_pose(&tilted, &index).unwrap().forward();
    assert!(forward.y.abs() < 1e-6, "{forward:?}");
    assert!((forward.length() - 1.0).abs() < 1e-5, "{forward:?}");
}
