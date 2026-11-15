//! M65 Phase 4 (SNK-HUD-001, -002, -005): which state the detection indicator
//! shows (`docs/engine/sneak-detection.md` §10).
//!
//! Each game's row is an [`IndicatorModel`] on the character ruleset
//! (`CharacterRulesProfile::indicator`). It holds a table from one detector's
//! (level, alert state, hostility) to an [`IndicatorState`], how the state is
//! drawn, and Starfield's skill gate. The detection tick folds its counted
//! detectors to the highest state, and every HUD reads that one state. No HUD
//! names a game.
//!
//! ## The Fallout 3 decision
//!
//! `Fallout3.exe` draws the meter from `0x937a70`, called each frame while the
//! player's movement flags hold sneaking (`0x400`) and not swimming (`0x800`,
//! `0x92dff6`). It sets the `sneak_nif` text's `string`, `systemcolor`,
//! `alpha` and `user0` traits (trait ids `0xfc4`, `0xff4`, `0xfa9`, `0x1004`,
//! registered at `0xb4ae3a`…). The first matching case wins:
//!
//! 1. A combat group fights the player (`0x8d0da0` → `0x81ab50`, player
//!    `+0x9a4`), and some member has had the player in sight within
//!    `fCombatDetectionLostTime`: `sSneakDanger`, flashing (an alpha animation
//!    and `user0` = 1).
//! 2. Every such group lost the player longer ago than that (`+0x9a5`):
//!    `sSneakCaution`.
//! 3. A detector that would attack the player has a value above 0 toward it
//!    (`+0x584`, set by the detection update at `0x874060` and by `0x8d4884`):
//!    `sSneakCaution`.
//! 4. Some high-process actor's value toward the player is above 0 (`0x8d4620`
//!    returns the highest): `sSneakDetected`.
//! 5. Otherwise `sSneakHidden`.
//!
//! [`FALLOUT3_TABLE`] states that per detector: the alert state Combat is case
//! 1, Lost is case 2, Seen and hostile is case 3, Seen is case 4. The engine's
//! Combat → Lost step happens when the level falls to Lost (`step_alert`), not
//! `fCombatDetectionLostTime` later, so [CAUTION] can arrive earlier than in
//! the original.
//!
//! Fallout 4, Skyrim and Starfield have the same four string settings
//! (`Fallout4.exe` and `SkyrimSE.exe` carry them, the masters author them;
//! Starfield's executable and master carry none), and their tables are
//! provisional copies of Fallout 3's until V20 (#6935). Oblivion has no label:
//! its eye lights while anyone knows the player is there (UESP
//! *Oblivion:Sneak*).

use super::{AlertState, DetectionLevel};

/// The state the indicator shows, ordered by precedence: when detectors
/// disagree, the highest wins (the order of `Fallout3.exe`'s cases).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IndicatorState {
    #[default]
    Hidden,
    Detected,
    Caution,
    Danger,
}

impl IndicatorState {
    /// Every state, in precedence order.
    pub const ALL: [Self; 4] = [Self::Hidden, Self::Detected, Self::Caution, Self::Danger];

    /// The state's slot in [`IndicatorStyle::Labels`] and [`StealthIndicator::labels`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// One row of a game's table: a detector matching every `Some` field shows
/// `shows`. Rows are read in order and the first match wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndicatorRow {
    pub level: Option<DetectionLevel>,
    pub alert: Option<AlertState>,
    pub hostile: Option<bool>,
    pub shows: IndicatorState,
}

impl IndicatorRow {
    const fn new(level: Option<DetectionLevel>, alert: Option<AlertState>, hostile: Option<bool>, shows: IndicatorState) -> Self {
        Self { level, alert, hostile, shows }
    }

    fn matches(&self, level: DetectionLevel, alert: AlertState, hostile: bool) -> bool {
        self.level.is_none_or(|l| l == level) && self.alert.is_none_or(|a| a == alert) && self.hostile.is_none_or(|h| h == hostile)
    }
}

use AlertState as A;
use DetectionLevel as L;
use IndicatorState as S;

/// `Fallout3.exe` `0x937a70`, per detector (the module docs give the cases).
pub const FALLOUT3_TABLE: &[IndicatorRow] = &[
    IndicatorRow::new(None, Some(A::Combat), None, S::Danger),
    IndicatorRow::new(None, Some(A::Lost), None, S::Caution),
    IndicatorRow::new(Some(L::Seen), None, Some(true), S::Caution),
    IndicatorRow::new(Some(L::Seen), None, None, S::Detected),
];

