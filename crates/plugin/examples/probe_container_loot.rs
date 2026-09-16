//! Headless cross-game check of CONT -> LVLI -> inventory expansion.
//! Usage: cargo run -p byroredux-plugin --example probe_container_loot -- ESM...
use byroredux_plugin::{equip::expand_leveled_loot, esm::parse_esm};

fn main() -> anyhow::Result<()> {
    let paths: Vec<_> = std::env::args().skip(1).collect();
    anyhow::ensure!(!paths.is_empty(), "supply one or more master plugin paths");
    for path in paths {
        let bytes = std::fs::read(&path)?;
        let index = parse_esm(&bytes)?;
        let mut resolved = Vec::new();
        let mut seeds = 0;
        let mut stacks = 0;
        let mut missing_items = 0;
        let mut missing_types = std::collections::BTreeMap::new();
        let mut untracked_ids = std::collections::BTreeMap::new();
        let empty_lists = index
            .leveled_items
            .values()
            .filter(|list| list.entries.is_empty())
            .count();
        for container in index.containers.values() {
            for entry in &container.contents {
                if entry.count <= 0 || !index.leveled_items.contains_key(&entry.item_form_id) {
                    continue;
                }
                seeds += 1;
                for level in [1, 10, 50] {
                    resolved.clear();
                    expand_leveled_loot(
                        entry.item_form_id,
                        entry.count as u32,
                        level,
                        &index,
                        &mut resolved,
                    );
                    for &(form_id, count) in &resolved {
                        anyhow::ensure!(count > 0, "zero stack in {path}");
                        anyhow::ensure!(
                            !index.leveled_items.contains_key(&form_id),
                            "unexpanded list {form_id:08X} in {path}"
                        );
                        // Report unsupported/missing metadata separately: a terminal
                        // ID need not belong to the equipment-oriented item index.
                        missing_items += usize::from(!index.items.contains_key(&form_id));
                        if !index.items.contains_key(&form_id) {
                            let kind = index
                                .record_types
                                .get(&form_id)
                                .copied()
                                .unwrap_or(*b"????");
                            *missing_types.entry(kind).or_insert(0usize) += 1;
                            if kind == *b"????" {
                                *untracked_ids.entry(form_id).or_insert(0usize) += 1;
                            }
                        }
                    }
                    stacks += resolved.len();
                }
            }
        }
        println!(
            "{path}: containers={} leveled_seeds={seeds} resolved_stacks={stacks} leaves_outside_item_index={missing_items}",
            index.containers.len(),
        );
        println!(
            "  leveled_lists={} empty_lists={empty_lists}",
            index.leveled_items.len()
        );
        for (kind, count) in missing_types {
            println!(
                "  missing_item_type={} stacks={count}",
                String::from_utf8_lossy(&kind)
            );
        }
        // The index traces consumed records only. Scan raw headers to
        // distinguish unsupported signatures from genuinely absent IDs.
        let mut raw_types = std::collections::BTreeMap::new();
        if !untracked_ids.is_empty() {
            let mut reader = byroredux_plugin::esm::reader::EsmReader::new(&bytes);
            reader.read_file_header()?;
            while reader.remaining() > 0 {
                if reader.is_group() {
                    reader.read_group_header()?;
                } else {
                    let header = reader.read_record_header()?;
                    if untracked_ids.contains_key(&header.form_id) {
                        raw_types.insert(header.form_id, header.record_type);
                    }
                    reader.skip_record(&header);
                }
            }
        }
        let mut untracked: Vec<_> = untracked_ids.into_iter().collect();
        untracked.sort_by_key(|&(form_id, count)| (std::cmp::Reverse(count), form_id));
        for (form_id, count) in untracked.into_iter().take(5) {
            let kind = raw_types.get(&form_id).copied().unwrap_or(*b"????");
            println!(
                "  untracked_form={form_id:08X} raw_type={} stacks={count}",
                String::from_utf8_lossy(&kind)
            );
        }
    }
    Ok(())
}
