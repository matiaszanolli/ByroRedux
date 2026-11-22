//! OAV-USE-004 (#7011) — the constant-effect abilities `Oblivion.esm` authors reach `ActorValues` through
//! `canonical_spell`: the unit test (`npc_spawn::tests::oblivion_ability_changes_getav_on_the_spawned_actor`)
//! builds its Fortify Sneak ability by hand; this leg walks the real master's abilities.
//!
//! **Opt-in / `#[ignore]`d** — needs Oblivion.esm on disk:
//! ```bash
//! cargo test -p byroredux-scripting --test oblivion_ability_real_data -- --ignored --nocapture
//! ```
//! Skips gracefully (passing) when the game data isn't present.

use std::path::PathBuf;

use byroredux_plugin::esm::reader::{GameKind, GlobalSlot};
use byroredux_plugin::esm::records::{EsmIndex, SpellType};
use byroredux_plugin::esm::test_paths;
use byroredux_scripting::magic::canonical_spell;

fn oblivion_index() -> Option<EsmIndex> {
    let dir = std::env::var(test_paths::OBLIVION_ENV).unwrap_or_else(|_| test_paths::OBLIVION_DEFAULT.into());
    let bytes = std::fs::read(PathBuf::from(dir).join("Oblivion.esm")).ok()?;
    let mut index = byroredux_plugin::esm::parse_esm(&bytes).ok()?;
    assert_eq!(index.game, GameKind::Oblivion);
    // The loader gives the engine's records the lowest slot no record occupies.
    let free = (0u8..=0xfb).find(|slot| !index.record_types.keys().any(|id| (id >> 24) as u8 == *slot))?;
    index.install_oblivion_actor_values(GlobalSlot::Regular(free)).ok()?;
    Some(index)
}

#[test]
#[ignore = "needs Oblivion.esm on disk"]
fn oblivion_abilities_that_fortify_a_skill_modify_the_installed_value() {
    let Some(index) = oblivion_index() else {
        eprintln!("SKIP: Oblivion.esm not found");
        return;
    };
    let sneak = index.actor_value_form_id("Sneak").expect("Sneak installed");
    let (mut abilities, mut with_modifier, mut sneak_rows) = (0usize, 0usize, Vec::new());
    let mut stranded = Vec::new();
    for (&form_id, spell) in &index.spells {
        if spell.spell_type != SpellType::Ability {
            continue;
        }
        abilities += 1;
        let canonical = canonical_spell(&index, form_id);
        assert!(canonical.constant, "{form_id:08X}: an Ability is constant");
        if !canonical.constant_modifiers.is_empty() {
            with_modifier += 1;
        }
        // Every modifier lands on an installed key, never an unresolved one.
        for modifier in &canonical.constant_modifiers {
            if !index.actor_values.contains_key(&modifier.actor_value) {
                stranded.push(format!("{form_id:08X} {}", spell.editor_id));
            }
            if modifier.actor_value == sneak {
                sneak_rows.push((form_id, spell.editor_id.clone(), modifier.amount));
            }
        }
    }
    eprintln!(
        "[Oblivion/abilities] {abilities} abilities, {with_modifier} with a value modifier, {} reach Sneak: {sneak_rows:?}",
        sneak_rows.len()
    );
    assert!(stranded.is_empty(), "modifiers keyed to a value no record installs: {stranded:?}");
    assert_eq!((abilities, with_modifier), (292, 221), "Oblivion.esm's ability census moved");
    // Two rows with a plain meaning: the Mankar Camoran birthsign's Fortify Sneak 100, and a detrimental
    // Sneak effect, whose authored magnitude 50 lowers the value (-50).
    let amount = |editor_id: &str| sneak_rows.iter().find(|row| row.1 == editor_id).map(|row| row.2);
    assert_eq!(amount("AbBirthsignMankarCamoran"), Some(100.0));
    assert_eq!(amount("DarkCeledaenNoSneak"), Some(-50.0));
    assert_eq!(sneak_rows.len(), 7, "the abilities reaching Sneak moved: {sneak_rows:?}");
}
