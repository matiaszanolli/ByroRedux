//! Skill roster + governing-attribute map (CHARAL).
//!
//! A game's skills and the attribute that governs each. Unlike [`Attribute`]
//! — a small, heavily-overlapping set worth a canonical enum — skills are a
//! large, mostly game-specific roster, so they are identified by EditorID
//! (resolved to an AUTHORED AVIF FormID by the loader) rather than a union
//! enum. The **governing attribute**, by contrast, *is* expressed canonically
//! ([`Attribute`]), so the skill→attribute map reads game-agnostically.
//!
//! Governing is `Option<Attribute>` because the relationship is per-family:
//!
//! * **TES classic** (Morrowind / Oblivion) — every skill is governed by one
//!   attribute; raising governed skills drives the level-up attribute bonus.
//! * **Fallout FO3 / FNV** — skills are governed by a SPECIAL for auto-calc
//!   base values (that map lives at the population boundary,
//!   `crates/plugin/.../actor_value_derive.rs`).
//! * **Skyrim** — 18 skills, **ungoverned** (`None`): attributes were removed,
//!   skills carry their own XP and drive leveling directly.
//! * **FO4 / FO76** — no skills at all (perks replace them); empty roster.
//!
//! ENGINE-SUPPLIED membership + governing map; AVIF FormIDs stay AUTHORED —
//! the CHARAL doctrine (`docs/engine/charal.md`).

use super::attribute::Attribute;

/// One skill: its canonical AVIF EditorID and the attribute that governs it.
///
/// `editor_id` resolves to an AUTHORED FormID via the loader's resolver (the
/// `resolve("Blade")` pattern). Pre-AVIF TES (Morrowind / Oblivion) carries
/// skills as hardcoded engine actor-value indices; that family maps these
/// EditorIDs to its legacy indices at the parser boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkillDef {
    /// Canonical AVIF EditorID (CS/CK internal name, no spaces — e.g.
    /// `"HandToHand"`, `"HeavyArmor"`).
    pub editor_id: &'static str,
    /// The governing attribute, or `None` for ungoverned skills (Skyrim).
    pub governing: Option<Attribute>,
}

impl SkillDef {
    /// A governed skill (TES classic / Fallout).
    const fn governed(editor_id: &'static str, governing: Attribute) -> Self {
        Self {
            editor_id,
            governing: Some(governing),
        }
    }

    /// An ungoverned skill (Skyrim — attributes were removed, so skills carry
    /// their own XP and no governing attribute).
    const fn ungoverned(editor_id: &'static str) -> Self {
        Self {
            editor_id,
            governing: None,
        }
    }
}

/// A resolved skill — its AUTHORED skill AVIF FormID paired with the AUTHORED
/// FormID of its governing attribute (if any). Produced by
/// [`SkillSet::resolve`]; keeps the canonical link as concrete ids the runtime
/// can index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedSkill {
    pub skill_av: u32,
    pub governing_av: Option<u32>,
}

/// The per-game skill roster — ENGINE-SUPPLIED membership + governing map over
/// a `&'static` slice of [`SkillDef`]. `Copy` and pointer-sized; held inline
/// by [`super::CharacterRuleset`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkillSet {
    skills: &'static [SkillDef],
}

impl SkillSet {
    /// Oblivion (TES IV) — the 21 skills across the three specializations,
    /// each governed by one attribute. Luck governs no skill (correct for
    /// Oblivion). Source: Elder Scrolls Wiki, *Skills (Oblivion)*.
    pub const OBLIVION: Self = Self {
        skills: &[
            // Combat — Strength
            SkillDef::governed("Blade", Attribute::Strength),
            SkillDef::governed("Blunt", Attribute::Strength),
            SkillDef::governed("HandToHand", Attribute::Strength),
            // Combat — Endurance
            SkillDef::governed("Armorer", Attribute::Endurance),
            SkillDef::governed("Block", Attribute::Endurance),
            SkillDef::governed("HeavyArmor", Attribute::Endurance),
            // Combat — Speed
            SkillDef::governed("Athletics", Attribute::Speed),
            // Magic — Willpower
            SkillDef::governed("Alteration", Attribute::Willpower),
            SkillDef::governed("Destruction", Attribute::Willpower),
            SkillDef::governed("Restoration", Attribute::Willpower),
            // Magic — Intelligence
            SkillDef::governed("Alchemy", Attribute::Intelligence),
            SkillDef::governed("Conjuration", Attribute::Intelligence),
            SkillDef::governed("Mysticism", Attribute::Intelligence),
            // Magic — Personality
            SkillDef::governed("Illusion", Attribute::Personality),
            // Stealth — Agility
            SkillDef::governed("Security", Attribute::Agility),
            SkillDef::governed("Sneak", Attribute::Agility),
            SkillDef::governed("Marksman", Attribute::Agility),
            // Stealth — Speed
            SkillDef::governed("Acrobatics", Attribute::Speed),
            SkillDef::governed("LightArmor", Attribute::Speed),
            // Stealth — Personality
            SkillDef::governed("Mercantile", Attribute::Personality),
            SkillDef::governed("Speechcraft", Attribute::Personality),
        ],
    };

