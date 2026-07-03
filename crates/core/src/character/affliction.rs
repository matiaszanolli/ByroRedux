//! Affliction runtime — the pool + threshold + `temporary_mod` half of the
//! affliction family (CHARAL). [`super::resistance`] owns the other half (the
//! resistance-percentage descriptor); this module is the mechanism every
//! game's Radiation / Poison / Disease affliction plugs into, per the
//! `{pool damage + resistance AV + SPECIAL-penalty via temporary_mod}` shape
//! (`docs/engine/charal-fnv-fo3-ruleset.md`, `charal-fo4-ruleset.md`,
//! `charal-fo76-ruleset.md`).
//!
//! ## Why a new mechanism, not just `ActorValues`
//!
//! The **pool** half needs no new storage: an affliction's accumulating pool
//! (Rads, a poison-damage total, …) is just the existing `damage` layer on
//! whatever actor value represents it — [`ActorValue::damage`] is already
//! "accumulates, floors at zero, restored separately by name" (RadAway calls
//! [`ActorValues::restore`] exactly the way it cures Rads). No pool type is
//! introduced here; [`AfflictionTable::pool_value`] just reads that layer.
//!
//! The **missing** half is applying/removing a **threshold-gated temporary
//! SPECIAL penalty** as the pool crosses band boundaries.
//! [`ActorValues::mod_temporary`] is a bare additive delta with no expiration
//! or idempotency of its own — calling it twice for the same band
//! double-applies the penalty, and nothing un-applies it when the pool drops
//! back down. A poisoning tick therefore needs to remember which band it last
//! applied, so it can reverse *exactly* that delta before applying a new one.
//! [`ActiveAffliction`] is that memory; [`reevaluate_affliction`] is the
//! diff-and-reapply step; [`affliction_tick_system`] drives it every actor
//! that opts in (carries [`AfflictionStatus`]).
//!
//! ## No-guessing status
//!
//! The **mechanism** below is engine-supplied and game-agnostic — it doesn't
//! care which affliction or which game. The **thresholds** (what pool level
//! triggers which SPECIAL penalty, for Radiation/Poison/Disease) are per-game
//! AUTHORED data that is still **PENDING**: no citable source has been found
//! yet for FO3/FNV/FO4 poisoning thresholds ([[feedback_no_guessing]],
//! `docs/engine/charal-fnv-fo3-ruleset.md`). No shipped [`AfflictionTable`]
//! exists yet — the tests below use stand-in data to prove the mechanism,
//! not real numbers. Wiring (stamping [`AfflictionStatus`] at spawn,
//! registering `affliction_tick_system`, populating real tables) waits on
//! that data, same as every other PENDING CHARAL row.

use crate::ecs::components::ActorValues;
use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::ecs::world::World;

/// One penalty a threshold band applies: an actor value's `temporary_mod`
/// gets `delta` added while the band is active. `avif_form_id` is AUTHORED —
/// resolved from an EditorID (e.g. `"Strength"`) at ruleset-build time via
/// the same resolve-or-skip pattern as every other CHARAL AV reference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AvPenalty {
    pub avif_form_id: u32,
    pub delta: f32,
}

/// One band of an affliction's threshold table: "once the pool reaches
/// `min_pool`, these penalties are active." A handful of penalties per band
/// (one game's poisoning stages debuff a few SPECIAL attributes at once), so
/// a `Vec` beats a fixed array without capping band width.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AfflictionBand {
    pub min_pool: f32,
    pub penalties: Vec<AvPenalty>,
}

/// A resolved (AVIF FormIDs, not EditorIDs) per-game threshold table for one
/// affliction — Radiation, Poison, Disease, … each get their own. `bands`
/// **must** be sorted ascending by `min_pool`; [`Self::band_for`] relies on
/// it and does not sort defensively (the per-game builder controls the data,
/// same trust boundary as [`super::derived::DerivedStatFormula`]).
///
/// Empty (`bands: vec![]`) until a game's thresholds are sourced — every
/// shipped table is empty today (see module docs).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AfflictionTable {
    /// The actor value whose `damage` layer holds this affliction's
    /// accumulated pool (e.g. `Rads`).
    pub pool_avif: u32,
    pub bands: Vec<AfflictionBand>,
}

