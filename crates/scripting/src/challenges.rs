//! The New Vegas challenges (`CHAL`): the Pip-Boy's challenges screen, and the three ObScript commands
//! that act on it — *UnlockChallenge*, *RemoveRecurringFromChallenge* and *GetChallengeCompleted*.
//!
//! What the GECK says (page *Challenge*): a challenge has a **threshold** ("the number of times an action
//! must be performed for the challenge to complete"), an **interval** (progress notifications), a
//! **reward script** ("runs upon completion"), and three flags — *Start Disabled* ("will not be visible
//! and will not progress until enabled by script"), *Recurring* ("will start over upon completion") and
//! *Show Zero Progress*. *Miscellaneous Stat* challenges progress "as the value of a misc. statistic
//! increases" — so their progress is the stat's rise since the challenge was enabled (or restarted).
//!
//! Progress sources the engine has: the misc stats (scripts' `ModPCMiscStat`, plus the kills the death
//! funnel counts), items consumed, items the player acquires (every inventory add announces itself
//! through `emit_item_transfers`), map markers discovered, and a kill of a specific form or one in a
//! list. The types whose progress needs a weapon category, a hit location or a crit (*Do Damage*, *Hit
//! an Enemy*, *Use a Skill*, *Craft*, and the weapon-keyed kill challenges) have no source to count
//! from — the combat model carries neither — so they stay at zero rather than progress on a guess.
//!
//! `GetChallengeCompleted` is "completed at least once": a recurring challenge restarts on completion,
//! but the tier chains (`GetChallengeCompleted NVChallengeShootEmUp`) ask whether it was ever done.

use std::collections::BTreeMap;
use std::sync::Arc;

use byroredux_core::ecs::{Resource, World};
use byroredux_plugin::esm::records::{challenge_kind as kind, ChalRecord, EsmIndex};

use crate::magic_effects::CastEffect;

/// The misc stats a challenge names by index (xEdit's `wbMiscStatEnum`), by the name `GetPCMiscStat`
/// takes.
pub const MISC_STAT_NAMES: [&str; 43] = [
    "Quests Completed",
    "Locations Discovered",
    "People Killed",
    "Creatures Killed",
    "Locks Picked",
    "Computers Hacked",
    "Stimpaks Taken",
    "Rad-X Taken",
    "RadAway Taken",
    "Chems Taken",
    "Times Addicted",
    "Mines Disarmed",
    "Speech Successes",
    "Pockets Picked",
    "Pants Exploded",
    "Books Read",
    "Bobbleheads Found",
    "Weapons Created",
    "People Mezzed",
    "Captives Rescued",
    "Sandman Kills",
    "Paralyzing Punches",
    "Robots Disabled",
    "Contracts Completed",
    "Corpses Eaten",
    "Mysterious Stranger Visits",
    "Doctor Bags Used",
    "Challenges Completed",
    "Miss Fortunate Occurrences",
    "Disintegrations",
    "Have Limbs Crippled",
    "Speech Failures",
    "Items Crafted",
    "Weapon Modifications",
    "Items Repaired",
    "Total Things Killed",
    "Dismembered Limbs",
    "Caravan Games Won",
    "Caravan Games Lost",
    "Barter Amount Traded",
    "Roulette Games Played",
    "Blackjack Games Played",
    "Slots Games Played",
];

/// The misc stat at `index`.
pub fn misc_stat_name(index: u16) -> Option<&'static str> {
    MISC_STAT_NAMES.get(usize::from(index)).copied()
}

/// The load order's challenges by form id. `NOT_SAVED_BY_DESIGN`: rebuilt from the records at every
/// content load. Ordered (#6265): when one event or stat rise finishes several challenges they
/// complete, bump `Challenges Completed` and start their reward scripts in form-id order, not in
/// per-process hash order.
#[derive(Debug, Default, Clone)]
pub struct ChallengeDefs(Arc<BTreeMap<u32, ChalRecord>>);

impl Resource for ChallengeDefs {}

