//! Canonical per-game tables — the **single source of truth** for
//! scripting behavior that varies across games.
//!
//! The architectural directive: every behavior that differs between
//! Oblivion / FO3 / FNV / Skyrim / FO4+ is resolved here, at the
//! translate boundary, and the runtime consumes only the canonical
//! result. A front-end or recognizer that hardcodes a per-game index or
//! event name inline — instead of routing through this module — is the
//! regression.
//!
//! What lives here:
//! - **Condition functions** — already canonical: M47.1's
//!   [`ConditionFunction::from_index`] maps the per-game CTDA function
//!   index to a typed variant. Re-exported, not duplicated.
//! - **Events** — [`CanonicalEvent`] maps Papyrus event-handler names
//!   (and, later, Obscript block types) to a game-agnostic event that
//!   the runtime's marker components key on.
//!
//! Deferred (needs an authoritative per-game source, like VMAD):
//! - **Perk entry points** — the ~120 entry-point indices vary per game
//!   and there is no authoritative index→meaning table on hand. The raw
//!   `entry_point_index: u8` is retained by the PERK parser today; the
//!   canonical `EntryPoint` enum lands here once the per-game table is
//!   sourced (do NOT fabricate the mapping — same discipline as VMAD).

pub use crate::condition::ConditionFunction;

/// A game-agnostic script event. Papyrus event-handler names map onto
/// this; each variant corresponds to a runtime marker component in
/// [`crate::events`] / [`crate::recurring_update`] (or a deferred emit
/// site). This is the *only* place Papyrus event names are interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalEvent {
    /// `OnActivate` → [`crate::events::ActivateEvent`].
    Activate,
    /// `OnHit` → [`crate::events::HitEvent`].
    Hit,
    /// `OnUpdate` → [`crate::recurring_update::OnUpdateEvent`].
    Update,
    /// `OnInit` — one-shot script initialization (no marker; recognizers
    /// fold init logic into the spawn or a first-frame system).
    Init,
    /// `OnLoad` — the reference's 3D loaded (treated like cell-load for
    /// the ECS streaming model).
    Load,
    /// `OnCellLoad` → [`crate::events::OnCellLoadEvent`].
    CellLoad,
    /// `OnTriggerEnter` → [`crate::events::OnTriggerEnterEvent`].
    TriggerEnter,
    /// `OnEquip` → [`crate::events::OnEquipEvent`].
    Equip,
    /// `OnTimer` → [`crate::events::TimerExpired`].
    Timer,
    /// An event name outside the recognized catalog (the long tail of
    /// ~130 Papyrus events — added as real content needs them).
    Unknown,
}

impl CanonicalEvent {
    /// Map a Papyrus event-handler name to its canonical event.
    /// Papyrus identifiers are case-insensitive, so the match is too.
    /// These are documented Papyrus API event names (grounded in the
    /// M30 source scripts), not per-game-varying values.
    pub fn from_papyrus(name: &str) -> Self {
        // A small fixed catalog; lower-cased once for the compare.
        match name.to_ascii_lowercase().as_str() {
            "onactivate" => Self::Activate,
            "onhit" => Self::Hit,
            "onupdate" => Self::Update,
            "oninit" => Self::Init,
            "onload" => Self::Load,
            "oncellload" | "oncellattach" => Self::CellLoad,
            "ontriggerenter" => Self::TriggerEnter,
            "onequip" => Self::Equip,
            "ontimer" => Self::Timer,
            _ => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_event_maps_known_papyrus_names_case_insensitively() {
        assert_eq!(CanonicalEvent::from_papyrus("OnActivate"), CanonicalEvent::Activate);
        assert_eq!(CanonicalEvent::from_papyrus("onactivate"), CanonicalEvent::Activate);
        assert_eq!(CanonicalEvent::from_papyrus("OnUpdate"), CanonicalEvent::Update);
        assert_eq!(CanonicalEvent::from_papyrus("OnInit"), CanonicalEvent::Init);
        assert_eq!(CanonicalEvent::from_papyrus("OnLoad"), CanonicalEvent::Load);
    }

    #[test]
    fn canonical_event_unknown_for_long_tail() {
        assert_eq!(
            CanonicalEvent::from_papyrus("OnLocationChange"),
            CanonicalEvent::Unknown
        );
    }

    #[test]
    fn condition_function_reexport_is_the_m47_1_canonical() {
        // The table re-exports M47.1's canonical mapping, not a copy.
        assert_eq!(ConditionFunction::from_index(59), ConditionFunction::GetStageDone);
    }
}
