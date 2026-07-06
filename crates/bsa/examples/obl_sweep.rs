//! OBL-D2 audit sweep: open each Oblivion BSA, report version/flags, and
//! extract-sweep every file (focus: meshes/*.nif). Throwaway audit tool.

use byroredux_bsa::BsaArchive;

fn main() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .try_init();

    let mut total_files = 0usize;
    let mut total_ok = 0usize;
    let mut total_fail = 0usize;
    let mut nif_files = 0usize;
    let mut nif_ok = 0usize;
    let mut nif_fail = 0usize;

    for path in std::env::args().skip(1) {
        let archive = match BsaArchive::open(&path) {
            Ok(a) => a,
            Err(e) => {
                println!("OPEN FAIL {}: {}", path, e);
                continue;
            }
        };
        let files: Vec<String> = archive.list_files().iter().map(|s| s.to_string()).collect();
        let mut ok = 0usize;
        let mut fail = 0usize;
        let mut nok = 0usize;
        let mut nfail = 0usize;
        for f in &files {
            let is_nif = f.ends_with(".nif");
            match archive.extract(f) {
                Ok(_) => {
                    ok += 1;
                    if is_nif {
                        nok += 1;
                    }
                }
                Err(e) => {
                    fail += 1;
                    if is_nif {
                        nfail += 1;
                    }
                    if fail <= 5 {
                        println!("  EXTRACT FAIL {}: {}", f, e);
                    }
                }
            }
        }
        let base = std::path::Path::new(&path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&path);
        println!(
            "{:<45} ver={} files={} ok={} fail={} | nif={} nif_ok={} nif_fail={}",
            base,
            archive.version(),
            files.len(),
            ok,
            fail,
            nok + nfail,
            nok,
            nfail
        );
        total_files += files.len();
        total_ok += ok;
        total_fail += fail;
        nif_files += nok + nfail;
        nif_ok += nok;
        nif_fail += nfail;
    }
    println!(
        "\nTOTAL files={} ok={} fail={} ({:.4}%) | nif={} nif_ok={} nif_fail={} ({:.4}%)",
        total_files,
        total_ok,
        total_fail,
        100.0 * total_ok as f64 / total_files.max(1) as f64,
        nif_files,
        nif_ok,
        nif_fail,
        100.0 * nif_ok as f64 / nif_files.max(1) as f64,
    );
}