impl ChallengeDefs {
    pub fn from_index(index: &EsmIndex) -> Self {
        Self(Arc::new(index.challenges.iter().map(|(&form, def)| (form, def.clone())).collect()))
    }

    pub fn get(&self, challenge: u32) -> Option<&ChalRecord> {
        self.0.get(&challenge)
    }
}

/// One challenge's progress.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
struct Entry {
    /// The count so far (an event-counted challenge; a misc-stat one is read live).
    progress: u32,
    /// A misc-stat challenge's stat value when it was enabled or last restarted.
    baseline: i32,
    /// How many times it completed.
    completions: u32,
    /// Enabled by `UnlockChallenge`.
    unlocked: bool,
    /// `RemoveRecurringFromChallenge`.
    recurring_removed: bool,
}

/// Every challenge a script or the player has touched. Saved.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ChallengeState {
    entries: BTreeMap<u32, Entry>,
    /// The map markers already counted as discovered (`Locations Discovered`, and the challenges that
    /// name a marker), so each is counted once.
    discovered_markers: std::collections::BTreeSet<u32>,
}

impl Resource for ChallengeState {}

/// Something that happened that a challenge may count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengeEvent {
    /// The player killed an actor of this base form.
    Kill { base: u32 },
    /// The player consumed an item.
    ItemUsed { item: u32 },
    /// The player acquired `count` of an item.
    ItemAcquired { item: u32, count: u32 },
    /// The player discovered a map marker.
    MarkerDiscovered { marker: u32 },
}

fn stat_value(world: &World, def: &ChalRecord) -> i32 {
    misc_stat_name(def.value1).map_or(0, |name| crate::obscript_more::misc_stat(world, name))
}

fn is_enabled(state: &ChallengeState, def: &ChalRecord) -> bool {
    !def.starts_disabled() || state.entries.get(&def.form_id).is_some_and(|entry| entry.unlocked)
}

fn is_recurring(state: &ChallengeState, def: &ChalRecord) -> bool {
    def.is_recurring() && !state.entries.get(&def.form_id).is_some_and(|entry| entry.recurring_removed)
}

/// `UnlockChallenge`: enable a challenge flagged *Start Disabled*; it counts from now. `false` for an
/// unknown challenge.
pub fn unlock(world: &World, challenge: u32) -> bool {
    let Some(def) = world.try_resource::<ChallengeDefs>().and_then(|defs| defs.get(challenge).cloned()) else { return false };
    let baseline = stat_value(world, &def);
    let Some(mut state) = world.try_resource_mut::<ChallengeState>() else { return false };
    let entry = state.entries.entry(challenge).or_default();
    if !entry.unlocked {
        entry.unlocked = true;
        entry.baseline = baseline;
    }
    true
}

/// `RemoveRecurringFromChallenge`: it will not start over. `false` for an unknown challenge.
pub fn remove_recurring(world: &World, challenge: u32) -> bool {
    if world.try_resource::<ChallengeDefs>().and_then(|defs| defs.get(challenge).map(|_| ())).is_none() {
        return false;
    }
    let Some(mut state) = world.try_resource_mut::<ChallengeState>() else { return false };
    state.entries.entry(challenge).or_default().recurring_removed = true;
    true
}

/// `GetChallengeCompleted`: whether the challenge was ever completed; `None` for an unknown one.
pub fn completed(world: &World, challenge: u32) -> Option<bool> {
    world.try_resource::<ChallengeDefs>()?.get(challenge)?;
    Some(world.try_resource::<ChallengeState>().is_some_and(|state| state.entries.get(&challenge).is_some_and(|entry| entry.completions > 0)))
}

/// The count a challenge shows now.
pub fn progress(world: &World, challenge: u32) -> Option<u32> {
    let def = world.try_resource::<ChallengeDefs>()?.get(challenge)?.clone();
    let entry = world.try_resource::<ChallengeState>()?.entries.get(&challenge).cloned().unwrap_or_default();
    Some(if def.challenge_type == kind::MISC_STAT {
        u32::try_from(stat_value(world, &def).saturating_sub(entry.baseline)).unwrap_or(0)
    } else {
        entry.progress
    })
}

