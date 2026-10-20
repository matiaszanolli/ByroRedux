//! #5015 — real-corpus pin for the FO4 `bhkRagdollSystem` decode
//! ([`byroredux_nif::blocks::collision::decode_np_ragdoll`]).
//!
//! The layout was recovered from the vanilla corpus (there is no reflection
//! metadata in the blobs and no SDK is consulted), so the pins here are the
//! consistency checks it was derived from, run over every ragdoll skeleton:
//!
//! - every `bhkRagdollSystem` blob decodes;
//! - for every decoded joint, both of its bodies name the **same world point**
//!   for the joint's pivot at the bind pose (a mis-read offset, frame order or
//!   body index breaks this at once);
//! - every body of every skeleton decodes a shape — capsule, sphere, convex
//!   polytope or compound — and its mass equals the shape's volume times the
//!   density stored beside the inverse mass (#6067). The polytope layout is
//!   pinned by the 8-vertex boxes, whose volume with the convex radius folded in
//!   (the Steiner sum of an axis-aligned box) reproduces the authored mass;
//! - the human skeleton imports as 18 named bodies and 17 joints, with the
//!   masses (pelvis 18 kg, thighs 4 kg, hands 2 kg) that equal capsule volume
//!   times the density stored beside the inverse mass (checked to 1e-7 when
//!   the layout was derived).
//!
//! `#[ignore]` — needs Fallout 4 on disk; run with:
//! `cargo test -p byroredux-nif --test np_ragdoll_corpus -- --ignored --nocapture`

mod common;

use byroredux_nif::blocks::collision::{
    decode_np_ragdoll, BhkSystemBinary, NpConstraintData, NpRagdoll, NpShape,
};
use common::{open_all_mesh_archives, Game};

/// Rotate `v` by the quaternion `q = [x, y, z, w]`.
fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    let t = [
        2.0 * (y * v[2] - z * v[1]),
        2.0 * (z * v[0] - x * v[2]),
        2.0 * (x * v[1] - y * v[0]),
    ];
    [
        v[0] + w * t[0] + (y * t[2] - z * t[1]),
        v[1] + w * t[1] + (z * t[0] - x * t[2]),
        v[2] + w * t[2] + (x * t[1] - y * t[0]),
    ]
}

fn world_pivot(ragdoll: &NpRagdoll, body: usize, pivot: [f32; 4]) -> [f32; 3] {
    let b = &ragdoll.bodies[body];
    let r = rotate(b.rotation, [pivot[0], pivot[1], pivot[2]]);
    [
        b.position[0] + r[0],
        b.position[1] + r[1],
        b.position[2] + r[2],
    ]
}

/// Skeletons whose every joint must agree to a centimetre: the human rig and
/// the creatures an exterior spawns most. The machines and props that do not
/// (turrets, the vertibird, Liberty Prime's furniture, a few robot parts) author
/// joints whose pivots meet at a rest pose other than the stored bind pose —
/// 67 of the corpus's 850 joints, concentrated in those files.
const EXACT: &[&str] = &[
    "character\\characterassets\\skeleton.nif",
    "supermutant\\characterassets\\skeleton.nif",
    "feralghoul\\characterassets\\skeleton.nif",
    "brahmin\\characterassets\\skeleton.nif",
    "molerat\\characterassets\\skeleton.nif",
    "dogmeat\\characterassets\\skeleton.nif",
    "radscorpion\\characterassets\\skeleton.nif",
    "yaoguai\\characterassets\\skeleton.nif",
    "mirelurkking\\characterassets\\skeleton.nif",
    "radstag\\characterassets\\skeleton.nif",
];

