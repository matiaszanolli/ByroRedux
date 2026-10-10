//! Diagnostic probe: parse a NIF and dump its imported ragdoll verbatim — bodies
//! (mass/damping/friction/restitution/shape/offset) and constraints (kind,
//! pivots, axes, limits) — for physics-stability investigation (#5161).
//!
//! Usage: ragdoll_dump <path.nif>

fn main() {
    let path = std::env::args().nth(1).expect("usage: ragdoll_dump <path.nif>");
    let bytes = std::fs::read(&path).expect("read");
    let scene = byroredux_nif::parse_nif(&bytes).expect("parse");
    let Some(ragdoll) = byroredux_nif::import::collision::extract_ragdoll(&scene) else {
        println!("no ragdoll");
        return;
    };
    println!(
        "bodies={} constraints={}",
        ragdoll.bodies.len(),
        ragdoll.constraints.len()
    );
    for (i, b) in ragdoll.bodies.iter().enumerate() {
        println!(
            "#{i} {:?} mass={} lin_damp={} ang_damp={} fric={} rest={} is_t={} t={:?} r={:?}",
            b.bone_name,
            b.mass,
            b.linear_damping,
            b.angular_damping,
            b.friction,
            b.restitution,
            b.is_t,
            b.translation,
            b.rotation,
        );
        println!("    shape={:?}", b.shape);
    }
    for (i, c) in ragdoll.constraints.iter().enumerate() {
        println!(
            "joint#{i} a={} b={} kind={:#?}",
            c.body_a, c.body_b, c.kind
        );
    }
}