/// Finish `def`: count it, run its reward script on the player, and restart it if it recurs.
fn complete(world: &World, def: &ChalRecord) {
    let stat_now = stat_value(world, def);
    let restart = world.try_resource::<ChallengeState>().is_some_and(|state| is_recurring(&state, def));
    if let Some(mut state) = world.try_resource_mut::<ChallengeState>() {
        let entry = state.entries.entry(def.form_id).or_default();
        entry.completions += 1;
        if restart {
            entry.progress = 0;
            entry.baseline = stat_now;
        } else {
            entry.progress = def.threshold;
        }
    }
    crate::obscript_more::mod_misc_stat(world, "Challenges Completed", 1);
    log::info!("challenge {:08X} {} completed", def.form_id, def.editor_id);
    if def.script_form_id != 0 {
        if let Some(player) = crate::papyrus_demo::PapyrusPlayerEntity::get(world) {
            let reward = CastEffect { mgef: 0, script: def.script_form_id, duration: 0.0, magnitude: 0.0, value: None };
            crate::magic_effects::start_effects(world, player, Some(player), def.form_id, &[reward], false);
        }
    }
}

/// Whether `def` still counts: enabled, and not a finished non-recurring one.
fn counts(state: &ChallengeState, def: &ChalRecord) -> bool {
    is_enabled(state, def)
        && (is_recurring(state, def) || state.entries.get(&def.form_id).is_none_or(|entry| entry.completions == 0))
}

/// Count `event` towards every challenge that takes it.
pub fn note(world: &World, event: ChallengeEvent) {
    let Some(defs) = world.try_resource::<ChallengeDefs>().map(|defs| defs.clone()) else { return };
    let in_list = |list: u32, form: u32| crate::form_lists::forms_of(world, list, &[]).is_some_and(|forms| forms.contains(&form));
    let mut finished = Vec::new();
    for def in defs.0.values() {
        let amount = match (event, def.challenge_type) {
            (ChallengeEvent::Kill { base }, kind::KILL_SPECIFIC) if def.form1 != 0 && def.form1 == base => 1,
            (ChallengeEvent::Kill { base }, kind::KILL_FROM_LIST) if def.form1 != 0 && in_list(def.form1, base) => 1,
            (ChallengeEvent::ItemUsed { item }, kind::USE_ITEM) if def.form1 != 0 && def.form1 == item => 1,
            (ChallengeEvent::ItemUsed { item }, kind::USE_ITEM_FROM_LIST) if def.form1 != 0 && in_list(def.form1, item) => 1,
            (ChallengeEvent::ItemAcquired { item, count }, kind::ACQUIRE_ITEM) if def.form1 != 0 && def.form1 == item => count,
            (ChallengeEvent::ItemAcquired { item, count }, kind::ACQUIRE_FROM_LIST) if def.form1 != 0 && in_list(def.form1, item) => count,
            (ChallengeEvent::MarkerDiscovered { marker }, kind::DISCOVER_MARKER) if def.form1 != 0 && (def.form1 == marker || in_list(def.form1, marker)) => 1,
            _ => 0,
        };
        if amount == 0 {
            continue;
        }
        let Some(mut state) = world.try_resource_mut::<ChallengeState>() else { return };
        if !counts(&state, def) {
            continue;
        }
        let entry = state.entries.entry(def.form_id).or_default();
        entry.progress = entry.progress.saturating_add(amount);
        // A threshold of 0 (one Vault marker authors it) is "the one thing".
        if entry.progress >= def.threshold.max(1) {
            finished.push(def.clone());
        }
    }
    for def in finished {
        complete(world, &def);
    }
}