    /// Skyrim (TES V) — the 18 skills, all **ungoverned** (attributes were
    /// removed; skills carry their own XP and drive character leveling). Six
    /// per specialization (Combat / Magic / Stealth). EditorIDs are the CK
    /// internal ActorValue names, which differ from the display names for two
    /// skills retained from earlier engines: Archery = `Marksman`, Speech =
    /// `Speechcraft`. Resolution against the parsed AVIF set is verified at
    /// load (resolve-or-skip), so any casing/name drift degrades gracefully.
    /// Source: Elder Scrolls Wiki / UESP *Skyrim:Skills*.
    pub const SKYRIM: Self = Self {
        skills: &[
            // Combat
            SkillDef::ungoverned("OneHanded"),
            SkillDef::ungoverned("TwoHanded"),
            SkillDef::ungoverned("Marksman"), // Archery
            SkillDef::ungoverned("Block"),
            SkillDef::ungoverned("Smithing"),
            SkillDef::ungoverned("HeavyArmor"),
            // Magic
            SkillDef::ungoverned("Alteration"),
            SkillDef::ungoverned("Conjuration"),
            SkillDef::ungoverned("Destruction"),
            SkillDef::ungoverned("Illusion"),
            SkillDef::ungoverned("Restoration"),
            SkillDef::ungoverned("Enchanting"),
            // Stealth
            SkillDef::ungoverned("LightArmor"),
            SkillDef::ungoverned("Sneak"),
            SkillDef::ungoverned("Lockpicking"),
            SkillDef::ungoverned("Pickpocket"),
            SkillDef::ungoverned("Speechcraft"), // Speech
            SkillDef::ungoverned("Alchemy"),
        ],
    };

    /// No skills (FO4 / FO76 — perks replace skills).
    pub const NONE: Self = Self { skills: &[] };

    /// Every skill in the roster.
    #[must_use]
    pub const fn skills(&self) -> &'static [SkillDef] {
        self.skills
    }

    /// Number of skills (21 Oblivion, 0 FO4/FO76).
    #[must_use]
    pub const fn len(&self) -> usize {
        self.skills.len()
    }

    /// Whether this game has no skills (FO4 / FO76).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// The definition of the skill with this EditorID, if present.
    #[must_use]
    pub fn get(&self, editor_id: &str) -> Option<&'static SkillDef> {
        self.skills.iter().find(|s| s.editor_id == editor_id)
    }

    /// The attribute governing `editor_id`, if the skill exists and is
    /// governed.
    #[must_use]
    pub fn governing(&self, editor_id: &str) -> Option<Attribute> {
        self.get(editor_id).and_then(|s| s.governing)
    }

    /// Resolve every skill to AUTHORED FormIDs: the skill's own AVIF id and
    /// its governing attribute's AVIF id. `resolve` maps EditorID → FormID
    /// (the parsed AVIF set). A skill whose own EditorID doesn't resolve is
    /// dropped; an unresolved governing attribute degrades to `None` rather
    /// than dropping the skill (the skill still exists, just without a
    /// resolved governor — the loader logs the gap).
    pub fn resolve<F: Fn(&str) -> Option<u32>>(&self, resolve: F) -> Vec<ResolvedSkill> {
        self.skills
            .iter()
            .filter_map(|s| {
                resolve(s.editor_id).map(|skill_av| ResolvedSkill {
                    skill_av,
                    governing_av: s.governing.and_then(|a| resolve(a.editor_id())),
                })
            })
            .collect()
    }
}

