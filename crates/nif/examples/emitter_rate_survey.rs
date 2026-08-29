//! #3329 probe — how many emitter-bearing NIFs surface an authored rate.
use byroredux_bsa::BsaArchive;

fn main() {
    let bsa = std::env::args().nth(1).expect("usage: <archive>");
    let archive = BsaArchive::open(&bsa).unwrap();
    let (mut files, mut with_emitter, mut rate_some) = (0usize, 0usize, 0usize);
    let mut samples: Vec<(String, f32)> = Vec::new();
    for f in archive.list_files() {
        if !f.to_ascii_lowercase().ends_with(".nif") {
            continue;
        }
        let Ok(bytes) = archive.extract(f) else {
            continue;
        };
        let Ok(scene) = byroredux_nif::parse_nif(&bytes) else {
            continue;
        };
        files += 1;
        let mut pool = byroredux_core::string::StringPool::new();
        let imported = byroredux_nif::import::import_nif_scene(&scene, &mut pool);
        if imported.particle_emitters.is_empty() {
            continue;
        }
        with_emitter += 1;
        if let Some(r) = imported
            .particle_emitters
            .iter()
            .find_map(|e| e.emitter_rate)
        {
            rate_some += 1;
            if samples.len() < 400 {
                samples.push((f.to_string(), r));
            }
        }
    }
    println!("files={files} with_emitter={with_emitter} rate_some={rate_some}");
    for needle in [
        "fxambdust04",
        "snowglobes_nelis",
        "vgeardoor01",
        "dlc03crawlerdustexplosion",
        "fxhelios_charging",
        "dlc04fxcrashthroughfloor",
    ] {
        if let Some((n, r)) = samples
            .iter()
            .find(|(n, _)| n.to_ascii_lowercase().contains(needle))
        {
            println!("  {n}: rate={r}");
        }
    }
}