/// Count the map markers that became visible since the last pass: each is one `Locations Discovered`
/// (a GECK misc stat) and an event for the challenges that name it.
fn note_discovered_markers(world: &World) {
    let Some(markers) = world.try_resource::<crate::MapMarkers>().map(|markers| markers.visible_markers()) else { return };
    let fresh: Vec<u32> = match world.try_resource_mut::<ChallengeState>() {
        Some(mut state) => markers.into_iter().filter(|&marker| state.discovered_markers.insert(marker)).collect(),
        None => return,
    };
    for marker in fresh {
        crate::obscript_more::mod_misc_stat(world, "Locations Discovered", 1);
        note(world, ChallengeEvent::MarkerDiscovered { marker });
    }
}

/// Complete the misc-stat challenges whose stat has risen far enough. Exclusive: a reward script runs.
pub fn challenge_system(world: &World, _dt: f32) {
    note_discovered_markers(world);
    let Some(defs) = world.try_resource::<ChallengeDefs>().map(|defs| defs.clone()) else { return };
    let mut finished = Vec::new();
    {
        let Some(state) = world.try_resource::<ChallengeState>() else { return };
        for def in defs.0.values().filter(|def| def.challenge_type == kind::MISC_STAT && def.threshold > 0) {
            if !counts(&state, def) {
                continue;
            }
            let baseline = state.entries.get(&def.form_id).map_or(0, |entry| entry.baseline);
            if stat_value(world, def).saturating_sub(baseline) >= i32::try_from(def.threshold).unwrap_or(i32::MAX) {
                finished.push(def.clone());
            }
        }
    }
    for def in finished {
        complete(world, &def);
    }
}

/// What `challenge.status` prints: one line per challenge asked for (every one that has progressed,
/// completed or been unlocked when none is named).
pub fn status_lines(world: &World, wanted: Option<u32>) -> Vec<String> {
    let (Some(defs), Some(state)) = (world.try_resource::<ChallengeDefs>().map(|defs| defs.clone()), world.try_resource::<ChallengeState>().map(|state| state.clone())) else {
        return vec!["no challenges (this load order has none)".to_owned()];
    };
    let mut forms: Vec<u32> = match wanted {
        Some(form) => vec![form],
        None => {
            let mut touched: Vec<u32> = state.entries.keys().copied().collect();
            touched.extend(defs.0.values().filter(|def| def.challenge_type == kind::MISC_STAT && progress(world, def.form_id).unwrap_or(0) > 0).map(|def| def.form_id));
            touched.sort_unstable();
            touched.dedup();
            touched
        }
    };
    forms.retain(|form| defs.get(*form).is_some());
    if forms.is_empty() {
        return vec!["no challenge with progress".to_owned()];
    }
    forms
        .into_iter()
        .map(|form| {
            let def = defs.get(form).expect("retained above");
            let entry = state.entries.get(&form).cloned().unwrap_or_default();
            format!(
                "{form:08X} {} type {} {}/{} enabled={} completions={} recurring={}",
                def.editor_id,
                def.challenge_type,
                progress(world, form).unwrap_or(0),
                def.threshold,
                is_enabled(&state, def),
                entry.completions,
                is_recurring(&state, def),
            )
        })
        .collect()
}

/// This subsystem's resources.
pub fn install(world: &mut World) {
    world.init_resource::<ChallengeDefs>();
    world.init_resource::<ChallengeState>();
}