#[test]
#[ignore = "needs Fallout 4 game data on disk"]
fn every_fo4_ragdoll_blob_decodes_with_consistent_joint_pivots() {
    let Some(archives) = open_all_mesh_archives(Game::Fallout4) else {
        return;
    };
    let (mut blobs, mut joints, mut disagreeing, mut skipped) = (0usize, 0usize, 0usize, 0usize);
    let mut failures: Vec<String> = Vec::new();
    for (_, archive) in &archives {
        for file in archive.list_files() {
            let lower = file.to_ascii_lowercase();
            if !byroredux_nif::corpus::is_nif_entry(&lower) || !lower.starts_with("meshes\\actors") {
                continue;
            }
            let Ok(bytes) = archive.extract(&file) else { continue };
            let Ok(scene) = byroredux_nif::parse_nif(&bytes) else { continue };
            for block in &scene.blocks {
                let Some(system) = block.as_any().downcast_ref::<BhkSystemBinary>() else {
                    continue;
                };
                if system.type_name != "bhkRagdollSystem" {
                    continue;
                }
                blobs += 1;
                let ragdoll = match decode_np_ragdoll(&system.data) {
                    Ok(r) => r,
                    Err(e) => {
                        failures.push(format!("{file}: does not decode: {e}"));
                        continue;
                    }
                };
                skipped += ragdoll.skipped_constraints;
                let must_agree = EXACT.iter().any(|name| lower.ends_with(name));
                for joint in &ragdoll.constraints {
                    let (a, b) = match &joint.data {
                        NpConstraintData::Ragdoll(r) => (r.pivot_a, r.pivot_b),
                        NpConstraintData::LimitedHinge(h) => (h.pivot_a, h.pivot_b),
                    };
                    let wa = world_pivot(&ragdoll, joint.body_a, a);
                    let wb = world_pivot(&ragdoll, joint.body_b, b);
                    let d = ((wa[0] - wb[0]).powi(2) + (wa[1] - wb[1]).powi(2) + (wa[2] - wb[2]).powi(2)).sqrt();
                    joints += 1;
                    // Metres; 1 cm.
                    if d > 0.01 {
                        disagreeing += 1;
                        if must_agree {
                            failures.push(format!(
                                "{file}: joint {} <-> {} pivots disagree by {d:.4} m at the bind pose",
                                joint.body_a, joint.body_b
                            ));
                        }
                    }
                }
            }
        }
    }
    eprintln!("[fo4] {blobs} ragdoll blobs, {joints} joints decoded ({skipped} skipped), {disagreeing} disagree by more than 1 cm");
    // Every joint row is a ragdoll or limited-hinge joint, behind an `hknpBreakableConstraintData`
    // wrapper on three of them (#6067).
    assert_eq!(skipped, 0, "every vanilla joint decodes");
    assert!(blobs >= 40, "the vanilla corpus carries dozens of ragdoll skeletons, found {blobs}");
    assert!(failures.is_empty(), "{} failure(s):\n{}", failures.len(), failures.join("\n"));
    // Measured 2026-10-19: 783 of 850 agree (92.1 %). A decode that misreads
    // offsets collapses this far below the machines-and-props residue.
    assert!(
        disagreeing * 100 <= joints * 15,
        "{disagreeing} of {joints} joints disagree — the layout has drifted"
    );
}

#[test]
#[ignore = "needs Fallout 4 game data on disk"]
fn the_human_skeleton_imports_as_eighteen_named_bodies_and_seventeen_joints() {
    let Some(archives) = open_all_mesh_archives(Game::Fallout4) else {
        return;
    };
    let bytes = archives
        .iter()
        .find_map(|(_, a)| a.extract("meshes\\actors\\character\\characterassets\\skeleton.nif").ok())
        .expect("the human skeleton");
    let scene = byroredux_nif::parse_nif(&bytes).expect("parses");
    let ragdoll = byroredux_nif::import::collision::extract_ragdoll(&scene).expect("a ragdoll");
    assert_eq!(ragdoll.bodies.len(), 18);
    assert_eq!(ragdoll.constraints.len(), 17);
    let names: Vec<&str> = ragdoll.bodies.iter().map(|b| &*b.bone_name).collect();
    for expected in ["COM", "LLeg_Thigh", "RLeg_Calf", "SPINE2", "Chest", "Neck", "HEAD", "LArm_Hand", "RArm_ForeArm1"] {
        assert!(names.contains(&expected), "{expected} missing from {names:?}");
    }
    // Pelvis 18 kg, thighs 4 kg, hands 2 kg — the values the volume×density
    // check reproduced to 1e-7 when the layout was derived.
    let mass = |bone: &str| ragdoll.bodies.iter().find(|b| &*b.bone_name == bone).unwrap().mass;
    assert!((mass("COM") - 18.0).abs() < 1e-3);
    assert!((mass("LLeg_Thigh") - 4.0).abs() < 1e-3);
    assert!((mass("LArm_Hand") - 2.0).abs() < 1e-3);
    // Every body of this skeleton sits on its bone.
    assert!(ragdoll.bodies.iter().all(|b| !b.is_t));
}

