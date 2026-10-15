//! `Actor.GetBribeAmount` (M47.2) — the gold a Skyrim NPC asks to be bribed.
//!
//! CK *GetBribeAmount - Actor*: "Returns the amount of gold required to bribe
//! this actor" — the page gives no formula. The community's reverse-engineered
//! one is on UESP *Skyrim:Speech* ("Bribe Formula"):
//!
//! ```text
//! morality = Morality(npc) * fBribeMoralityMult + 1
//! v        = NpcLevel * fBribeNPCLevelMult + PlayerLevel
//!            - ((PlayerSpeechcraft - NpcSpeechcraft) - 100) * fBribeSpeechcraftMult
//! amount   = v ^ fBribeCostCurve * (fBribeScale * morality)
//! ```
//!
//! (The page's second line carries a stray closing parenthesis; the grouping
//! above is the only balanced reading.) Three of the five settings are game
//! settings in `Skyrim.esm` (`fBribeSpeechcraftMult` 0.1, `fBribeScale` 0.5,
//! `fBribeCostCurve` 1.4); the other two are not authored there, so the
//! engine's built-in defaults apply — read from the static setting table of
//! `SkyrimSE.exe` (`fBribeMoralityMult` 1.0, `fBribeNPCLevelMult` 0.1; the same
//! table holds 1.0 / 1.5 / 0.03 for the three the master overrides, so the
//! method reads the right slots).
//!
//! Engine policy where the source is silent: the float result truncates to the
//! `int` Papyrus returns (the C cast), and a base that would go negative clamps
//! to 0 (a fractional power of a negative number is not a number).

use byroredux_core::character::CharacterLevel;
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::EsmIndex;

use crate::actor_value_names::ActorValueNames;
use crate::condition::{actor_value_view, ActorValueView};

/// `fBribeMoralityMult` as `SkyrimSE.exe` builds it when `Skyrim.esm` does not
/// author it.
const EXE_DEFAULT_MORALITY_MULT: f32 = 1.0;
/// `fBribeNPCLevelMult`, likewise.
const EXE_DEFAULT_NPC_LEVEL_MULT: f32 = 0.1;

/// The bribe formula's settings and actor values. `NOT_SAVED_BY_DESIGN`:
/// re-derived from the load order every boot.
#[derive(Debug, Clone, PartialEq)]
pub struct BribeRules {
    speechcraft: u32,
    morality: u32,
    morality_mult: f32,
    npc_level_mult: f32,
    speechcraft_mult: f32,
    scale: f32,
    cost_curve: f32,
}

impl Resource for BribeRules {}

impl BribeRules {
    /// The rules of a load order that defines the speechcraft and morality
    /// actor values and the bribe game settings the master authors; `None`
    /// elsewhere (only Skyrim has bribery).
    pub fn from_index(index: &EsmIndex) -> Option<Self> {
        let names = ActorValueNames::from_index(index);
        Some(Self {
            speechcraft: names.resolve("Speechcraft")?,
            morality: names.resolve("Morality")?,
            morality_mult: index
                .game_setting_float("fBribeMoralityMult")
                .unwrap_or(EXE_DEFAULT_MORALITY_MULT),
            npc_level_mult: index
                .game_setting_float("fBribeNPCLevelMult")
                .unwrap_or(EXE_DEFAULT_NPC_LEVEL_MULT),
            speechcraft_mult: index.game_setting_float("fBribeSpeechcraftMult")?,
            scale: index.game_setting_float("fBribeScale")?,
            cost_curve: index.game_setting_float("fBribeCostCurve")?,
        })
    }

    /// The bribe for an NPC of `npc_level`, `npc_speechcraft` and `npc_morality`
    /// by a player of `player_level` and `player_speechcraft`.
    pub fn amount(
        &self,
        npc_level: f32,
        npc_speechcraft: f32,
        npc_morality: f32,
        player_level: f32,
        player_speechcraft: f32,
    ) -> i32 {
        let morality = npc_morality * self.morality_mult + 1.0;
        let base = (npc_level * self.npc_level_mult + player_level
            - ((player_speechcraft - npc_speechcraft) - 100.0) * self.speechcraft_mult)
            .max(0.0);
        (base.powf(self.cost_curve) * (self.scale * morality)) as i32
    }
}

