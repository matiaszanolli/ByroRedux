//! Census the stealth / detection game-setting family each master authors
//! (M65 Phase 0, #6909; PRD `docs/prd/sneak-and-detection.md` §7.2 scenario 3).
//!
//! Prints one tab-separated line per authored `GMST` whose editor ID belongs to
//! the family: `<master>\t<editor id>\t<value>`. A setting a master does not
//! author is absent from the output; its value then comes from the documented
//! default (the CS / GECK / CK settings pages) or the executable's default
//! table, which this tool does not read.
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example stealth_gmst_census -- <ESM> [...]

use byroredux_plugin::esm::parse_esm;
use byroredux_plugin::esm::records::SettingValue;

/// Lower-case substrings that put a setting in the family. Broad on purpose:
/// the census is the input the verify items read, and a false positive costs
/// one table row.
const FAMILY: &[&str] = &[
    "sneak",
    "detect",
    "stealth",
    "lightlevel",
    "viewcone",
    "aicombatmin",
    "soundlevel",
    "soundlos",
];

fn main() -> anyhow::Result<()> {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        anyhow::bail!("usage: stealth_gmst_census <ESM> [...]");
    }
    for path in paths {
        let master = std::path::Path::new(&path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        let bytes = std::fs::read(&path)?;
        let index = parse_esm(&bytes)?;
        let mut rows: Vec<(String, String)> = index
            .game_settings
            .values()
            .filter(|setting| {
                let id = setting.editor_id.to_ascii_lowercase();
                FAMILY.iter().any(|needle| id.contains(needle))
            })
            .map(|setting| {
                let value = match &setting.value {
                    SettingValue::Int(v) => v.to_string(),
                    SettingValue::Float(v) => v.to_string(),
                    SettingValue::Short(v) => v.to_string(),
                    SettingValue::String(v) => format!("{v:?}"),
                    #[allow(unreachable_patterns)]
                    other => format!("{other:?}"),
                };
                (setting.editor_id.clone(), value)
            })
            .collect();
        rows.sort_by(|a, b| a.0.to_ascii_lowercase().cmp(&b.0.to_ascii_lowercase()));
        for (id, value) in rows {
            println!("{master}\t{id}\t{value}");
        }
    }
    Ok(())
}