/// The volume of a shape whose volume has a closed form: a capsule, a sphere, or
/// an axis-aligned box hull rounded by its convex radius. Empty for the organic
/// hulls (20–252 vertices), whose rounded volume has no closed form here.
///
/// A box hull's authored volume is one of two: the sphere-rounded (Steiner) sum,
/// or — on the mirelurk hunter's 3 cm box, whose mass is a round 5 kg — the box
/// grown by the radius on every side. Both are returned.
fn closed_form_volumes(shape: &NpShape) -> Vec<f64> {
    use std::f64::consts::PI;
    match shape {
        NpShape::Sphere { radius } => vec![4.0 / 3.0 * PI * (*radius as f64).powi(3)],
        NpShape::Capsule { a, b, radius } => {
            let length = (0..3).map(|k| ((a[k] - b[k]) as f64).powi(2)).sum::<f64>().sqrt();
            let r = *radius as f64;
            vec![PI * r * r * length + 4.0 / 3.0 * PI * r.powi(3)]
        }
        NpShape::ConvexHull { vertices, radius } if vertices.len() == 8 => {
            let mut lo = [f64::MAX; 3];
            let mut hi = [f64::MIN; 3];
            for v in vertices {
                for k in 0..3 {
                    lo[k] = lo[k].min(v[k] as f64);
                    hi[k] = hi[k].max(v[k] as f64);
                }
            }
            // An axis-aligned box has every vertex on a bounding-box corner.
            let on_corner = |v: &[f32; 3]| (0..3).all(|k| (v[k] as f64 - lo[k]).abs() < 1e-5 || (v[k] as f64 - hi[k]).abs() < 1e-5);
            let mut corners: Vec<[bool; 3]> = vertices
                .iter()
                .map(|v| [0, 1, 2].map(|k| (v[k] as f64 - hi[k]).abs() < 1e-5))
                .collect();
            corners.sort();
            corners.dedup();
            // …and all eight corners are present (a collapsed prism is not a box).
            if !vertices.iter().all(on_corner) || corners.len() != 8 {
                return Vec::new();
            }
            let [a, b, c] = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
            let r = *radius as f64;
            vec![
                a * b * c + 2.0 * r * (a * b + b * c + c * a) + PI * r * r * (a + b + c) + 4.0 / 3.0 * PI * r.powi(3),
                (a + 2.0 * r) * (b + 2.0 * r) * (c + 2.0 * r),
            ]
        }
        _ => Vec::new(),
    }
}

#[test]
#[ignore = "needs Fallout 4 game data on disk"]
fn every_fo4_ragdoll_body_has_a_shape_whose_volume_times_density_is_its_mass() {
    let Some(archives) = open_all_mesh_archives(Game::Fallout4) else {
        return;
    };
    let (mut bodies, mut shapeless, mut hulls, mut compounds) = (0usize, 0usize, 0usize, 0usize);
    let (mut checked, mut max_hull) = (0usize, 0usize);
    let mut off_mass: Vec<String> = Vec::new();
    for (_, archive) in &archives {
        for file in archive.list_files() {
            let lower = file.to_ascii_lowercase();
            if !byroredux_nif::corpus::is_nif_entry(&lower) || !lower.starts_with("meshes\\actors") {
                continue;
            }
            let Ok(bytes) = archive.extract(&file) else { continue };
            let Ok(scene) = byroredux_nif::parse_nif(&bytes) else { continue };
            for block in &scene.blocks {
                let Some(system) = block.as_any().downcast_ref::<BhkSystemBinary>() else { continue };
                if system.type_name != "bhkRagdollSystem" {
                    continue;
                }
                let Ok(ragdoll) = decode_np_ragdoll(&system.data) else { continue };
                for body in &ragdoll.bodies {
                    bodies += 1;
                    let Some(shape) = &body.shape else {
                        shapeless += 1;
                        eprintln!("[fo4] {file}: body {} has no decoded shape", body.index);
                        continue;
                    };
                    match shape {
                        NpShape::ConvexHull { vertices, .. } => {
                            hulls += 1;
                            max_hull = max_hull.max(vertices.len());
                        }
                        NpShape::Compound { .. } => compounds += 1,
                        _ => {}
                    }
                    let volumes = closed_form_volumes(shape);
                    if !volumes.is_empty() {
                        checked += 1;
                        let ratios: Vec<f64> = volumes.iter().map(|v| body.mass as f64 / (v * body.density as f64)).collect();
                        if !ratios.iter().any(|r| (r - 1.0).abs() <= 0.02) {
                            off_mass.push(format!("{file}: body {} mass/(volume x density) = {ratios:.4?}", body.index));
                        }
                    }
                }
            }
        }
    }
    eprintln!("[fo4] {bodies} bodies: {hulls} hulls (max {max_hull} vertices), {compounds} compounds, {shapeless} without a shape; {checked} masses checked");
    assert_eq!(shapeless, 0, "every vanilla ragdoll body decodes a shape");
    assert!(off_mass.is_empty(), "{} mass(es) off:\n{}", off_mass.len(), off_mass.join("\n"));
    // Measured 2026-10-19: 986 bodies, 58 hulls, 6 compounds, 252-vertex maximum;
    // 922 capsules/spheres plus the 8-vertex boxes have a closed-form volume.
    assert!(bodies >= 980, "{bodies} bodies");
    assert!(hulls >= 50 && compounds >= 5, "{hulls} hulls, {compounds} compounds");
    assert!(checked >= 940, "only {checked} masses had a closed-form volume to check");
}