/// Oblivion's eye, lit ([`IndicatorState::Detected`]) while a detector knows
/// the player is there: it sees the player or is fighting it (UESP
/// *Oblivion:Sneak*; V20 on #6935).
pub const OBLIVION_TABLE: &[IndicatorRow] = &[
    IndicatorRow::new(Some(L::Seen), None, None, S::Detected),
    IndicatorRow::new(None, Some(A::Combat), None, S::Detected),
    IndicatorRow::new(None, Some(A::Lost), None, S::Detected),
];

/// The four string settings, in [`IndicatorState`] order.
pub const LABEL_SETTINGS: [&str; 4] = ["sSneakHidden", "sSneakDetected", "sSneakCaution", "sSneakDanger"];

/// How a game draws the state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorStyle {
    /// No indicator: the game has no sourced one.
    None,
    /// A text label per state from [`LABEL_SETTINGS`], `defaults` where no
    /// plugin authors them (the executable's). [`IndicatorState::Danger`]
    /// flashes (`Fallout3.exe` `0x937bd2` starts a 0.5 s alpha animation).
    Labels { defaults: [&'static str; 4] },
    /// Oblivion's eye (`hud_reticle.xml` `hudreticle_sneak`): lit for any
    /// state above [`IndicatorState::Hidden`].
    Eye,
}

/// The per-game indicator row of `CharacterRulesProfile`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndicatorModel {
    pub table: &'static [IndicatorRow],
    pub style: IndicatorStyle,
    /// SNK-HUD-005: the indicator shows only while the player holds this perk
    /// (by EditorID) at rank 1 or more.
    pub gate_perk: Option<&'static str>,
}

/// `Fallout3.exe`'s label defaults (each string sits beside its setting's name).
const BRACKETED: [&str; 4] = ["[HIDDEN]", "[DETECTED]", "[CAUTION]", "[DANGER]"];
/// `Fallout4.exe`'s label defaults.
const PLAIN: [&str; 4] = ["HIDDEN", "DETECTED", "CAUTION", "DANGER"];

impl IndicatorModel {
    pub const NONE: Self = Self { table: &[], style: IndicatorStyle::None, gate_perk: None };
    pub const OBLIVION: Self = Self { table: OBLIVION_TABLE, style: IndicatorStyle::Eye, gate_perk: None };
    /// Fallout 3 and New Vegas (`FalloutNV.exe` is encrypted; New Vegas keeps
    /// Fallout 3's HUD code and authors none of the four settings).
    pub const FALLOUT: Self = Self { table: FALLOUT3_TABLE, style: IndicatorStyle::Labels { defaults: BRACKETED }, gate_perk: None };
    /// `SkyrimSE.exe` carries the bracketed defaults; `Skyrim.esm` authors the settings.
    pub const SKYRIM: Self = Self::FALLOUT;
    /// `Fallout4.exe`'s unbracketed defaults; `Fallout4.esm` authors the settings.
    pub const FALLOUT4: Self = Self { table: FALLOUT3_TABLE, style: IndicatorStyle::Labels { defaults: PLAIN }, gate_perk: None };
    /// Fallout 4's labels stand in (no Starfield source carries them), behind
    /// the Stealth skill: "Rank 1 - Adds a Stealth Meter" (Starfield wiki
    /// *Stealth*); the skill is the `PERK` `Skill_Stealth` (`0x2CFCB2`).
    pub const STARFIELD: Self = Self { gate_perk: Some("Skill_Stealth"), ..Self::FALLOUT4 };

    /// Whether the game has an indicator at all.
    #[must_use]
    pub const fn shown(&self) -> bool {
        !matches!(self.style, IndicatorStyle::None)
    }

    /// The state one detector asks for.
    #[must_use]
    pub fn state(&self, level: DetectionLevel, alert: AlertState, hostile: bool) -> IndicatorState {
        self.table
            .iter()
            .find(|row| row.matches(level, alert, hostile))
            .map_or(IndicatorState::Hidden, |row| row.shows)
    }
}

/// The skill gate, resolved against the load order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorGate {
    /// No gate.
    Open,
    /// The player must hold this perk; `None` when the load order lacks it, so
    /// the indicator never shows.
    Perk(Option<u32>),
}

