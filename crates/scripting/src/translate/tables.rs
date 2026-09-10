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
//! - **Events** — [`CanonicalEvent`] maps the recognizer chain's Papyrus
//!   event-handler names (and, later, Obscript block types) to a
//!   game-agnostic event that the runtime's marker components key on.
//!   The sandbox provider lowering keeps its own `PapyrusProviderEvent`
//!   vocabulary; see [`CanonicalEvent`] for why the two are separate.
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
/// site).
///
/// This is the only place the **recognizer chain** interprets a Papyrus
/// event name — `quest_stage_gate::find_advance_event` routes through
/// [`Self::from_papyrus`] rather than comparing strings itself (#3947).
///
/// It is deliberately *not* the only such table in the crate, and the
/// blanket "only place" claim this doc used to carry was wrong. The
/// sandbox provider lowering (`papyrus_provider::lower_program`) maps
/// names onto `PapyrusProviderEvent`, a different vocabulary serving a
/// different consumer: it carries `OnObjectEquipped` /
/// `OnObjectUnequipped`, which have no marker component here, and lacks
/// `CellLoad` / `Timer` / `Unknown`, which have no provider hook there.
/// Collapsing the two would mean either fabricating variants for one side
/// or lossily folding names on the other, so they stay separate — with
/// the overlap noted here rather than left for the next reader to
/// discover.
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
    /// `OnEquip` → [`crate::events::EquipmentEventBatch`] entries whose
    /// `equipped` flag is true.
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

    /// #3947 — `from_papyrus` had no production caller while this
    /// module's doc claimed it was the only place Papyrus event names are
    /// interpreted, and `quest_stage_gate` matched the names inline. The
    /// routing is what makes the (now correctly scoped) claim true for
    /// the recognizer chain, so pin it at the source: a reader who
    /// reintroduces a string compare there silently re-splits the table.
    #[test]
    fn the_recognizer_chain_routes_event_names_through_this_table() {
        const GATE: &str = include_str!("recognizers/quest_stage_gate.rs");
        assert!(
            GATE.contains("CanonicalEvent::from_papyrus"),
            "quest_stage_gate must interpret event names through CanonicalEvent"
        );
        for inline in [
            "eq_ignore_case(\"OnActivate\")",
            "eq_ignore_case(\"OnTriggerEnter\")",
        ] {
            assert!(
                !GATE.contains(inline),
                "quest_stage_gate compares an event name inline again ({inline}) — \
                 route it through CanonicalEvent::from_papyrus instead"
            );
        }
    }
    use super::*;

    #[test]
    fn canonical_event_maps_known_papyrus_names_case_insensitively() {
        assert_eq!(
            CanonicalEvent::from_papyrus("OnActivate"),
            CanonicalEvent::Activate
        );
        assert_eq!(
            CanonicalEvent::from_papyrus("onactivate"),
            CanonicalEvent::Activate
        );
        assert_eq!(
            CanonicalEvent::from_papyrus("OnUpdate"),
            CanonicalEvent::Update
        );
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
        assert_eq!(
            ConditionFunction::from_index(59),
            ConditionFunction::GetStageDone
        );
    }
}
