//! TEMP scratch: D7 — validate the #2336 root-space→bone-local conversion
//! against real FNV data, and report bhkRigidBodyT vs bhkRigidBody.
use byroredux_bsa::BsaArchive;
use byroredux_core::ecs::GlobalTransform;
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_nif::blocks::collision::{BhkCollisionObject, BhkRigidBody};
use byroredux_nif::import::import_nif_scene;
use byroredux_nif::parse_nif;
use std::collections::HashMap;

fn main() {
    let bsa = std::env::args().nth(1).unwrap();
    let skel = std::env::args().nth(2).unwrap();
    let arc = BsaArchive::open(&bsa).unwrap();
    let bytes = arc.extract(&skel).unwrap();
    let scene = parse_nif(&bytes).unwrap();

    // is_t census on the rigid bodies actually hosted by a bone.
    let mut hosted: HashMap<usize, String> = HashMap::new();
    for b in scene.blocks.iter() {
        if let Some(av) = b.as_av_object() {
            if let Some(ci) = av.collision_ref().index() {
                if let Some(co) = scene.get_as::<BhkCollisionObject>(ci) {
                    if let Some(bi) = co.body_ref.index() {
                        hosted.insert(bi, av.name_arc().map(|s| s.to_string()).unwrap_or_default());
                    }
                }
            }
        }
    }
    let mut t_count = 0;
    let mut plain = 0;
    for (i, b) in scene.blocks.iter().enumerate() {
        if let Some(rb) = b.as_any().downcast_ref::<BhkRigidBody>() {
            if hosted.contains_key(&i) {
                if rb.is_t {
                    t_count += 1
                } else {
                    plain += 1
                }
            }
        }
    }
    println!("hosted rigid bodies: is_t={t_count} plain={plain}");

    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    let rag = imported.ragdoll.as_ref().unwrap();

    // Rest poses, mirroring nif_loader.rs.
    let mut rest: Vec<GlobalTransform> = Vec::with_capacity(imported.nodes.len());
    let mut rest_by_name: HashMap<String, GlobalTransform> = HashMap::new();
    for nd in &imported.nodes {
        let q = Quat::from_xyzw(
            nd.rotation[0],
            nd.rotation[1],
            nd.rotation[2],
            nd.rotation[3],
        );
        let t = Vec3::new(nd.translation[0], nd.translation[1], nd.translation[2]);
        let local = GlobalTransform {
            translation: t,
            rotation: q,
            scale: nd.scale,
        };
        let g = nd
            .parent_node
            .and_then(|p| rest.get(p))
            .map(|p| GlobalTransform::compose(p, t, q, nd.scale))
            .unwrap_or(local);
        if let Some(nm) = nd.name.as_ref() {
            rest_by_name.entry(nm.to_string()).or_insert(g);
        }
        rest.push(g);
    }

    println!(
        "{:<20} {:>28} {:>28} {:>10}",
        "bone", "body root pose", "bone rest pose", "|local_t|"
    );
    let mut max_local = 0.0f32;
    for b in &rag.bodies {
        let r = rest_by_name.get(b.bone_name.as_ref()).unwrap();
        let inv = r.rotation.inverse();
        let local_t = inv * (b.translation - r.translation) / r.scale;
        max_local = max_local.max(local_t.length());
        println!(
            "{:<20} {:>8.2},{:>8.2},{:>8.2} {:>8.2},{:>8.2},{:>8.2} {:>10.2}",
            b.bone_name,
            b.translation.x,
            b.translation.y,
            b.translation.z,
            r.translation.x,
            r.translation.y,
            r.translation.z,
            local_t.length()
        );
    }
    println!("max |local_translation| = {max_local:.2}");
}
