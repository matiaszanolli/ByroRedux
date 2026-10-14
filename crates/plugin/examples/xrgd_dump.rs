//! #5015 census tool: dump every placed actor's authored `XRGD` ragdoll pose.
//!
//! One JSON line per placement carrying a pose: the placement's own transform,
//! its base NPC / race, whether it is a Starts-Dead corpse, and every pose
//! entry `[bone_id, px, py, pz, rx, ry, rz]` exactly as decoded
//! (`PlacedRef::ragdoll_pose`). Feeds the offline analysis that derives the
//! id → ragdoll-body mapping and the rotation convention (#5015).
//!
//! ```text
//! cargo run -p byroredux-plugin --example xrgd_dump -- <Skyrim.esm> > poses.jsonl
//! ```

use byroredux_plugin::esm;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: xrgd_dump <ESM>"))?;
    let bytes = std::fs::read(&path)?;
    let index = esm::records::parse_esm(&bytes)?;

    let emit = |cell: &str, r: &esm::cell::PlacedRef| {
        // `XRGD_ALL=1` also emits pose-less placed NPCs (the live actors a
        // corpse's height can be compared against).
        if r.ragdoll_pose.is_empty()
            && !(std::env::var_os("XRGD_ALL").is_some() && index.npcs.contains_key(&r.base_form_id))
        {
            return;
        }
        let (editor_id, race, creature) = index
            .npcs
            .get(&r.base_form_id)
            .map(|npc| {
                let race = index
                    .races
                    .get(&npc.race_form_id)
                    .map(|race| race.editor_id.clone())
                    .unwrap_or_default();
                (npc.editor_id.clone(), race, npc.is_creature)
            })
            .unwrap_or_default();
        let entries: Vec<[f32; 7]> = r
            .ragdoll_pose
            .iter()
            .map(|b| {
                [
                    f32::from(b.bone_id),
                    b.position[0],
                    b.position[1],
                    b.position[2],
                    b.rotation[0],
                    b.rotation[1],
                    b.rotation[2],
                ]
            })
            .collect();
        // No `serde_json` in this crate; the fields are plain numbers and
        // identifiers, so the line is spelled by hand.
        let entries = entries
            .iter()
            .map(|e| format!("[{}]", e.map(|v| format!("{v:?}")).join(",")))
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{{\"cell\":\"{cell}\",\"ref\":{},\"base\":{},\"npc\":\"{editor_id}\",\"race\":\"{race}\",\"creature\":{creature},\"dead\":{},\"pos\":{:?},\"rot\":{:?},\"entries\":[{entries}]}}",
            r.form_id, r.base_form_id, r.starts_dead, r.position, r.rotation,
        );
    };

    for cell in index.cells.cells.values() {
        for r in &cell.references {
            emit(&cell.editor_id, r);
        }
    }
    for (world, cells) in &index.cells.exterior_cells {
        for cell in cells.values() {
            for r in &cell.references {
                emit(world, r);
            }
        }
    }
    Ok(())
}
