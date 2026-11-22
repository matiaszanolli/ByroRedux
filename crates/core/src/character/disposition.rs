//! Starting disposition for the classic TES character rules.
//!
//! The calculation is from Oblivion.exe 0x546260–0x5463e7; the setting
//! initializers and master overrides are documented in CHARAL §9. Runtime
//! actor, race and faction lookups are supplied by the translation boundary.

/// The inputs whose starting values are relevant to the hostility decision.
#[derive(Debug, Clone, Copy, Default)]
pub struct DispositionInput {
    pub actor_personality: f32,
    pub target_personality: f32,
    pub actor_responsibility: f32,
    pub target_fame: f32,
    pub target_infamy: f32,
    pub race_reaction: i32,
    pub faction_reaction: i32,
    pub target_faction_rank: i32,
}

/// Executable settings, overlaid with values from the active load order.
#[derive(Debug, Clone, Copy)]
pub struct DispositionModel {
    actor_personality_base: f32,
    actor_personality_mult: f32,
    target_personality_base: f32,
    target_personality_mult: f32,
    fame_mult: f32,
    infamy_mult: f32,
    responsibility_mult: f32,
    race_mult: f32,
    faction_mult: f32,
    faction_rank_base: f32,
    fame_max: f32,
    infamy_max: f32,
}

impl DispositionModel {
    /// Executable defaults. The vanilla master changes both Personality
    /// multipliers, so these must be overlaid even for the unmodded game.
    pub const OBLIVION: Self = Self {
        actor_personality_base: 0.0,
        actor_personality_mult: 1.0,
        target_personality_base: 0.0,
        target_personality_mult: 1.0,
        fame_mult: 0.3,
        infamy_mult: -0.3,
        responsibility_mult: 0.02,
        race_mult: 1.0,
        faction_mult: 1.0,
        faction_rank_base: 1.0,
        fame_max: 20.0,
        infamy_max: 20.0,
    };

    #[must_use]
    pub fn with_gmst(mut self, gmst: impl Fn(&str) -> Option<f32>) -> Self {
        let setting = |name, default| gmst(name).filter(|v| v.is_finite()).unwrap_or(default);
        self.actor_personality_base = setting("fDispActorPerBase", self.actor_personality_base);
        self.actor_personality_mult = setting("fDispActorPerMult", self.actor_personality_mult);
        self.target_personality_base = setting("fDispTargetPerBase", self.target_personality_base);
        self.target_personality_mult = setting("fDispTargetPerMult", self.target_personality_mult);
        self.fame_mult = setting("fDispTargetFameMult", self.fame_mult);
        self.infamy_mult = setting("fDispTargetInfamyMult", self.infamy_mult);
        self.responsibility_mult = setting("fDispActorInfamyMult", self.responsibility_mult);
        self.race_mult = setting("fDispTargetRaceMult", self.race_mult);
        self.faction_mult = setting("fDispTargetFactionMult", self.faction_mult);
        self.faction_rank_base = setting("fDispTargetFactionRankBase", self.faction_rank_base);
        self.fame_max = setting("iDispFameMax", self.fame_max).max(0.0);
        self.infamy_max = setting("fDispInfamyMax", self.infamy_max).max(0.0);
        self
    }

    /// Starting disposition, including the executable's truncations and the
    /// final 0–100 clamp in the actor virtual method at 0x5eacc6–0x5eacef.
    #[must_use]
    pub fn starting(self, input: DispositionInput) -> i32 {
        let f = |v: f32| f64::from(v);
        // The actor methods at 0x5eac9d–0x5eacbd supply integer AVs.
        let personality = ((f(input.actor_personality.trunc()) * f(self.actor_personality_mult)
            + f(self.actor_personality_base)
            + f(input.target_personality.trunc()) * f(self.target_personality_mult)
            + f(self.target_personality_base))
            * 0.5)
            .trunc();
        let fame = (f(input.target_fame.trunc()) * f(self.fame_mult))
            .trunc()
            .clamp(-f(self.fame_max), f(self.fame_max));
        // The executable uses fDispTargetInfamyMult in both places, rather
        // than the separately registered fDispActorInfamyBase (0x5462b8).
        let infamy = ((f(input.actor_responsibility.trunc()) * f(self.responsibility_mult)
            + f(self.infamy_mult))
            * f(input.target_infamy.trunc())
            * f(self.infamy_mult))
        .clamp(-f(self.infamy_max), f(self.infamy_max));
        let race = (f64::from(input.race_reaction) * f(self.race_mult)).trunc();
        // 0x54631a duplicates fDispTargetFactionMult; the registered
        // fDispTargetFactionRankMult is not read by this calculation.
        let faction = (f64::from(input.faction_reaction)
            * f(self.faction_mult)
            * (f64::from(input.target_faction_rank) * f(self.faction_mult)
                + f(self.faction_rank_base)))
        .trunc();
        (personality + fame + infamy + race + faction)
            .trunc()
            .clamp(0.0, 100.0) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vanilla() -> DispositionModel {
        DispositionModel::OBLIVION.with_gmst(|name| match name {
            "fDispActorPerMult" => Some(1.5),
            "fDispTargetPerMult" => Some(0.5),
            _ => None,
        })
    }

    #[test]
    fn personality_and_reputation_match_the_executable() {
        let input = DispositionInput {
            actor_personality: 40.0,
            target_personality: 35.0,
            actor_responsibility: 100.0,
            ..Default::default()
        };
        assert_eq!(vanilla().starting(input), 38);
        assert_eq!(
            vanilla().starting(DispositionInput {
                target_fame: 100.0,
                target_infamy: 20.0,
                ..input
            }),
            47
        );
        assert_eq!(
            vanilla().starting(DispositionInput {
                actor_responsibility: 10.0,
                target_infamy: 100.0,
                ..input
            }),
            41
        );
    }

    /// UESP *Oblivion:Disposition* (OAV-USE-005): NPC Personality 40 with the player at 39 and at 35 loses
    /// 1 and 2 points, the player at 44 gains 1, and Fame pays +3 per 10 up to +20.
    #[test]
    fn personality_differential_and_fame_match_the_uesp_worked_examples() {
        let at = |player: f32, fame: f32| {
            vanilla().starting(DispositionInput {
                actor_personality: 40.0,
                target_personality: player,
                target_fame: fame,
                ..Default::default()
            })
        };
        assert_eq!((at(40.0, 0.0), at(39.0, 0.0), at(35.0, 0.0), at(44.0, 0.0)), (40, 39, 38, 41));
        assert_eq!((at(40.0, 10.0), at(40.0, 67.0), at(40.0, 100.0)), (43, 60, 60));
    }

    #[test]
    fn faction_rank_and_mod_settings_are_applied_before_final_truncation() {
        let input = DispositionInput {
            actor_personality: 40.0,
            target_personality: 40.0,
            race_reaction: -5,
            faction_reaction: 3,
            target_faction_rank: 2,
            ..Default::default()
        };
        assert_eq!(vanilla().starting(input), 44);
        let changed = vanilla().with_gmst(|name| (name == "fDispTargetFactionMult").then_some(2.0));
        assert_eq!(changed.starting(input), 65);
    }
}
