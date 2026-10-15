//! M47.2 census: the scripts attached to quest aliases (the QUST `VMAD`
//! alias section), and whether the decoder walks every section to its last
//! byte — the proof of the layout (xEdit `wbVMADFragmentedQUST`).
//!
//! ```bash
//! cargo run --release -p byroredux-plugin --example qust_alias_script_census -- \
//!     "<Skyrim SE>/Data/Skyrim.esm" [TOP]
//! ```
//!
//! Prints the QUST VMAD count, how many carry alias scripts, how many sections
//! end exactly, and the most common alias script names.

use std::collections::BTreeMap;

use byroredux_plugin::esm::reader::EsmReader;
use byroredux_plugin::esm::records::script_instance::quest_alias_section;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: qust_alias_script_census ESM [TOP]"))?;
    let top: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(25);
    let bytes = std::fs::read(&path)?;
    let mut reader = EsmReader::new(&bytes);
    let end = bytes.len();

    let (mut vmads, mut with_aliases, mut exact, mut inexact) = (0usize, 0usize, 0usize, 0usize);
    let mut alias_scripts = 0usize;
    let mut names: BTreeMap<String, usize> = BTreeMap::new();
    let mut inexact_samples = Vec::new();
    reader.for_each_record(end, Some(b"QUST"), |header, subs| {
        let form_id = header.form_id;
        let Some(vmad) = subs.iter().find(|s| s.sub_type == *b"VMAD") else {
            return;
        };
        vmads += 1;
        let (aliases, ends_exactly) = quest_alias_section(&vmad.data);
        if ends_exactly {
            exact += 1;
        } else {
            inexact += 1;
            if inexact_samples.len() < 5 {
                inexact_samples.push(form_id);
            }
        }
        if !aliases.is_empty() {
            with_aliases += 1;
        }
        for alias in aliases {
            for script in alias.scripts.scripts {
                alias_scripts += 1;
                *names.entry(script.name).or_default() += 1;
            }
        }
    })?;
    println!("QUST VMADs: {vmads}; section ends exactly: {exact}; not exact: {inexact}");
    println!("with alias scripts: {with_aliases}; alias script instances: {alias_scripts}; distinct scripts: {}", names.len());
    if !inexact_samples.is_empty() {
        println!("not-exact samples: {:08X?}", inexact_samples);
    }
    let mut ranked: Vec<_> = names.into_iter().collect();
    ranked.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (name, count) in ranked.into_iter().take(top) {
        println!("{count:>6}  {name}");
    }
    Ok(())
}
