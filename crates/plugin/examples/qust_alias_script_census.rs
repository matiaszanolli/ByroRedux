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

type OwnedSub = ([u8; 4], Vec<u8>);

fn walk(reader: &mut EsmReader, end: usize, f: &mut dyn FnMut(u32, &[OwnedSub])) {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let Ok(g) = reader.read_group_header() else {
                return;
            };
            let sub_end = reader.group_content_end(&g);
            walk(reader, sub_end, f);
            continue;
        }
        let Ok(header) = reader.read_record_header() else {
            return;
        };
        if &header.record_type == b"QUST" {
            if let Ok(subs) = reader.read_sub_records(&header) {
                let owned: Vec<OwnedSub> = subs.into_iter().map(|s| (s.sub_type, s.data)).collect();
                f(header.form_id, &owned);
            }
        } else {
            reader.skip_record(&header);
        }
    }
}

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
    walk(&mut reader, end, &mut |form_id, subs| {
        let Some((_, vmad)) = subs.iter().find(|(t, _)| t == b"VMAD") else {
            return;
        };
        vmads += 1;
        let (aliases, ends_exactly) = quest_alias_section(vmad);
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
    });
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
