//! Per-emitter NIFAL particle dump — tabulate the authored
//! `NiPSysEmitter` base params the importer now captures, so the
//! canonical translate (authored params override the name-heuristic
//! preset) is wired against real numbers rather than assumptions
//! (no-guessing policy). Parallel to `material_dump.rs`.
//!
//! Usage:
//!   cargo run -p byroredux-nif --example emitter_dump -- <path.nif>

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: emitter_dump <path.nif>");
    let bytes = std::fs::read(&path).expect("read");
    let bsver = byroredux_nif::header::NifHeader::parse(&bytes)
        .map(|(h, _)| h.user_version_2)
        .unwrap_or(0);
    let scene = byroredux_nif::parse_nif(&bytes).expect("parse");
    let mut pool = byroredux_core::string::StringPool::new();
    let imported = byroredux_nif::import::import_nif_scene(&scene, &mut pool);

    println!("# {} (BSVER {})", path, bsver);
    println!(
        "{:>5} {:>6} {:>5} {:>6} {:>5} {:>6}  {:<22}  {}",
        "speed", "spdVar", "decl", "declVar", "life", "lifeVar", "initColor(rgba)", "host/type",
    );
    let mut any = false;
    for e in &imported.particle_emitters {
        let rate = e
            .emitter_rate
            .map(|r| format!("{r:.1}"))
            .unwrap_or_else(|| "-".to_string());
        let Some(p) = e.emitter_params else {
            if e.emitter_rate.is_some() {
                println!("(rate {rate}, no NiPSysEmitter base)");
                any = true;
            }
            continue;
        };
        any = true;
        let bscale = p
            .base_scale
            .map(|b| format!("{b:.2}"))
            .unwrap_or_else(|| "-".to_string());
        print!(
            "rate={rate:>6} radius={:>5.2} bscale={bscale:>5}  ",
            p.initial_radius
        );
        let host = imported
            .nodes
            .get(e.parent_node.unwrap_or(usize::MAX))
            .and_then(|n| n.name.as_ref())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{:>5.2} {:>6.2} {:>5.2} {:>6.2} {:>5.2} {:>6.2}  [{:.2},{:.2},{:.2},{:.2}]  {} / {}",
            p.speed,
            p.speed_variation,
            p.declination,
            p.declination_variation,
            p.life_span,
            p.life_span_variation,
            p.initial_color[0],
            p.initial_color[1],
            p.initial_color[2],
            p.initial_color[3],
            host,
            e.original_type,
        );
    }
    if !any {
        println!("(no NiPSysEmitter base params captured — legacy controller or no emitter)");
    }
}