impl Default for SkillSet {
    /// Empty — a game declares its roster explicitly.
    fn default() -> Self {
        Self::NONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oblivion_has_twenty_one_skills_all_governed() {
        assert_eq!(SkillSet::OBLIVION.len(), 21);
        assert!(!SkillSet::OBLIVION.is_empty());
        for s in SkillSet::OBLIVION.skills() {
            assert!(s.governing.is_some(), "{} ungoverned", s.editor_id);
        }
    }

    #[test]
    fn governing_map_matches_the_wiki() {
        let ob = SkillSet::OBLIVION;
        assert_eq!(ob.governing("Blade"), Some(Attribute::Strength));
        assert_eq!(ob.governing("HandToHand"), Some(Attribute::Strength));
        assert_eq!(ob.governing("HeavyArmor"), Some(Attribute::Endurance));
        assert_eq!(ob.governing("Athletics"), Some(Attribute::Speed));
        assert_eq!(ob.governing("Acrobatics"), Some(Attribute::Speed));
        assert_eq!(ob.governing("Destruction"), Some(Attribute::Willpower));
        assert_eq!(ob.governing("Mysticism"), Some(Attribute::Intelligence));
        assert_eq!(ob.governing("Illusion"), Some(Attribute::Personality));
        assert_eq!(ob.governing("Speechcraft"), Some(Attribute::Personality));
        assert_eq!(ob.governing("Marksman"), Some(Attribute::Agility));
    }

    #[test]
    fn governing_attributes_are_a_subset_of_the_tes_roster() {
        use crate::character::AttributeSet;
        for s in SkillSet::OBLIVION.skills() {
            let g = s.governing.expect("governed");
            assert!(
                AttributeSet::TES_CLASSIC.contains(g),
                "{} governed by non-TES attr {:?}",
                s.editor_id,
                g
            );
        }
    }

    #[test]
    fn luck_governs_no_oblivion_skill() {
        assert!(SkillSet::OBLIVION
            .skills()
            .iter()
            .all(|s| s.governing != Some(Attribute::Luck)));
    }

    #[test]
    fn unknown_skill_resolves_to_none() {
        assert!(SkillSet::OBLIVION.get("Spelunking").is_none());
        assert_eq!(SkillSet::OBLIVION.governing("Spelunking"), None);
    }

    #[test]
    fn resolve_pairs_skill_and_governor_and_degrades_gracefully() {
        // Resolver knows Blade + Strength, and Sneak but NOT Agility.
        let resolve = |id: &str| -> Option<u32> {
            Some(match id {
                "Blade" => 0x1C,
                "Strength" => 0x00, // TES legacy index for Strength
                "Sneak" => 0x15,
                // Agility intentionally absent.
                _ => return None,
            })
        };
        let r = SkillSet::OBLIVION.resolve(resolve);
        // Only Blade + Sneak resolve their own id.
        assert_eq!(r.len(), 2);
        let blade = r[0];
        assert_eq!(blade.skill_av, 0x1C);
        assert_eq!(blade.governing_av, Some(0x00));
        // Sneak resolves, but its governor (Agility) doesn't → None, not dropped.
        let sneak = r[1];
        assert_eq!(sneak.skill_av, 0x15);
        assert_eq!(sneak.governing_av, None);
    }

    #[test]
    fn empty_roster_for_perk_games() {
        assert_eq!(SkillSet::NONE.len(), 0);
        assert!(SkillSet::NONE.is_empty());
        assert!(SkillSet::default().is_empty());
    }

    #[test]
    fn skyrim_has_eighteen_ungoverned_skills() {
        assert_eq!(SkillSet::SKYRIM.len(), 18);
        for s in SkillSet::SKYRIM.skills() {
            assert!(s.governing.is_none(), "{} should be ungoverned", s.editor_id);
        }
        // The two renamed-internal skills resolve by CK name.
        assert!(SkillSet::SKYRIM.get("Marksman").is_some()); // Archery
        assert!(SkillSet::SKYRIM.get("Speechcraft").is_some()); // Speech
        assert_eq!(SkillSet::SKYRIM.governing("OneHanded"), None);
        // No duplicate editor ids.
        let mut ids: Vec<_> = SkillSet::SKYRIM.skills().iter().map(|s| s.editor_id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 18);
    }
}
