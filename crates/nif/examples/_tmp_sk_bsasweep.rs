//! TEMP scratch (audit 2026-08-16): full-archive extraction sweep for BSA v105.
use byroredux_bsa::BsaArchive;

fn main() {
    for path in std::env::args().skip(1) {
        let arc = match BsaArchive::open(&path) {
            Ok(a) => a,
            Err(e) => {
                println!("{path}: OPEN FAILED {e}");
                continue;
            }
        };
        let names: Vec<String> = arc
            .list_files()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        let mut ok = 0usize;
        let mut fail = 0usize;
        let mut bytes_total: u64 = 0;
        let mut first_errs: Vec<String> = Vec::new();
        for n in &names {
            match arc.extract(n) {
                Ok(b) => {
                    ok += 1;
                    bytes_total += b.len() as u64;
                }
                Err(e) => {
                    fail += 1;
                    if first_errs.len() < 5 {
                        first_errs.push(format!("{n}: {e}"));
                    }
                }
            }
        }
        println!(
            "{}: files={} ok={} fail={} bytes={:.1} MiB",
            path.rsplit('/').next().unwrap_or(&path),
            names.len(),
            ok,
            fail,
            bytes_total as f64 / 1048576.0
        );
        for e in &first_errs {
            println!("    ERR {e}");
        }
    }
}
