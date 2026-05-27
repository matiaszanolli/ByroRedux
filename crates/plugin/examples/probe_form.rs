//! Locate FormIDs across an ESM's record categories. Used to find
//! which record type a missing-base-form REFR was pointing at —
//! F5 investigation in the 2026-05-26 Fallout symptom sweep.
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example probe_form -- <ESM> <FORMID_HEX> [<FORMID_HEX>…]

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let esm_path = args.next().ok_or_else(|| {
        anyhow::anyhow!("usage: probe_form ESM FORMID_HEX [FORMID_HEX …]")
    })?;
    let candidates: Vec<u32> = args
        .map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16))
        .collect::<Result<_, _>>()?;
    if candidates.is_empty() {
        anyhow::bail!("supply at least one FormID");
    }

    let bytes = std::fs::read(&esm_path)?;
    let index = byroredux_plugin::esm::parse_esm(&bytes)?;
    eprintln!("[probe_form] {} — probing {} FormID(s)", esm_path, candidates.len());

    for fid in candidates {
        if let Some(r) = index.cells.statics.get(&fid) {
            println!("{:08X}  STAT       model={:?}", fid, r.model_path);
        } else if let Some(r) = index.npcs.get(&fid) {
            println!("{:08X}  NPC_       editor_id={:?}", fid, r.editor_id);
        } else if let Some(r) = index.containers.get(&fid) {
            println!("{:08X}  CONT       editor_id={:?}", fid, r.editor_id);
        } else if let Some(r) = index.items.get(&fid) {
            println!("{:08X}  ITEM       editor_id={:?}", fid, r.common.editor_id);
        } else if let Some(r) = index.leveled_items.get(&fid) {
            println!("{:08X}  LVLI       editor_id={:?}", fid, r.editor_id);
        } else if let Some(r) = index.leveled_npcs.get(&fid) {
            println!("{:08X}  LVLN       editor_id={:?}", fid, r.editor_id);
        } else if let Some(r) = index.activators.get(&fid) {
            println!("{:08X}  ACTI       editor_id={:?}", fid, r.editor_id);
        } else if let Some(r) = index.projectiles.get(&fid) {
            println!("{:08X}  PROJ       editor_id={:?}", fid, r.editor_id);
        } else if let Some(r) = index.explosions.get(&fid) {
            println!("{:08X}  EXPL       editor_id={:?}", fid, r.editor_id);
        } else if index.creatures.contains_key(&fid) {
            println!("{:08X}  CREA       (creature)", fid);
        } else {
            println!("{:08X}  NOT FOUND in any category", fid);
        }
    }
    Ok(())
}
