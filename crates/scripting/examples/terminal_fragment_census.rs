//! M47.2 census: the fragments terminals (`TERM` `VMAD`) bind, and whether
//! each fragment index names one of the terminal's menu items (`ITID`) — the
//! mapping the terminal dispatch relies on.
//!
//! ```bash
//! cargo run --release -p byroredux-scripting --example terminal_fragment_census -- \
//!     "<Fallout 4>/Data/Fallout4.esm"
//! ```

use byroredux_plugin::esm::parse_esm;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let esm = std::env::args().nth(1).expect("usage: terminal_fragment_census <ESM>");
    let index = parse_esm(&std::fs::read(&esm)?)?;
    let (mut terminals, mut fragments, mut matched) = (0usize, 0usize, 0usize);
    for term in index.terminals.values() {
        if term.script_fragments.is_empty() {
            continue;
        }
        terminals += 1;
        if let Ok(filter) = std::env::var("LIST")
            && term.editor_id.to_ascii_lowercase().contains(&filter.to_ascii_lowercase()) {
                println!("{:08X} {} fragments {:?}", term.form_id, term.editor_id, term.script_fragments.iter().map(|f| f.index).collect::<Vec<_>>());
            }
        for fragment in &term.script_fragments {
            fragments += 1;
            let named = fragment.fragment_name.rsplit('_').next().and_then(|n| n.parse::<u32>().ok());
            let item = term.menu_items.iter().any(|item| u32::from(item.item_id) == fragment.index);
            if item && named == Some(fragment.index) {
                matched += 1;
            } else if std::env::var("SHOW_MISMATCH").is_ok() {
                println!(
                    "{:08X} {} idx {} {} (items {:?})",
                    term.form_id,
                    term.editor_id,
                    fragment.index,
                    fragment.fragment_name,
                    term.menu_items.iter().map(|i| i.item_id).collect::<Vec<_>>()
                );
            }
        }
    }
    println!("{terminals} terminals with fragments, {fragments} fragments, {matched} whose index is a menu item id and the function suffix");
    Ok(())
}
