//! Primary-attribute set (CHARAL) — the per-game attribute roster.
//!
//! The Bethesda lineage does not agree on what a character's *primary
//! attributes* are: Fallout has the 7 SPECIAL, the classic Elder Scrolls
//! titles (Morrowind / Oblivion) have 8, and Skyrim / Starfield dropped the
//! concept entirely (their "primary stats" are the derived Health / Magicka /
//! Stamina pools, which CHARAL models as derived stats, not attributes).
//!
//! [`Attribute`] is the **canonical union** across the lineage — a stable
//! identity the runtime keys on regardless of source game. Five members are
//! shared by both families (Strength / Intelligence / Agility / Endurance /
//! Luck); Perception and Charisma are Fallout-only; Willpower, Speed and
//! Personality are TES-only.
//!
//! [`AttributeSet`] is the per-game **roster** — which of those canonical
//! attributes a game actually uses. It is ENGINE-SUPPLIED (the shape is
//! per-family knowledge, not in any one record); the AVIF **FormIDs** each
//! attribute resolves to stay AUTHORED, pulled from the parsed AVIF set via a
//! resolver — never hardcoded here. This is the CHARAL doctrine: the per-game
//! seam is the *data in the table*, never a branch in the consumer
//! (`docs/engine/charal.md`).

/// A canonical primary attribute — the union across the Bethesda lineage.
///
/// Identity only; the numeric AVIF FormID (Fallout) or legacy actor-value
/// index (pre-AVIF TES) is AUTHORED and resolved by the loader, not stored on
/// the variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Attribute {
    // --- shared by Fallout SPECIAL and the TES octet ---
    Strength,
    Endurance,
    Intelligence,
    Agility,
    Luck,
    // --- Fallout-only ---
    Perception,
    Charisma,
    // --- TES-only (Morrowind / Oblivion) ---
    Willpower,
    Speed,
    Personality,
}

impl Attribute {
    /// The canonical AVIF EditorID used to resolve this attribute against a
    /// game's parsed AVIF set (the `resolve("Strength")` pattern the per-game
    /// builders already use). FormIDs are AUTHORED — never hardcoded.
    ///
    /// Note: pre-AVIF TES (Morrowind / Oblivion) carries attributes as
    /// hardcoded engine actor-value indices rather than `AVIF` records; that
    /// family supplies a resolver mapping these EditorIDs to its legacy
    /// indices at the parser boundary. The roster itself stays game-agnostic.
    #[must_use]
    pub const fn editor_id(self) -> &'static str {
        match self {
            Attribute::Strength => "Strength",
            Attribute::Endurance => "Endurance",
            Attribute::Intelligence => "Intelligence",
            Attribute::Agility => "Agility",
            Attribute::Luck => "Luck",
            Attribute::Perception => "Perception",
            Attribute::Charisma => "Charisma",
            Attribute::Willpower => "Willpower",
            Attribute::Speed => "Speed",
            Attribute::Personality => "Personality",
        }
    }
}

/// The per-game primary-attribute roster — ENGINE-SUPPLIED membership over a
/// `&'static` slice of canonical [`Attribute`]s. `Copy` and pointer-sized;
/// held inline by [`super::CharacterRuleset`].
///
/// Resolve to AUTHORED AVIF FormIDs with [`AttributeSet::resolve`] when the
/// loader needs the concrete ids; the roster itself never carries them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttributeSet {
    members: &'static [Attribute],
}

impl AttributeSet {
    /// Fallout — the 7 SPECIAL (FO3 / FNV / FO4 / FO76). AV codes 5–11 in the
    /// AVIF set; ordering is the canonical SPECIAL order.
    pub const FALLOUT: Self = Self {
        members: &[
            Attribute::Strength,
            Attribute::Perception,
            Attribute::Endurance,
            Attribute::Charisma,
            Attribute::Intelligence,
            Attribute::Agility,
            Attribute::Luck,
        ],
    };

    /// Classic Elder Scrolls — the 8 attributes (Morrowind / Oblivion).
    pub const TES_CLASSIC: Self = Self {
        members: &[
            Attribute::Strength,
            Attribute::Intelligence,
            Attribute::Willpower,
            Attribute::Agility,
            Attribute::Speed,
            Attribute::Endurance,
            Attribute::Personality,
            Attribute::Luck,
        ],
    };

    /// Skyrim — no primary attributes (the concept was removed; Health /
    /// Magicka / Stamina are derived pools, modelled as derived stats).
    pub const SKYRIM: Self = Self { members: &[] };

