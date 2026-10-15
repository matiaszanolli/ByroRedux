//! Census the Papyrus-variable condition functions — `GetVMQuestVariable`
//! (629) and `GetVMScriptVariable` (630 Skyrim / 660 FO4) — across a master:
//! which record types author them, and the variable-name spelling their
//! `CIS2` carries (the backing `::Name_var` form or the bare property name).
//!
//! Evidence harness for M47.2 quest script variables: the runtime key a
//! fragment write (`kmyQuest.Name = 1`) lands under must be the key the
//! condition reads.
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example ctda_variable_census -- <ESM> [...]

use std::collections::BTreeMap;

use byroredux_plugin::esm::reader::EsmReader;

#[derive(Default)]
struct Stats {
    by_record: BTreeMap<(u32, String), usize>,
    spellings: BTreeMap<(u32, &'static str), usize>,
    samples: BTreeMap<u32, Vec<String>>,
    missing_cis2: usize,
}

fn main() -> anyhow::Result<()> {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path)?;
        let mut reader = EsmReader::new(&bytes);
        let _header = reader.read_file_header()?;
        let mut stats = Stats::default();
        // No `want` filter: every record type can author these CTDAs.
        reader.for_each_record(bytes.len(), None, |header, subs| {
            let kind = String::from_utf8_lossy(&header.record_type).to_string();
            for (i, sub) in subs.iter().enumerate() {
                if &sub.sub_type != b"CTDA" || sub.data.len() < 12 {
                    continue;
                }
                let function = u32::from_le_bytes(sub.data[8..12].try_into().unwrap());
                if !matches!(function, 629 | 630 | 660) {
                    continue;
                }
                *stats.by_record.entry((function, kind.clone())).or_default() += 1;
                // The CIS1/CIS2 strings follow their CTDA directly.
                let cis2 = subs[i + 1..]
                    .iter()
                    .take_while(|s| matches!(&s.sub_type, b"CIS1" | b"CIS2"))
                    .find(|s| &s.sub_type == b"CIS2");
                let Some(cis2) = cis2 else {
                    stats.missing_cis2 += 1;
                    continue;
                };
                let text = String::from_utf8_lossy(&cis2.data)
                    .trim_end_matches('\0')
                    .to_string();
                let spelling = if text.starts_with("::") && text.to_ascii_lowercase().ends_with("_var") {
                    "::Name_var"
                } else if text.starts_with("::") {
                    "::Name"
                } else {
                    "Name"
                };
                *stats.spellings.entry((function, spelling)).or_default() += 1;
                let samples = stats.samples.entry(function).or_default();
                if samples.len() < 8 {
                    samples.push(text);
                }
            }
        })?;
        println!("=== {path}");
        println!("  (function, record): {:?}", stats.by_record);
        println!("  (function, spelling): {:?}", stats.spellings);
        println!("  CTDAs without a CIS2: {}", stats.missing_cis2);
        for (function, samples) in &stats.samples {
            println!("  {function} samples: {samples:?}");
        }
    }
    Ok(())
}
