//! M47.2 — the misc stats scripts keep: `Game.IncrementStat(asStatName,
//! aiModAmount = 1)` and `Game.QueryStat(asStatName)`.
//!
//! CK *IncrementStat - Game*: it "can be used to modify any misc stat that
//! the game tracks", but its main use is the stats only designers know of —
//! "Standing Stones Found", "Horses Owned", "Houses Owned", "Stores Invested
//! In" — which no engine code maintains, so the scripts' increments are the
//! whole of them. Every increment is recorded; a read resolves only those
//! script-kept stats ([`SCRIPT_STATS`]). The engine-tracked ones (*QueryStat*
//! lists "Locations Discovered", "Days Passed", …) are not counted here, so
//! reading one declines rather than answering 0.

use std::collections::BTreeMap;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;

/// The misc stats scripts maintain: the four the CK says only scripts keep, and
/// the three speechcraft counts only `FavorDialogueScript` raises here.
pub const SCRIPT_STATS: [&str; 7] = [
    "Standing Stones Found",
    "Horses Owned",
    "Houses Owned",
    "Stores Invested In",
    // The CK lists these among the tracked stats, but Skyrim's own
    // `FavorDialogueScript` is what raises them (`Game.IncrementStat("Persuasions"
    // | "Bribes" | "Intimidations", 1)`), and the engine has no speechcraft screen
    // that could raise them another way, so the scripts' increments are the whole
    // of them here.
    "Persuasions",
    "Bribes",
    "Intimidations",
];

/// `name` as one of the [`SCRIPT_STATS`], case-insensitively.
pub fn script_stat(name: &str) -> Option<&'static str> {
    SCRIPT_STATS.into_iter().find(|stat| stat.eq_ignore_ascii_case(name))
}

/// Every misc stat a script has incremented, by lowercased name. Saved.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct MiscStats {
    counts: BTreeMap<String, i32>,
}

impl Resource for MiscStats {}

impl MiscStats {
    pub fn increment(&mut self, stat: &str, amount: i32) {
        let count = self.counts.entry(stat.to_ascii_lowercase()).or_default();
        *count = count.saturating_add(amount);
    }

    /// The stat's count; 0 when no script touched it.
    pub fn get(&self, stat: &str) -> i32 {
        self.counts.get(&stat.to_ascii_lowercase()).copied().unwrap_or(0)
    }
}

/// The achievements / trophies scripts awarded (`Game.AddAchievement`), by
/// ID. Saved. The platform profile is the host's; this is the engine's own
/// record that one was awarded, which a second award leaves unchanged.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct Achievements {
    awarded: std::collections::BTreeSet<i32>,
}

impl Resource for Achievements {}

impl Achievements {
    /// Award `id`; `true` the first time.
    pub fn award(&mut self, id: i32) -> bool {
        self.awarded.insert(id)
    }

    pub fn has(&self, id: i32) -> bool {
        self.awarded.contains(&id)
    }
}

/// #5802 — this subsystem's world setup, owned here instead of the crate's
/// `register` hub: `Game.IncrementStat` misc stats and `Game.AddAchievement` awards (both
/// saved).
pub fn install(world: &mut World) {
    world.init_resource::<MiscStats>();
    world.init_resource::<Achievements>();
}

/// #5802 — this subsystem's saved columns. Called by the binary's
/// `build_save_registry` at their place in the row order (the schema
/// fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_resource::<MiscStats>("MiscStats");
    registry.register_resource::<Achievements>("Achievements");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn increments_accumulate_case_insensitively() {
        let mut stats = MiscStats::default();
        stats.increment("Houses Owned", 1);
        stats.increment("houses owned", 2);
        assert_eq!(stats.get("HOUSES OWNED"), 3);
        assert_eq!(script_stat("standing stones found"), Some("Standing Stones Found"));
        assert_eq!(script_stat("Days Passed"), None, "engine-tracked");
    }
}
