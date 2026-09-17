//! Inspect the production LSCR decoder against installed game data.
//! Usage: probe_load_screens <plugin.esm> [editor-id substring]
use byroredux_plugin::esm::records::{parse_esm, StringsTableGuard};
use byroredux_plugin::esm::strings_table::StringTableSet;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or_else(|| {
        anyhow::anyhow!("usage: probe_load_screens <plugin.esm> [editor-id substring]")
    })?;
    let needle = args.next().unwrap_or_default().to_ascii_lowercase();
    let path = std::path::Path::new(&path);
    let _strings = StringsTableGuard::new(StringTableSet::load(path, "english"));
    let index = parse_esm(&std::fs::read(path)?)?;
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
