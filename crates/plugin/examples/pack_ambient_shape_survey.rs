//! Scoping check for the Skyrim+ ambient-AI gap: of the `PACK` records an
//! NPC's own `PKID` list references directly (ambient candidates — the
//! packages `npc_spawn.rs`'s M42 selector would need to run, with no `SCEN`
//! driving them), how many are FO3/FNV flat-shaped (the only shape the
//! current ambient selector understands) vs. Skyrim+ tree/template-shaped
//! (`PackRecord.procedures` / `package_template_form_id` / `data_inputs` —
//! the shape `crates/scripting/src/package.rs`'s executor already handles,
//! but only when a `SCEN` action names the package)?
//!
//! Cross-references against `SCEN` action package lists too, so a package
//! referenced *both* ways (ambient PKID **and** some scene's action list)
//! is counted separately rather than silently folded into either bucket.
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example pack_ambient_shape_survey -- <ESM>

use std::collections::{HashMap, HashSet};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let esm_path = args
        .next()
        .expect("usage: pack_ambient_shape_survey <ESM>");
    let bytes = std::fs::read(&esm_path)?;
    let index = byroredux_plugin::esm::parse_esm(&bytes).map_err(|e| anyhow::anyhow!(e))?;

    println!("game variant: {:?}", index.game);
    println!("NPC_ records: {}", index.npcs.len());
    println!("PACK records: {}", index.packages.len());
    println!("SCEN records: {}", index.scenes.len());

    // Every package FormID any SCEN action names, across every scene.
    let scene_owned: HashSet<u32> = index
        .scenes
        .values()
        .flat_map(|scene| scene.actions.iter())
        .flat_map(|action| action.packages.iter().copied())
        .collect();

    // Every package FormID any NPC_'s own PKID list references, and how
    // many distinct NPCs reference each (a package shared by many NPCs
    // matters more than one an obscure unique actor carries).
    let mut ambient_refs: HashMap<u32, usize> = HashMap::new();
    let mut npcs_with_packages = 0usize;
    for npc in index.npcs.values() {
        if npc.ai_packages.is_empty() {
            continue;
        }
        npcs_with_packages += 1;
        for &form_id in &npc.ai_packages {
            *ambient_refs.entry(form_id).or_default() += 1;
        }
    }

    println!();
    println!("NPCs carrying >=1 PKID entry: {npcs_with_packages}");
    println!("distinct packages referenced by some NPC's PKID list: {}", ambient_refs.len());
    println!("distinct packages referenced by some SCEN action: {}", scene_owned.len());

    let ambient_only: Vec<u32> = ambient_refs
        .keys()
        .copied()
        .filter(|form_id| !scene_owned.contains(form_id))
        .collect();
    let ambient_and_scene = ambient_refs.len() - ambient_only.len();
    println!(
        "  -> ambient-only (never named by a SCEN action): {}",
        ambient_only.len()
    );
    println!("  -> referenced both ways (ambient PKID + some SCEN action): {ambient_and_scene}");

    // Shape classification. A package counts as Skyrim+-shaped if it (or,
    // failing that, the type-19 template it points at) carries any of the
    // three Skyrim+-only signals; otherwise it's flat-shaped if it carries
    // any FO3/FNV-legacy content; otherwise it's genuinely empty (no PSDT/
    // PLDT/PTDT *and* no procedures/template/data-inputs — rare, but worth
    // knowing about rather than silently mis-bucketing).
    #[derive(Debug, Default)]
    struct ShapeCounts {
        skyrim_shaped: usize,
        skyrim_shaped_via_template: usize,
        flat_shaped: usize,
        empty: usize,
    }
    let classify = |form_id: u32, counts: &mut ShapeCounts| {
        let Some(package) = index.packages.get(&form_id) else {
            return; // PKID/action pointed at a FormID with no matching PACK record — dangling ref, skip.
        };
        let self_skyrim_shaped = package.package_template_form_id.is_some()
            || !package.procedures.is_empty()
            || !package.data_inputs.is_empty();
        if self_skyrim_shaped {
            counts.skyrim_shaped += 1;
            return;
        }
        // Empty concrete package with no template ref of its own is not
        // Skyrim+-shaped by definition; only chase a template when this
        // package actually has one but its *own* procedures were empty.
        let via_template = package
            .package_template_form_id
            .and_then(|tid| index.packages.get(&tid))
            .is_some_and(|template| !template.procedures.is_empty());
        if via_template {
            counts.skyrim_shaped_via_template += 1;
            return;
        }
        let flat_shaped =
            package.location.is_some() || package.schedule.is_some() || package.target.is_some();
        if flat_shaped {
            counts.flat_shaped += 1;
        } else {
            counts.empty += 1;
        }
    };

    let mut ambient_only_counts = ShapeCounts::default();
    for &form_id in &ambient_only {
        classify(form_id, &mut ambient_only_counts);
    }
    let mut scene_only_counts = ShapeCounts::default();
    for &form_id in scene_owned.iter().filter(|f| !ambient_refs.contains_key(f)) {
        classify(form_id, &mut scene_only_counts);
    }

    println!();
    println!("=== shape of ambient-only packages (n={}) ===", ambient_only.len());
    println!("{ambient_only_counts:#?}");
    println!();
    println!(
        "=== shape of scene-only packages (n={}), for comparison ===",
        scene_owned.len() - ambient_and_scene
    );
    println!("{scene_only_counts:#?}");

    // A few concrete examples from the highest-signal bucket, so the
    // classification can be spot-checked by hand (editor ID + reference
    // count) rather than trusted blind.
    let mut ambient_only_by_refs: Vec<(u32, usize)> = ambient_only
        .iter()
        .map(|&form_id| (form_id, ambient_refs[&form_id]))
        .collect();
    ambient_only_by_refs.sort_by(|a, b| b.1.cmp(&a.1));
    println!();
    println!("top 15 ambient-only packages by NPC reference count:");
    for (form_id, refs) in ambient_only_by_refs.iter().take(15) {
        let Some(package) = index.packages.get(form_id) else {
            continue;
        };
        let mut counts = ShapeCounts::default();
        classify(*form_id, &mut counts);
        let shape = if counts.skyrim_shaped > 0 || counts.skyrim_shaped_via_template > 0 {
            "skyrim+"
        } else if counts.flat_shaped > 0 {
            "flat"
        } else {
            "empty"
        };
        println!(
            "  {form_id:08X} '{}' refs={refs} shape={shape} procedures={} template={:?} inputs={}",
            package.editor_id,
            package.procedures.len(),
            package.package_template_form_id,
            package.data_inputs.len(),
        );
    }

    Ok(())
}
