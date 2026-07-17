//! Full-archive extraction sweep for Skyrim SE v105 BSAs (Dim 5 audit).
//! Opens each named archive and extracts every file, counting errors.

use byroredux_bsa::BsaArchive;
use std::time::Instant;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let data_dir = "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data";
    let archives = [
        "Skyrim - Meshes0.bsa",
        "Skyrim - Meshes1.bsa",
        "Skyrim - Textures0.bsa",
        "Skyrim - Textures1.bsa",
        "Skyrim - Textures2.bsa",
        "Skyrim - Textures3.bsa",
        "Skyrim - Textures4.bsa",
        "Skyrim - Textures5.bsa",
        "Skyrim - Textures6.bsa",
        "Skyrim - Textures7.bsa",
        "Skyrim - Textures8.bsa",
    ];

    let mut grand_total = 0usize;
    let mut grand_errors = 0usize;

    for name in archives {
        let path = format!("{}/{}", data_dir, name);
        let t0 = Instant::now();
        let archive = match BsaArchive::open(&path) {
            Ok(a) => a,
            Err(e) => {
                println!("{}: OPEN FAILED: {}", name, e);
                grand_errors += 1;
                continue;
            }
        };
        let version = archive.version();
        let files: Vec<String> = archive.list_files().iter().map(|s| s.to_string()).collect();
        let total = files.len();
        let mut errors = 0usize;
        let mut magic_mismatches = 0usize;
        let mut total_bytes: u64 = 0;
        let mut first_errors: Vec<String> = Vec::new();
        for f in &files {
            match archive.extract(f) {
                Ok(data) => {
                    total_bytes += data.len() as u64;
                    // Content-level sanity check beyond "extract didn't error":
                    // verify magic bytes for the two dominant content types.
                    let lower = f.to_ascii_lowercase();
                    let ok = if lower.ends_with(".nif") || lower.ends_with(".kf") {
                        data.len() >= 4 && &data[..4] == b"Game"
                    } else if lower.ends_with(".dds") {
                        data.len() >= 4 && &data[..4] == b"DDS "
                    } else {
                        true // no magic check for other extensions
                    };
                    if !ok {
                        magic_mismatches += 1;
                        if first_errors.len() < 10 {
                            first_errors.push(format!(
                                "{}: bad magic {:?}",
                                f,
                                &data[..8.min(data.len())]
                            ));
                        }
                    }
                }
                Err(e) => {
                    errors += 1;
                    if first_errors.len() < 10 {
                        first_errors.push(format!("{}: {}", f, e));
                    }
                }
            }
        }
        let elapsed = t0.elapsed();
        println!(
            "{name}: v{version} files={total} errors={errors} magic_mismatches={magic_mismatches} bytes={:.1}MB time={:.2}s",
            total_bytes as f64 / 1_000_000.0,
            elapsed.as_secs_f64()
        );
        if !first_errors.is_empty() {
            for e in &first_errors {
                println!("    ERROR: {}", e);
            }
        }
        grand_errors += magic_mismatches;
        grand_total += total;
        grand_errors += errors;
    }

    println!("\n=== TOTAL: {} files, {} errors ===", grand_total, grand_errors);
    if grand_errors > 0 {
        std::process::exit(1);
    }
}
