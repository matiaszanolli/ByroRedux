//! Print the PROJ records (and AMMO damage) behind the named editor-id substring:
//! evidence for the projectile launch rules.
//!
//! Usage: cargo run --release -p byroredux-plugin --example proj_census -- <ESM> <substring>

use byroredux_plugin::esm::parse_esm;
use byroredux_plugin::esm::records::ItemKind;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("an ESM path");
    let needle = args.next().unwrap_or_default().to_ascii_lowercase();
    let index = parse_esm(&std::fs::read(&path)?)?;
    for item in index.items.values() {
        if let ItemKind::Weapon { damage, ammo_form, .. } = &item.kind
            && item.common.editor_id.to_ascii_lowercase().contains(&needle) {
                println!("WEAP {:08X} {} dmg {damage} ammo {ammo_form:08X}", item.form_id, item.common.editor_id);
            }
        if let ItemKind::Ammo { damage, projectile_form, .. } = &item.kind {
            if !item.common.editor_id.to_ascii_lowercase().contains(&needle) {
                continue;
            }
            let proj = index.projectiles.get(projectile_form);
            println!(
                "AMMO {:08X} {} dmg {damage} -> PROJ {:08X} {:?}",
                item.form_id,
                item.common.editor_id,
                projectile_form,
                proj.map(|p| (&p.editor_id, p.projectile_type(), p.projectile_flags(), p.muzzle_speed, p.gravity, p.range, p.collision_radius, p.lifetime, p.explosion, p.impact_force))
            );
        }
    }
    Ok(())
}
