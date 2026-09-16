//! Headless check of authored ALCH -> MGEF -> AVIF restoration plans.
use byroredux_plugin::{consumables::instant_restorations, esm::parse_esm};
fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).expect("supply a master ESM path");
    let index = parse_esm(&std::fs::read(path)?)?;
    let mut supported: Vec<_> = index
        .items
        .keys()
        .filter_map(|&id| instant_restorations(&index, id).map(|effects| (id, effects)))
        .collect();
    supported.sort_by_key(|entry| entry.0);
    println!("supported_immediate_restoratives={}", supported.len());
    let conditional_supported = index
        .items
        .keys()
        .filter(|&&id| {
            byroredux_plugin::consumables::conditional_restorations(&index, id).is_some()
        })
        .count();
    println!("supported_conditional_restoratives={conditional_supported}");
    let candidates = index
        .items
        .values()
        .filter(|item| {
            matches!(
                &item.kind,
                byroredux_plugin::esm::records::ItemKind::Aid {
                    immediate_effects: Some(_),
                    ..
                }
            )
        })
        .count();
    println!("immediate_alch_chains={candidates}");
    let mut authored = 0;
    let mut conditional = 0;
    let mut timed = 0;
    for item in index.items.values() {
        if let byroredux_plugin::esm::records::ItemKind::Aid {
            authored_effects: Some(effects),
            ..
        } = &item.kind
        {
            authored += 1;
            conditional += usize::from(effects.iter().any(|e| !e.conditions.is_empty()));
            timed += usize::from(effects.iter().any(|e| e.effect.duration > 0));
        }
    }
    println!(
        "authored_alch_chains={authored} conditional_chains={conditional} timed_chains={timed}"
    );
    for (id, effects) in supported.iter().take(20) {
        println!("{id:08X} {} {effects:?}", index.items[id].common.editor_id);
    }
    let mut conditional_ids: Vec<_> = index
        .items
        .keys()
        .copied()
        .filter(|&id| byroredux_plugin::consumables::conditional_restorations(&index, id).is_some())
        .collect();
    conditional_ids.sort_unstable();
    for id in conditional_ids.into_iter().take(20) {
        println!(
            "conditional {id:08X} {} {:?}",
            index.items[&id].common.editor_id,
            byroredux_plugin::consumables::conditional_restorations(&index, id).unwrap()
        );
    }
    Ok(())
}
