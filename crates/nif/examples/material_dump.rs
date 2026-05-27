//! Per-mesh canonical-material dump. Used to ground the "full canonical
//! material pass" (2026-05-27) — tabulate what each engine version
//! actually produces for metalness / roughness / material_kind / glass
//! signals on equivalent surfaces, so the canonical convention is
//! designed against real numbers rather than assumptions.
//!
//! Usage:
//!   cargo run -p byroredux-nif --example material_dump -- <path.nif>

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: material_dump <path.nif>");
    let bytes = std::fs::read(&path).expect("read");
    let bsver = byroredux_nif::header::NifHeader::parse(&bytes)
        .map(|(h, _)| h.user_version_2)
        .unwrap_or(0);
    let scene = byroredux_nif::parse_nif(&bytes).expect("parse");
    let mut pool = byroredux_core::string::StringPool::new();
    let imported = byroredux_nif::import::import_nif(&scene, &mut pool);

    println!("# {} (BSVER {})  — {} meshes", path, bsver, imported.len());
    println!(
        "{:<22} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>6} {:>5} {:>5} {:>5}  {}",
        "mesh", "kind", "metO", "rghO", "gloss", "env", "specS", "specClum", "emisM", "alpha",
        "decal", "tex/mat path",
    );
    for m in &imported {
        let name = m
            .name
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "-".to_string());
        let tex = m
            .texture_path
            .and_then(|s| pool.resolve(s))
            .map(str::to_string)
            .or_else(|| m.material_path.and_then(|s| pool.resolve(s)).map(str::to_string))
            .unwrap_or_else(|| "(none)".to_string());
        let meto = m
            .metalness_override
            .map(|v| format!("{:.2}", v))
            .unwrap_or_else(|| "-".to_string());
        let rgho = m
            .roughness_override
            .map(|v| format!("{:.2}", v))
            .unwrap_or_else(|| "-".to_string());
        // Specular-color luminance (Rec.709) — a near-1.0 value with high
        // specular_strength is the legacy metal hint we currently ignore.
        let sc = m.specular_color;
        let spec_lum = 0.2126 * sc[0] + 0.7152 * sc[1] + 0.0722 * sc[2];
        println!(
            "{:<22.22} {:>5} {:>5} {:>5} {:>5.0} {:>5.2} {:>5.2} {:>6.2} {:>6.1} {:>5} {:>5}  {}",
            name,
            m.material_kind,
            meto,
            rgho,
            m.glossiness,
            m.env_map_scale,
            m.specular_strength,
            spec_lum,
            m.emissive_mult,
            m.has_alpha as u8,
            m.is_decal as u8,
            tex,
        );
    }
}
