//! Real-data smoke test against vanilla `materialsbeta.cdb` from
//! `Starfield - Materials.ba2`. `#[ignore]`-gated by Starfield install.
//!
//! Run with:
//! ```
//! BYROREDUX_STARFIELD_DATA="/path/to/Starfield/Data" \
//!     cargo test -p byroredux-sfmaterial --test real_cdb -- --ignored --nocapture
//! ```

use byroredux_bsa::Ba2Archive;
use byroredux_sfmaterial::ComponentDatabaseFile;
use std::path::PathBuf;

fn data_dir() -> Option<PathBuf> {
    let env = std::env::var("BYROREDUX_STARFIELD_DATA").ok();
    let fallback = "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data";
    let path = env
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(fallback));
    if path.exists() {
        Some(path)
    } else {
        eprintln!("[sfmaterial] skip: Starfield data dir missing");
        None
    }
}

#[test]
#[ignore]
fn parse_vanilla_materialsbeta_cdb() {
    let Some(data) = data_dir() else {
        return;
    };
    let ba2 = Ba2Archive::open(data.join("Starfield - Materials.ba2")).expect("open materials BA2");
    let bytes = ba2
        .extract("materials\\materialsbeta.cdb")
        .expect("extract cdb");
    eprintln!("[sfmaterial] extracted {} bytes", bytes.len());

    let cdb = ComponentDatabaseFile::parse(&bytes).expect("parse cdb");
    eprintln!(
        "[sfmaterial] parsed: {} classes / {} instances",
        cdb.classes.len(),
        cdb.instances.len()
    );

    // Floor asserts — these should hold for any non-empty CDB.
    assert!(cdb.classes.len() > 0, "vanilla CDB must declare classes");
    assert!(
        cdb.instances.len() > 0,
        "vanilla CDB must contain instances"
    );

    // Spot-check that the first few class names look sensible (printable
    // ASCII) — a misaligned reader would print mojibake.
    for c in cdb.classes.iter().take(5) {
        let printable = c.name.chars().all(|ch| ch.is_ascii_graphic());
        assert!(printable, "class name not ASCII-printable: {:?}", c.name);
        eprintln!("  class[0..5] {} -> {}", c.type_id, c.name);
    }
}
