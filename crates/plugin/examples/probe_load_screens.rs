//! Inspect the production LSCR decoder against installed game data.
//! Usage: probe_load_screens <plugin.esm> [--census] [editor-id substring]
//!
//! `--census` prints the cover-eligibility census (the same verdicts the
//! transition cover and the `loadscreen.census` console command use) instead
//! of record dumps. Without archive-backed strings every localized tip reads
//! as a `<lstring …>` placeholder, so run the census on the installed master
//! with `BYRO_ARCHIVE_STRINGS=1` (or accept that TipUnresolved dominates).
use byroredux_plugin::esm::records::{
    first_backend_load_screen, load_screen_census, parse_esm, StringsTableGuard,
};
use byroredux_plugin::esm::strings_table::StringTableSet;

fn main() -> anyhow::Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let census = args.iter().any(|a| a == "--census");
    args.retain(|a| a != "--census");
    let path = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("usage: probe_load_screens <plugin.esm> [--census] [edid]"))?;
    let needle = args.get(1).cloned().unwrap_or_default().to_ascii_lowercase();
    let path = std::path::Path::new(&path);
    // Census runs against the same archive-backed string source the engine
    // uses when one is discoverable next to the plugin, so TipUnresolved
    // counts reflect runtime rather than the bare-loose-file floor.
    let tables = if std::env::var_os("BYRO_ARCHIVE_STRINGS").is_some() {
        StringTableSet::load_with_archive(path, "english", |relative| {
            let data_dir = path.parent().unwrap_or(std::path::Path::new("."));
            let interface_bsa = data_dir.join("Skyrim - Interface.bsa");
            if let Ok(archive) = byroredux_bsa::BsaArchive::open(&interface_bsa) {
                if let Ok(extracted) = archive.extract(relative) {
                    return Some(extracted);
                }
            }
            let interface_ba2 = data_dir.join("Fallout4 - Interface.ba2");
            if let Ok(archive) = byroredux_bsa::Ba2Archive::open(&interface_ba2) {
                if let Ok(extracted) = archive.extract(relative) {
                    return Some(extracted);
                }
            }
            None
        })
    } else {
        StringTableSet::load(path, "english")
    };
    let _strings = StringsTableGuard::new(tables);
    let index = parse_esm(&std::fs::read(path)?)?;
    if census {
        let c = load_screen_census(&index);
        println!(
            "LSCR total={} eligible={} rejected{{conditions={}, locations={}, malformed={}, \
             tip_unresolved={}, missing_artwork={}}} trns_total={}",
            c.total,
            c.eligible,
            c.rejected_conditions,
            c.rejected_locations,
            c.rejected_malformed,
            c.rejected_tip_unresolved,
            c.rejected_missing_artwork,
            index.load_screen_transforms.len(),
        );
        if let Some((screen, verdict)) = first_backend_load_screen(&index) {
            println!(
                "cover pick: {:08X} {} model={:?}",
                screen.form_id,
                screen.editor_id,
                match verdict {
                    byroredux_plugin::esm::records::LoadScreenVerdict::Model(Some(m)) => {
                        m.model_path
                    }
                    byroredux_plugin::esm::records::LoadScreenVerdict::Model(None) => {
                        screen.icon.clone()
                    }
                    _ => String::new(),
                }
            );
        } else {
            println!("cover pick: NONE");
        }
        return Ok(());
    }
    let mut records: Vec<_> = index
        .load_screens
        .values()
        .filter(|r| r.editor_id.to_ascii_lowercase().contains(&needle))
        .collect();
    records.sort_by_key(|r| r.form_id);
    anyhow::ensure!(!records.is_empty(), "no matching LSCR records");
    println!(
        "LSCR total={} matched={} malformed={} unresolved_tips={}",
        index.load_screens.len(),
        records.len(),
        records
            .iter()
            .filter(|r| !r.malformed_fields.is_empty())
            .count(),
        records
            .iter()
            .filter(|r| r.description.starts_with("<lstring "))
            .count(),
    );
    for record in records.iter().take(10) {
        println!("{record:#?}");
    }
    anyhow::ensure!(
        records.iter().all(|r| r.malformed_fields.is_empty()),
        "malformed LSCR presentation fields"
    );
    Ok(())
}