/// The values `GetBribeAmount` reads for `npc`, for the debug console.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BribeInputs {
    pub npc_level: f32,
    pub npc_speechcraft: f32,
    pub npc_morality: f32,
    pub player_level: f32,
    pub player_speechcraft: f32,
    pub amount: i32,
}

/// The inputs and result of `npc.GetBribeAmount()`.
pub fn bribe_inputs(world: &World, npc: EntityId) -> Option<BribeInputs> {
    let rules = world.try_resource::<BribeRules>()?;
    let player = crate::papyrus_demo::PapyrusPlayerEntity::get(world)?;
    let level = |entity| world.get::<CharacterLevel>(entity).map(|level| f32::from(level.level));
    let value = |entity, avif| actor_value_view(world, entity, avif, ActorValueView::Current);
    let inputs = BribeInputs {
        npc_level: level(npc)?,
        npc_speechcraft: value(npc, rules.speechcraft),
        npc_morality: value(npc, rules.morality),
        player_level: level(player)?,
        player_speechcraft: value(player, rules.speechcraft),
        amount: 0,
    };
    let amount = rules.amount(
        inputs.npc_level,
        inputs.npc_speechcraft,
        inputs.npc_morality,
        inputs.player_level,
        inputs.player_speechcraft,
    );
    Some(BribeInputs { amount, ..inputs })
}

/// `npc.GetBribeAmount()`: `None` when the load order has no bribery, or the
/// player or the NPC carries no level.
pub fn bribe_amount(world: &World, npc: EntityId) -> Option<i32> {
    let rules = world.try_resource::<BribeRules>()?;
    let player = crate::papyrus_demo::PapyrusPlayerEntity::get(world)?;
    let level = |entity| world.get::<CharacterLevel>(entity).map(|level| f32::from(level.level));
    let value = |entity, avif| actor_value_view(world, entity, avif, ActorValueView::Current);
    Some(rules.amount(
        level(npc)?,
        value(npc, rules.speechcraft),
        value(npc, rules.morality),
        level(player)?,
        value(player, rules.speechcraft),
    ))
}

/// Install (or, for a load order without bribery, remove) the rules.
pub fn install_bribe_rules(world: &mut World, index: &EsmIndex) -> bool {
    match BribeRules::from_index(index) {
        Some(rules) => {
            world.insert_resource(rules);
            true
        }
        None => {
            world.remove_resource::<BribeRules>();
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skyrim() -> BribeRules {
        BribeRules {
            speechcraft: 1,
            morality: 2,
            morality_mult: EXE_DEFAULT_MORALITY_MULT,
            npc_level_mult: EXE_DEFAULT_NPC_LEVEL_MULT,
            speechcraft_mult: 0.1,
            scale: 0.5,
            cost_curve: 1.4,
        }
    }

    /// Both level 10, equal speech, a morality-0 NPC: v = 10*0.1 + 10 + 100*0.1
    /// = 21; 21^1.4 * (0.5 * 1) = 35.5.
    #[test]
    fn the_uesp_formula_with_skyrims_settings() {
        let rules = skyrim();
        assert_eq!(rules.amount(10.0, 25.0, 0.0, 10.0, 25.0), 35);
        // Morality 1 (the "any crime" tier) doubles the (morality + 1) factor.
        assert_eq!(rules.amount(10.0, 25.0, 1.0, 10.0, 25.0), 70, "70.97 truncates");
    }

    #[test]
    fn a_better_speaker_pays_less_and_a_higher_level_pays_more() {
        let rules = skyrim();
        let base = rules.amount(10.0, 25.0, 0.0, 10.0, 25.0);
        assert!(rules.amount(10.0, 25.0, 0.0, 10.0, 75.0) < base);
        assert!(rules.amount(10.0, 25.0, 0.0, 30.0, 25.0) > base);
        assert!(rules.amount(30.0, 25.0, 0.0, 10.0, 25.0) > base);
    }

    #[test]
    fn a_base_below_zero_clamps_instead_of_going_nan() {
        // Player speech 1000 above the NPC's: -(900 * 0.1) swamps the levels.
        assert_eq!(skyrim().amount(1.0, 0.0, 0.0, 1.0, 1000.0), 0);
    }
}