impl AfflictionTable {
    /// The affliction's current pool level for `avs` — the accumulated
    /// `damage` on [`Self::pool_avif`], **not** `ActorValues::current` (that
    /// formula subtracts damage; the pool *is* the damage). `0.0` if the
    /// actor doesn't carry the pool AV yet (healthy default).
    #[inline]
    pub fn pool_value(&self, avs: &ActorValues) -> f32 {
        avs.get(self.pool_avif).map_or(0.0, |v| v.damage)
    }

    /// Index of the active band for `pool_value` — the highest `min_pool`
    /// reached — or `None` below every threshold (healthy, no penalty).
    #[inline]
    pub fn band_for(&self, pool_value: f32) -> Option<usize> {
        self.bands.iter().rposition(|b| pool_value >= b.min_pool)
    }
}

/// One affliction's currently-applied band on a specific actor — the memory
/// [`reevaluate_affliction`] needs to reverse exactly what it last applied
/// before applying a new band. Keyed by the affliction's `pool_avif`, so no
/// separate slot-id bookkeeping is needed: an actor can be tracked for
/// several afflictions (Radiation, Poison, …) without them colliding.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveAffliction {
    pub pool_avif: u32,
    /// `None` = healthy (no band applied). `Some(i)` = index into that
    /// affliction's [`AfflictionTable::bands`].
    pub band: Option<usize>,
}

/// Per-actor affliction state. An actor tracks a handful of afflictions at
/// most, so a contiguous `Vec` (linear scan) beats a map — the same shape as
/// [`super::Perks`] / [`super::FactionReputation`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AfflictionStatus {
    pub entries: Vec<ActiveAffliction>,
}

impl AfflictionStatus {
    /// The currently-applied band for the affliction pooled on `pool_avif`,
    /// or `None` if this actor has never been evaluated for it (also
    /// healthy).
    #[inline]
    pub fn band_of(&self, pool_avif: u32) -> Option<usize> {
        self.entries
            .iter()
            .find(|e| e.pool_avif == pool_avif)
            .and_then(|e| e.band)
    }

    fn set_band(&mut self, pool_avif: u32, band: Option<usize>) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.pool_avif == pool_avif) {
            e.band = band;
        } else {
            self.entries.push(ActiveAffliction { pool_avif, band });
        }
    }
}

impl Component for AfflictionStatus {
    type Storage = SparseSetStorage<Self>;
}

/// Re-evaluate one affliction's threshold band against an actor's current
/// pool value: reverses the previously-applied band's penalties (if any),
/// applies the new band's (if any), and records it — a no-op when the band
/// hasn't changed (so calling this every frame is cheap and idempotent).
///
/// Pure function over `ActorValues` + `AfflictionStatus`; the caller decides
/// *when* to tick (every frame, on pool-value change, …) — see
/// [`affliction_tick_system`] for the ECS-driven form.
pub fn reevaluate_affliction(
    status: &mut AfflictionStatus,
    avs: &mut ActorValues,
    table: &AfflictionTable,
) {
    let new_band = table.band_for(table.pool_value(avs));
    let old_band = status.band_of(table.pool_avif);
    if new_band == old_band {
        return;
    }
    if let Some(i) = old_band {
        for p in &table.bands[i].penalties {
            avs.mod_temporary(p.avif_form_id, -p.delta);
        }
    }
    if let Some(i) = new_band {
        for p in &table.bands[i].penalties {
            avs.mod_temporary(p.avif_form_id, p.delta);
        }
    }
    status.set_band(table.pool_avif, new_band);
}