    /// Starfield — no primary attributes (skill / background / trait driven).
    pub const STARFIELD: Self = Self { members: &[] };

    /// The canonical attributes this game uses, in canonical order.
    #[must_use]
    pub const fn members(&self) -> &'static [Attribute] {
        self.members
    }

    /// Number of primary attributes (7 Fallout, 8 TES classic, 0 Skyrim/SF).
    #[must_use]
    pub const fn len(&self) -> usize {
        self.members.len()
    }

    /// Whether this game has no primary attributes (Skyrim / Starfield).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Whether `attr` is part of this game's roster.
    #[must_use]
    pub fn contains(&self, attr: Attribute) -> bool {
        self.members.contains(&attr)
    }

    /// Resolve every attribute to its AUTHORED AVIF FormID via `resolve`
    /// (EditorID → FormID, from the parsed AVIF set). Attributes the resolver
    /// can't map are dropped — the loader logs the gap; the canonical runtime
    /// stays branch-free. Pairs each id with its canonical [`Attribute`] so
    /// the runtime keeps the identity, not just the number.
    pub fn resolve<F: Fn(&str) -> Option<u32>>(&self, resolve: F) -> Vec<(Attribute, u32)> {
        self.members
            .iter()
            .filter_map(|&a| resolve(a.editor_id()).map(|id| (a, id)))
            .collect()
    }
}

impl Default for AttributeSet {
    /// Empty — a game declares its roster explicitly; defaulting to "no
    /// attributes" (Skyrim's shape) is the safe neutral.
    fn default() -> Self {
        Self::SKYRIM
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rosters_have_the_documented_counts() {
        assert_eq!(AttributeSet::FALLOUT.len(), 7);
        assert_eq!(AttributeSet::TES_CLASSIC.len(), 8);
        assert_eq!(AttributeSet::SKYRIM.len(), 0);
        assert!(AttributeSet::SKYRIM.is_empty());
        assert!(AttributeSet::STARFIELD.is_empty());
    }

    #[test]
    fn family_specific_membership() {
        // Perception / Charisma are Fallout-only.
        assert!(AttributeSet::FALLOUT.contains(Attribute::Perception));
        assert!(!AttributeSet::TES_CLASSIC.contains(Attribute::Perception));
        // Willpower / Speed / Personality are TES-only.
        assert!(AttributeSet::TES_CLASSIC.contains(Attribute::Willpower));
        assert!(!AttributeSet::FALLOUT.contains(Attribute::Willpower));
        // The five shared attributes appear in both.
        for a in [
            Attribute::Strength,
            Attribute::Endurance,
            Attribute::Intelligence,
            Attribute::Agility,
            Attribute::Luck,
        ] {
            assert!(AttributeSet::FALLOUT.contains(a), "Fallout missing {a:?}");
            assert!(AttributeSet::TES_CLASSIC.contains(a), "TES missing {a:?}");
        }
    }

    #[test]
    fn resolve_maps_editor_ids_and_drops_the_unknown() {
        // Stand-in AVIF resolver: the Fallout SPECIAL codes 5–11, but
        // "Charisma" deliberately absent to exercise the drop path.
        let resolve = |id: &str| -> Option<u32> {
            Some(match id {
                "Strength" => 0x05,
                "Perception" => 0x06,
                "Endurance" => 0x07,
                // Charisma (0x08) intentionally missing.
                "Intelligence" => 0x09,
                "Agility" => 0x0A,
                "Luck" => 0x0B,
                _ => return None,
            })
        };
        let resolved = AttributeSet::FALLOUT.resolve(resolve);
        assert_eq!(resolved.len(), 6, "Charisma should be dropped");
        assert_eq!(resolved[0], (Attribute::Strength, 0x05));
        assert!(!resolved.iter().any(|(a, _)| *a == Attribute::Charisma));
        // Identity is preserved alongside the authored id.
        assert!(resolved.contains(&(Attribute::Agility, 0x0A)));
    }

    #[test]
    fn editor_ids_are_distinct() {
        let all = [
            Attribute::Strength,
            Attribute::Endurance,
            Attribute::Intelligence,
            Attribute::Agility,
            Attribute::Luck,
            Attribute::Perception,
            Attribute::Charisma,
            Attribute::Willpower,
            Attribute::Speed,
            Attribute::Personality,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a.editor_id(), b.editor_id(), "{a:?} vs {b:?}");
            }
        }
    }
}
