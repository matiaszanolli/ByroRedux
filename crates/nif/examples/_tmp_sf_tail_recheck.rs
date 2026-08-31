//! Throwaway (#3474 recheck, 2026-08-31): re-measure starfield_tail length
//! over `Starfield - LODMeshes.ba2`, the archive the field doc cites.
use byroredux_bsa::Ba2Archive;
use byroredux_nif::blocks::shader::BSLightingShaderProperty;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut lens: std::collections::BTreeMap<usize, u64> = Default::default();
    let mut sample_bytes: Option<Vec<u8>> = None;

    for path in &args {
        let Ok(archive) = Ba2Archive::open(path) else {
            eprintln!("skip {path}");
            continue;
        };
        let names: Vec<String> = archive
            .list_files()
            .into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
            .map(|s| s.to_string())
            .collect();
        eprintln!("{path}: {} nifs", names.len());
        for name in names {
            let Ok(bytes) = archive.extract(&name) else {
                continue;
            };
            let Ok(scene) = byroredux_nif::parse_nif(&bytes) else {
                continue;
            };
            if scene.bsver < 172 {
                continue;
            }
            for i in 0..scene.blocks.len() {
                if let Some(s) = scene.get_as::<BSLightingShaderProperty>(i) {
                    let len = s.starfield_tail.len();
                    *lens.entry(len).or_insert(0) += 1;
                    if len > 0 && sample_bytes.is_none() {
                        sample_bytes = Some(s.starfield_tail.clone());
                    }
                }
            }
        }
    }
    println!("starfield_tail length histogram: {lens:?}");
    if let Some(b) = sample_bytes {
        println!("sample tail bytes ({} B): {:?}", b.len(), b);
    }
}