/// This subsystem's saved column. Called by the binary's `build_save_registry` in its row order.
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_resource::<ChallengeState>("ChallengeState");
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::{CHALLENGE_FLAG_RECURRING, CHALLENGE_FLAG_START_DISABLED};

    const WIN: u32 = 0x0016_5777;
    const MASTER: u32 = 0x0016_ba4e;
    const VAULT: u32 = 0x0016_0001;
    const MARKER: u32 = 0x000c_df00;

    fn chal(form_id: u32, challenge_type: u32, threshold: u32, flags: u32, value1: u16, form1: u32) -> ChalRecord {
        ChalRecord { form_id, editor_id: format!("Chal{form_id:X}"), challenge_type, threshold, flags, value1, form1, ..Default::default() }
    }

    fn world(defs: Vec<ChalRecord>) -> World {
        let mut world = World::new();
        crate::register(&mut world);
        let mut index = EsmIndex::default();
        for def in defs {
            index.challenges.insert(def.form_id, def);
        }
        world.insert_resource(ChallengeDefs::from_index(&index));
        world
    }

    #[test]
    fn the_misc_stat_names_are_the_enum_in_order() {
        assert_eq!(misc_stat_name(37), Some("Caravan Games Won"));
        assert_eq!(misc_stat_name(35), Some("Total Things Killed"));
        assert_eq!(misc_stat_name(42), Some("Slots Games Played"));
        assert_eq!(misc_stat_name(43), None);
    }

    #[test]
    fn a_misc_stat_challenge_completes_when_its_stat_has_risen_by_the_threshold() {
        let world = world(vec![chal(WIN, kind::MISC_STAT, 3, 0, 37, 0)]);
        assert_eq!(completed(&world, WIN), Some(false));
        crate::obscript_more::mod_misc_stat(&world, "Caravan Games Won", 2);
        challenge_system(&world, 0.0);
        assert_eq!((completed(&world, WIN), progress(&world, WIN)), (Some(false), Some(2)));
        crate::obscript_more::mod_misc_stat(&world, "Caravan Games Won", 1);
        challenge_system(&world, 0.0);
        assert_eq!(completed(&world, WIN), Some(true));
        assert_eq!(crate::obscript_more::misc_stat(&world, "Challenges Completed"), 1, "completing counts a stat of its own");
        challenge_system(&world, 0.0);
        assert_eq!(crate::obscript_more::misc_stat(&world, "Challenges Completed"), 1, "a finished challenge does not complete again");
    }

    #[test]
    fn a_start_disabled_challenge_waits_for_unlock_and_counts_from_then() {
        let world = world(vec![chal(MASTER, kind::MISC_STAT, 2, CHALLENGE_FLAG_START_DISABLED, 37, 0)]);
        crate::obscript_more::mod_misc_stat(&world, "Caravan Games Won", 5);
        challenge_system(&world, 0.0);
        assert_eq!(completed(&world, MASTER), Some(false), "disabled: not progressing");
        assert!(unlock(&world, MASTER));
        challenge_system(&world, 0.0);
        assert_eq!(completed(&world, MASTER), Some(false), "the five wins before the unlock do not count");
        crate::obscript_more::mod_misc_stat(&world, "Caravan Games Won", 2);
        challenge_system(&world, 0.0);
        assert_eq!(completed(&world, MASTER), Some(true));
        assert!(!unlock(&world, 0xdead), "an unknown challenge");
    }

    #[test]
    fn a_recurring_challenge_starts_over_until_the_recurring_flag_is_removed() {
        let world = world(vec![chal(WIN, kind::MISC_STAT, 1, CHALLENGE_FLAG_RECURRING, 37, 0)]);
        for _ in 0..2 {
            crate::obscript_more::mod_misc_stat(&world, "Caravan Games Won", 1);
            challenge_system(&world, 0.0);
        }
        assert_eq!(world.try_resource::<ChallengeState>().unwrap().entries[&WIN].completions, 2);
        // The reward script of the last completion removes the flag ("if it was completed, it will now
        // be greyed out"): it does not count again.
        assert!(remove_recurring(&world, WIN));
        crate::obscript_more::mod_misc_stat(&world, "Caravan Games Won", 3);
        challenge_system(&world, 0.0);
        assert_eq!(world.try_resource::<ChallengeState>().unwrap().entries[&WIN].completions, 2, "finished for good");
        assert_eq!(completed(&world, WIN), Some(true));
        assert!(!remove_recurring(&world, 0xdead));
    }

    #[test]
    fn a_marker_challenge_counts_the_one_discovery_and_a_threshold_of_zero_is_one() {
        let world = world(vec![chal(VAULT, kind::DISCOVER_MARKER, 0, 0, 0, MARKER)]);
        note(&world, ChallengeEvent::MarkerDiscovered { marker: 0x1234 });
        assert_eq!(completed(&world, VAULT), Some(false));
        note(&world, ChallengeEvent::MarkerDiscovered { marker: MARKER });
        assert_eq!(completed(&world, VAULT), Some(true));
    }

    #[test]
    fn a_marker_made_visible_counts_once_as_a_location_and_for_its_challenge() {
        let world = world(vec![chal(VAULT, kind::DISCOVER_MARKER, 1, 0, 0, MARKER)]);
        world.try_resource_mut::<crate::MapMarkers>().unwrap().add(MARKER, false);
        challenge_system(&world, 0.0);
        challenge_system(&world, 0.0);
        assert_eq!(crate::obscript_more::misc_stat(&world, "Locations Discovered"), 1, "counted once");
        assert_eq!(completed(&world, VAULT), Some(true));
    }

    #[test]
    fn using_an_item_counts_for_its_challenge_only() {
        const TURBO: u32 = 0x0012_0d81;
        let world = world(vec![chal(0x20, kind::USE_ITEM, 2, 0, 0, TURBO)]);
        note(&world, ChallengeEvent::ItemUsed { item: 0x999 });
        note(&world, ChallengeEvent::ItemUsed { item: TURBO });
        assert_eq!((progress(&world, 0x20), completed(&world, 0x20)), (Some(1), Some(false)));
        note(&world, ChallengeEvent::ItemUsed { item: TURBO });
        assert_eq!(completed(&world, 0x20), Some(true));
    }

    /// #6266 — an add to the player's inventory, through the funnel every add announces itself on,
    /// counts its stack size towards *Acquire Item*; an add to anyone else does not.
    #[test]
    fn the_player_acquiring_an_item_counts_its_stack_for_its_challenge() {
        const CAPS: u32 = 0x0000_000f;
        let mut world = world(vec![chal(0x30, kind::ACQUIRE_ITEM, 100, 0, 0, CAPS)]);
        let player = world.spawn();
        let other = world.spawn();
        world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
        let caps = |count, added| crate::ItemTransfer { item_form_id: CAPS, count, added, stolen: false };
        crate::emit_item_transfers(&world, other, [caps(500, true)]);
        crate::emit_item_transfers(&world, player, [caps(40, false)]);
        assert_eq!(progress(&world, 0x30), Some(0), "not the player's, or not an add");
        crate::emit_item_transfers(&world, player, [caps(60, true), caps(1, false)]);
        assert_eq!((progress(&world, 0x30), completed(&world, 0x30)), (Some(60), Some(false)));
        crate::emit_item_transfers(&world, player, [caps(40, true)]);
        assert_eq!(completed(&world, 0x30), Some(true));
    }

    /// #6265 — challenges finished by the same event complete, and start their reward scripts, in
    /// form-id order. Under the old `HashMap` the 32 rewards landed in per-process hash order.
    #[test]
    fn challenges_finished_together_complete_in_form_id_order() {
        const TURBO: u32 = 0x0012_0d81;
        let mut defs: Vec<ChalRecord> = (0..32u32)
            .map(|i| ChalRecord { script_form_id: 0x0050_0000 + i, ..chal(0x0040_0000 + i * 0x101, kind::USE_ITEM, 1, 0, 0, TURBO) })
            .collect();
        defs.reverse();
        let mut world = world(defs);
        world.register::<crate::magic_effects::ActiveMagicEffects>();
        let player = world.spawn();
        world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(player));
        note(&world, ChallengeEvent::ItemUsed { item: TURBO });
        let sources: Vec<u32> = world.get::<crate::magic_effects::ActiveMagicEffects>(player).unwrap().0.iter().map(|effect| effect.source).collect();
        let mut sorted = sources.clone();
        sorted.sort_unstable();
        assert_eq!(sources.len(), 32);
        assert_eq!(sources, sorted);
    }
}