/// The indicator row resolved per load order: the labels and the gate perk's
/// FormID. Installed as a resource beside `StealthRules`.
#[derive(Debug, Clone, PartialEq)]
pub struct StealthIndicator {
    pub model: IndicatorModel,
    /// One label per [`IndicatorState`]; empty for a style without labels.
    pub labels: [std::sync::Arc<str>; 4],
    pub gate: IndicatorGate,
}

impl crate::ecs::resource::Resource for StealthIndicator {}

impl StealthIndicator {
    /// Resolve `model`: `string_setting` reads a string game setting (an empty
    /// authored string falls back too), `perk_form_id` a `PERK` by EditorID.
    #[must_use]
    pub fn resolve(
        model: IndicatorModel,
        string_setting: impl Fn(&str) -> Option<String>,
        perk_form_id: impl Fn(&str) -> Option<u32>,
    ) -> Self {
        let labels = match model.style {
            IndicatorStyle::Labels { defaults } => std::array::from_fn(|i| {
                let authored = string_setting(LABEL_SETTINGS[i]).filter(|s| !s.trim().is_empty());
                std::sync::Arc::from(authored.as_deref().unwrap_or(defaults[i]))
            }),
            IndicatorStyle::None | IndicatorStyle::Eye => std::array::from_fn(|_| std::sync::Arc::from("")),
        };
        let gate = match model.gate_perk {
            Some(editor_id) => IndicatorGate::Perk(perk_form_id(editor_id)),
            None => IndicatorGate::Open,
        };
        Self { model, labels, gate }
    }

    /// The label for `state` (empty without labels).
    #[must_use]
    pub fn label(&self, state: IndicatorState) -> &std::sync::Arc<str> {
        &self.labels[state.index()]
    }

    /// Whether a player holding `perk_rank(form_id)` passes the gate.
    #[must_use]
    pub fn gate_passes(&self, perk_rank: impl Fn(u32) -> u8) -> bool {
        match self.gate {
            IndicatorGate::Open => true,
            IndicatorGate::Perk(Some(form_id)) => perk_rank(form_id) >= 1,
            IndicatorGate::Perk(None) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fallout3_table_follows_the_executables_case_order() {
        let m = IndicatorModel::FALLOUT;
        assert_eq!(m.state(L::Lost, A::Normal, true), S::Hidden);
        assert_eq!(m.state(L::Lost, A::Alert, true), S::Hidden, "a noticed band alone does not set +0x584");
        assert_eq!(m.state(L::Seen, A::Normal, false), S::Detected);
        assert_eq!(m.state(L::Seen, A::Normal, true), S::Caution);
        assert_eq!(m.state(L::Lost, A::Lost, true), S::Caution);
        assert_eq!(m.state(L::Seen, A::Combat, true), S::Danger);
        assert!(S::Danger > S::Caution && S::Caution > S::Detected && S::Detected > S::Hidden);
    }

    #[test]
    fn labels_come_from_the_settings_then_the_executable() {
        let authored = StealthIndicator::resolve(
            IndicatorModel::SKYRIM,
            |name| (name == "sSneakDanger").then(|| "DANGER!".to_string()),
            |_| None,
        );
        assert_eq!(&*authored.labels[S::Danger.index()], "DANGER!");
        assert_eq!(&*authored.labels[S::Hidden.index()], "[HIDDEN]");
        let fo4 = StealthIndicator::resolve(IndicatorModel::FALLOUT4, |_| Some(" ".into()), |_| None);
        assert_eq!(&**fo4.label(S::Caution), "CAUTION", "a blank authored string falls back");
        assert!(StealthIndicator::resolve(IndicatorModel::OBLIVION, |_| None, |_| None).labels.iter().all(|l| l.is_empty()));
    }

    #[test]
    fn starfields_gate_needs_the_stealth_skill() {
        let resolved = StealthIndicator::resolve(IndicatorModel::STARFIELD, |_| None, |id| (id == "Skill_Stealth").then_some(0x2CFCB2));
        assert_eq!(resolved.gate, IndicatorGate::Perk(Some(0x2CFCB2)));
        assert!(!resolved.gate_passes(|_| 0));
        assert!(resolved.gate_passes(|form| u8::from(form == 0x2CFCB2)));
        let missing = StealthIndicator::resolve(IndicatorModel::STARFIELD, |_| None, |_| None);
        assert!(!missing.gate_passes(|_| 4), "a load order without the skill never shows the meter");
        assert!(StealthIndicator::resolve(IndicatorModel::FALLOUT, |_| None, |_| None).gate_passes(|_| 0));
    }
}