/// System: re-evaluate every configured affliction table against every actor
/// that carries both [`ActorValues`] and [`AfflictionStatus`]. Actors that
/// don't carry `AfflictionStatus` are untouched — opting an actor into
/// affliction tracking is the spawn path's job (stamp a default
/// `AfflictionStatus` alongside `ActorValues`), not this system's.
///
/// `tables` is typically the handful of afflictions a loaded game's
/// [`super::CharacterRuleset`] configures (Radiation, Poison, …); empty until
/// real thresholds are sourced (see module docs), in which case this is a
/// no-op every tick.
pub fn affliction_tick_system(world: &World, tables: &[AfflictionTable]) {
    if tables.is_empty() {
        return;
    }
    let Some((mut avs_q, mut status_q)) = world.query_2_mut_mut::<ActorValues, AfflictionStatus>()
    else {
        return;
    };
    for (entity, status) in status_q.iter_mut() {
        let Some(avs) = avs_q.get_mut(entity) else {
            continue;
        };
        for table in tables {
            reevaluate_affliction(status, avs, table);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::world::World;

    const RADS: u32 = 0x100;
    const STR: u32 = 0x05;
    const AGI: u32 = 0x0A;
    const POISON_POOL: u32 = 0x200;

    /// Stand-in radiation-poisoning table — NOT sourced data (see module
    /// docs), just enough shape to exercise the mechanism: band 0 at 200
    /// rads (−1 STR), band 1 at 600 rads (−1 STR, −1 AGI).
    fn stand_in_radiation_table() -> AfflictionTable {
        AfflictionTable {
            pool_avif: RADS,
            bands: vec![
                AfflictionBand {
                    min_pool: 200.0,
                    penalties: vec![AvPenalty {
                        avif_form_id: STR,
                        delta: -1.0,
                    }],
                },
                AfflictionBand {
                    min_pool: 600.0,
                    penalties: vec![
                        AvPenalty {
                            avif_form_id: STR,
                            delta: -1.0,
                        },
                        AvPenalty {
                            avif_form_id: AGI,
                            delta: -1.0,
                        },
                    ],
                },
            ],
        }
    }

    #[test]
    fn pool_value_reads_the_damage_layer_not_current() {
        let mut avs = ActorValues::new();
        avs.apply_damage(RADS, 250.0);
        // `current()` would read −250 (damage is subtracted); the pool reads
        // the raw accumulated damage instead.
        assert_eq!(avs.current(RADS), -250.0);
        let table = stand_in_radiation_table();
        assert_eq!(table.pool_value(&avs), 250.0);
    }

    #[test]
    fn band_for_picks_the_highest_threshold_reached() {
        let table = stand_in_radiation_table();
        assert_eq!(table.band_for(0.0), None, "healthy below every threshold");
        assert_eq!(table.band_for(199.9), None);
        assert_eq!(table.band_for(200.0), Some(0));
        assert_eq!(table.band_for(599.9), Some(0));
        assert_eq!(table.band_for(600.0), Some(1));
        assert_eq!(table.band_for(9000.0), Some(1), "no cap — stays in the top band");
    }

    #[test]
    fn reevaluate_applies_penalties_entering_a_band() {
        let table = stand_in_radiation_table();
        let mut status = AfflictionStatus::default();
        let mut avs = ActorValues::new();
        avs.apply_damage(RADS, 250.0); // into band 0

        reevaluate_affliction(&mut status, &mut avs, &table);

        assert_eq!(avs.current(STR), -1.0, "band 0 penalty applied");
        assert_eq!(status.band_of(RADS), Some(0));
    }

    #[test]
    fn reevaluate_is_idempotent_within_the_same_band() {
        let table = stand_in_radiation_table();
        let mut status = AfflictionStatus::default();
        let mut avs = ActorValues::new();
        avs.apply_damage(RADS, 250.0);

        reevaluate_affliction(&mut status, &mut avs, &table);
        reevaluate_affliction(&mut status, &mut avs, &table);
        reevaluate_affliction(&mut status, &mut avs, &table);

        assert_eq!(avs.current(STR), -1.0, "same band never double-applies");
    }

    #[test]
    fn reevaluate_swaps_penalties_when_the_band_escalates() {
        let table = stand_in_radiation_table();
        let mut status = AfflictionStatus::default();
        let mut avs = ActorValues::new();
        avs.apply_damage(RADS, 250.0);
        reevaluate_affliction(&mut status, &mut avs, &table);
        assert_eq!(avs.current(STR), -1.0);
        assert_eq!(avs.current(AGI), 0.0);

        // Pool climbs into band 1: STR penalty stays (both bands apply it),
        // AGI penalty newly applies. The old band's deltas are reversed
        // first, so this must NOT double the STR penalty.
        avs.apply_damage(RADS, 400.0); // total 650 → band 1
        reevaluate_affliction(&mut status, &mut avs, &table);

        assert_eq!(avs.current(STR), -1.0, "still exactly −1, not −2");
        assert_eq!(avs.current(AGI), -1.0, "band 1's extra penalty now applied");
        assert_eq!(status.band_of(RADS), Some(1));
    }

    #[test]
    fn reevaluate_clears_penalties_on_cure() {
        let table = stand_in_radiation_table();
        let mut status = AfflictionStatus::default();
        let mut avs = ActorValues::new();
        avs.apply_damage(RADS, 250.0);
        reevaluate_affliction(&mut status, &mut avs, &table);
        assert_eq!(avs.current(STR), -1.0);

        // RadAway: restore the pool back to 0 (cured).
        avs.restore(RADS, 250.0);
        reevaluate_affliction(&mut status, &mut avs, &table);

        assert_eq!(avs.current(STR), 0.0, "penalty fully reversed");
        assert_eq!(status.band_of(RADS), None, "back to healthy");
    }

    #[test]
    fn independent_afflictions_do_not_interfere() {
        let radiation = stand_in_radiation_table();
        let poison = AfflictionTable {
            pool_avif: POISON_POOL,
            bands: vec![AfflictionBand {
                min_pool: 50.0,
                penalties: vec![AvPenalty {
                    avif_form_id: AGI,
                    delta: -2.0,
                }],
            }],
        };
        let mut status = AfflictionStatus::default();
        let mut avs = ActorValues::new();
        avs.apply_damage(RADS, 250.0);
        avs.apply_damage(POISON_POOL, 60.0);

        reevaluate_affliction(&mut status, &mut avs, &radiation);
        reevaluate_affliction(&mut status, &mut avs, &poison);

        assert_eq!(avs.current(STR), -1.0, "radiation band 0");
        assert_eq!(avs.current(AGI), -2.0, "poison band 0 (radiation contributes 0 here)");
        assert_eq!(status.entries.len(), 2);

        // Curing radiation must not touch poison's tracked band.
        avs.restore(RADS, 250.0);
        reevaluate_affliction(&mut status, &mut avs, &radiation);
        assert_eq!(avs.current(STR), 0.0);
        assert_eq!(avs.current(AGI), -2.0, "poison penalty untouched");
        assert_eq!(status.band_of(POISON_POOL), Some(0));
    }

    #[test]
    fn empty_table_never_applies_anything() {
        let table = AfflictionTable {
            pool_avif: RADS,
            bands: vec![],
        };
        let mut status = AfflictionStatus::default();
        let mut avs = ActorValues::new();
        avs.apply_damage(RADS, 10_000.0);

        reevaluate_affliction(&mut status, &mut avs, &table);

        assert_eq!(status.band_of(RADS), None);
        assert!(avs.is_empty() || avs.get(STR).is_none());
    }

    #[test]
    fn tick_system_evaluates_only_actors_with_affliction_status() {
        let mut world = World::new();

        let tracked = world.spawn();
        world.insert(tracked, ActorValues::new());
        world.insert(tracked, AfflictionStatus::default());
        world
            .get_mut::<ActorValues>(tracked)
            .unwrap()
            .apply_damage(RADS, 250.0);

        // An actor with ActorValues but no AfflictionStatus is untouched —
        // opting in is the spawn path's job, not this system's.
        let untracked = world.spawn();
        world.insert(untracked, ActorValues::new());
        world
            .get_mut::<ActorValues>(untracked)
            .unwrap()
            .apply_damage(RADS, 900.0);

        affliction_tick_system(&world, &[stand_in_radiation_table()]);

        assert_eq!(world.get::<ActorValues>(tracked).unwrap().current(STR), -1.0);
        assert_eq!(
            world.get::<ActorValues>(untracked).unwrap().current(STR),
            0.0,
            "untracked actor's pool damage never gets a penalty applied"
        );
    }

    #[test]
    fn descriptors_are_compact() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<AvPenalty>();
        assert_copy::<ActiveAffliction>();
        assert_eq!(std::mem::size_of::<AvPenalty>(), 8);
        // u32 pool_avif + Option<usize> band — usize has no spare niche, so
        // this is 4 (+4 pad) + 16, not the tighter `u8`-index shape used
        // elsewhere in CHARAL. Not hot-path (a `Vec` entry per actor per
        // affliction, a handful at most), so clarity wins over packing here.
        assert_eq!(std::mem::size_of::<ActiveAffliction>(), 24);
    }
}
