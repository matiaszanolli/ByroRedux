//! Throwaway research probe (M42 seat assignment): dump an NPC's AI package
//! list in priority order with each package's procedure type, to decide the
//! "which NPC actually sandboxes" selection rule.
//!
//! Usage: cargo run -p byroredux-plugin --example probe_npc_packages -- <ESM> [edid-substr...]

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let esm_path = args.next().ok_or_else(|| anyhow::anyhow!("usage: <ESM> [substr...]"))?;
    let filters: Vec<String> = {
        let rest: Vec<String> = args.map(|s| s.to_lowercase()).collect();
        if rest.is_empty() { vec!["gstrudy".into(), "gsjoecobb".into(), "gsringo".into()] } else { rest }
    };

    let bytes = std::fs::read(&esm_path)?;
    let index = byroredux_plugin::esm::parse_esm(&bytes)?;

    use byroredux_plugin::esm::records::active_package_is_sandbox;
    let hour: f32 = std::env::var("HOUR").ok().and_then(|h| h.parse().ok()).unwrap_or(10.0);
    let mut sandbox_anywhere = 0usize;
    let mut active_sandbox = 0usize;
    let mut total = 0usize;
    for npc in index.npcs.values() {
        total += 1;
        let resolved: Vec<_> = npc.ai_packages.iter().filter_map(|p| index.packages.get(p)).collect();
        let any_sandbox = resolved.iter().any(|pk| pk.is_sandbox());
        let active = active_package_is_sandbox(resolved.iter().copied(), hour);
        if any_sandbox { sandbox_anywhere += 1; }
        if active { active_sandbox += 1; }

        let edid = npc.editor_id.to_lowercase();
        if filters.iter().any(|f| edid.contains(f.as_str())) {
            // Which package is the active one at `hour`?
            let active_idx = resolved.iter().position(|pk| pk.scheduled_active_at(hour));
            println!("\nNPC {:08X} {:?} @ {:.0}h → sandbox={}", npc.form_id, npc.editor_id, hour, active);
            for (i, p) in npc.ai_packages.iter().enumerate() {
                match index.packages.get(p) {
                    Some(pk) => println!(
                        "  [{}]{} {:<28} proc={:<2} sched={:?}{}",
                        i, if Some(i) == active_idx { "*" } else { " " }, pk.editor_id, pk.procedure_type,
                        pk.schedule.map(|s| (s.start_hour, s.duration_hours)),
                        if pk.is_sandbox() { "  <SANDBOX>" } else { "" },
                    ),
                    None => println!("  [{}]  <unresolved>", i),
                }
            }
        }
    }
    println!(
        "\n=== corpus @ {:.0}h: {} NPCs | active-package-is-sandbox: {} ({:.0}%) | sandbox-anywhere: {} ({:.0}%) ===",
        hour, total, active_sandbox, 100.0 * active_sandbox as f64 / total.max(1) as f64,
        sandbox_anywhere, 100.0 * sandbox_anywhere as f64 / total.max(1) as f64,
    );
    Ok(())
}