/// The left/right mirror of a bone name (`LLeg_Thigh` ↔ `RLeg_Thigh`, `LArm…`,
/// `…Left…`), or `None` for a bone with no mirror.
fn mirrored(name: &str) -> Option<String> {
    for (a, b) in [
        ("LLeg", "RLeg"),
        ("RLeg", "LLeg"),
        ("LArm", "RArm"),
        ("RArm", "LArm"),
        ("_L_", "_R_"),
        ("_R_", "_L_"),
        ("Left", "Right"),
        ("Right", "Left"),
        ("LFront", "RFront"),
        ("RFront", "LFront"),
        ("LRear", "RRear"),
        ("RRear", "LRear"),
    ] {
        if name.contains(a) {
            return Some(name.replacen(a, b, 1));
        }
    }
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some('L'), Some(c)) if c.is_uppercase() => Some(format!("R{}", &name[1..])),
        (Some('R'), Some(c)) if c.is_uppercase() => Some(format!("L{}", &name[1..])),
        _ => None,
    }
}

/// #6066 — friction, restitution and damping, read from the material and
/// motion-property rows and identified by varying evidence over every vanilla
/// skeleton:
///
/// - every one of the 986 bodies has readable rows, and the material's two
///   friction halves are equal on all of them;
/// - the body each of a handful of skeletons leaves at the authoring tool's
///   defaults reads friction 0.3 / restitution 0.8 (the Skyrim `bhkRigidBody`
///   defaults) *and* damping 0.1 linear / 0.05 angular — the pairing that pins
///   which damping field is which (84 of the 88 default-material bodies carry
///   it, none carries the reverse);
/// - the rows follow the body's motion index: left/right mirror bones carry
///   identical rows in 562 of 566 pairs under it, 514 under the body index.
///
/// Measured 2026-10-20.
#[test]
#[ignore = "needs Fallout 4 game data on disk"]
fn every_fo4_ragdoll_body_reads_friction_restitution_and_damping() {
    let Some(archives) = open_all_mesh_archives(Game::Fallout4) else {
        return;
    };
    let near = |a: f32, b: f32| (a - b).abs() < 0.01;
    let (mut bodies, mut readable, mut unequal_friction) = (0usize, 0usize, 0usize);
    let (mut default_material, mut default_damping_forward, mut default_damping_reverse) = (0usize, 0usize, 0usize);
    let (mut pairs, mut agreeing) = (0usize, 0usize);
    for (_, archive) in &archives {
        for file in archive.list_files() {
            let lower = file.to_ascii_lowercase();
            if !byroredux_nif::corpus::is_nif_entry(&lower) || !lower.starts_with("meshes\\actors") {
                continue;
            }
            let Ok(bytes) = archive.extract(&file) else { continue };
            let Ok(scene) = byroredux_nif::parse_nif(&bytes) else { continue };
            for block in &scene.blocks {
                let Some(system) = block.as_any().downcast_ref::<BhkSystemBinary>() else { continue };
                if system.type_name != "bhkRagdollSystem" {
                    continue;
                }
                let Ok(ragdoll) = decode_np_ragdoll(&system.data) else { continue };
                for body in &ragdoll.bodies {
                    bodies += 1;
                    let Some(p) = body.properties else { continue };
                    readable += 1;
                    if p.friction != p.static_friction {
                        unequal_friction += 1;
                    }
                    if near(p.friction, 0.3) && near(p.restitution, 0.8) {
                        default_material += 1;
                        if near(p.linear_damping, 0.1) && near(p.angular_damping, 0.05) {
                            default_damping_forward += 1;
                        }
                        if near(p.linear_damping, 0.05) && near(p.angular_damping, 0.1) {
                            default_damping_reverse += 1;
                        }
                    }
                }
            }
            // Mirror bones of one skeleton carry the same rows (the import names bones).
            if let Some(imported) = byroredux_nif::import::collision::extract_ragdoll(&scene) {
                let by_name: std::collections::HashMap<&str, [f32; 4]> = imported
                    .bodies
                    .iter()
                    .map(|b| (&*b.bone_name, [b.friction, b.restitution, b.linear_damping, b.angular_damping]))
                    .collect();
                for (name, tuple) in &by_name {
                    if let Some(other) = mirrored(name).and_then(|m| by_name.get(m.as_str()).copied()) {
                        pairs += 1;
                        agreeing += usize::from(other == *tuple);
                    }
                }
            }
        }
    }
    eprintln!(
        "[fo4] {bodies} bodies, {readable} with readable rows, {unequal_friction} with unequal friction halves; \
         {default_material} default-material bodies, {default_damping_forward} with damping 0.1/0.05 and \
         {default_damping_reverse} with the reverse; {agreeing} of {pairs} mirror pairs identical"
    );
    assert!(bodies >= 980, "{bodies} bodies");
    assert_eq!(readable, bodies, "every vanilla body has readable material and damping rows");
    assert_eq!(unequal_friction, 0, "dynamic and static friction are equal on every body");
    assert!(default_material >= 80, "{default_material} default-material bodies");
    assert_eq!(default_damping_reverse, 0, "no default-material body carries the reversed pair");
    assert!(
        default_damping_forward * 100 >= default_material * 90,
        "{default_damping_forward} of {default_material} default-material bodies carry linear 0.1 / angular 0.05"
    );
    assert!(pairs >= 400 && agreeing * 100 >= pairs * 97, "{agreeing} of {pairs} mirror pairs are identical");
}

/// #6066 — the human rig's values (identical on all 18 bodies) and a creature
/// rig's, where one body keeps the defaults.
#[test]
#[ignore = "needs Fallout 4 game data on disk"]
fn the_human_and_deathclaw_rigs_carry_their_authored_values() {
    let Some(archives) = open_all_mesh_archives(Game::Fallout4) else {
        return;
    };
    let import = |path: &str| {
        let bytes = archives
            .iter()
            .find_map(|(_, a)| a.extract(path).ok())
            .unwrap_or_else(|| panic!("{path}"));
        let scene = byroredux_nif::parse_nif(&bytes).expect("parses");
        byroredux_nif::import::collision::extract_ragdoll(&scene).expect("a ragdoll")
    };
    let close = |a: f32, b: f32| (a - b).abs() < 0.005;

    let human = import("meshes\\actors\\character\\characterassets\\skeleton.nif");
    assert_eq!(human.bodies.len(), 18);
    for b in &human.bodies {
        assert!(
            close(b.friction, 0.6) && close(b.restitution, 0.2) && close(b.linear_damping, 0.4) && close(b.angular_damping, 0.6),
            "{}: friction {} restitution {} damping {} / {}",
            b.bone_name,
            b.friction,
            b.restitution,
            b.linear_damping,
            b.angular_damping
        );
    }

    let deathclaw = import("meshes\\actors\\deathclaw\\characterassets\\skeleton.nif");
    let defaults: Vec<&str> = deathclaw
        .bodies
        .iter()
        .filter(|b| close(b.friction, 0.3) && close(b.restitution, 0.8))
        .map(|b| &*b.bone_name)
        .collect();
    assert_eq!(defaults.len(), 1, "one body keeps the authoring defaults: {defaults:?}");
    let kept = deathclaw.bodies.iter().find(|b| &*b.bone_name == defaults[0]).unwrap();
    assert!(close(kept.linear_damping, 0.1) && close(kept.angular_damping, 0.05));
    assert!(
        deathclaw.bodies.iter().filter(|b| &*b.bone_name != defaults[0]).all(|b| close(b.friction, 0.6) && close(b.restitution, 0.2)),
        "the other 27 carry the creature values"
    );
}
