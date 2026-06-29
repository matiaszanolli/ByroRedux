//! FNV / FO3 NPC actor-value derivation (#1663 population).
//!
//! Computes an NPC's SPECIAL attributes and base skill values from its
//! class, per the documented GECK auto-calculate model, and resolves each
//! to its `AVIF` FormID so the result can populate an `ActorValues`
//! component the `GetActorValue` condition reads.
//!
//! ## Model (cited)
//!
//! - **SPECIAL**: an auto-calc NPC adopts its **class's base attributes**
//!   as its SPECIAL (geckwiki *Stats Tab - NPC* / *Class*). Those are the
//!   7 bytes of the class's `ATTR` subrecord (fopdoc `CLAS`), Str → Luck.
//! - **Skill base**: `skill = fAVDSkill<name>Base + governing_SPECIAL ×
//!   fAVDSkillPrimaryBonusMult + ceil(Luck × fAVDSkillLuckBonusMult)`
//!   (geckwiki *Derived Skill Settings*). Defaults: base **2**, primary
//!   mult **2**, luck mult **0.5**. Worked example (geckwiki): END 5 +
//!   Luck 5 → Unarmed = 2 + 5×2 + ceil(5×0.5) = 15.
//! - **Governing SPECIAL per FNV skill**: fallout.fandom *New Vegas SPECIAL*
//!   / geckwiki *SPECIAL*. FO3's Small Guns (→Agility) and Big Guns
//!   (→Endurance) are included so the same table serves both games — each
//!   game populates only the skills whose `AVIF` exists (the others resolve
//!   to `None` and are skipped).
//!
//! ## Deferred (intentionally, not guessed)
//!
//! - **Tag-skill bonus + per-level growth.** The 3 class tag skills get a
//!   flat `fAVDTagSkillBonus` (+15) and an Intelligence-scaled per-level
//!   bonus; the exact per-level formula is **not published anywhere
//!   citable** (geckwiki / Fallout wikis describe it only qualitatively),
//!   so it is left out rather than fabricated. The base values below are
//!   correct; tag skills will read a few points low until the level model
//!   is pinned against the engine.
//! - **Non-auto-calc NPCs.** NPCs with "Auto-calculate stats" off store
//!   their own SPECIAL; we always use the class base attributes. Correct
//!   for the auto-calc majority; an approximation for hand-tuned actors.
//! - **Derived attributes** (Health, Action Points, Carry Weight, …).
//!
//! ## FormID space
//!
//! `index.classes` and `index.actor_values` are keyed in global load-order
//! space; the returned `AVIF` FormIDs are too (the same space a remapped
//! CTDA `param_1` / `GetActorValue` compares against). `NpcRecord.
//! class_form_id` (CNAM) is carried in the NPC's source-plugin space, so
//! the `index.classes` lookup is exact in single-plugin loads (identity
//! remap) and shares the NPC subsystem's known multi-plugin remap gap —
//! the same one [`super::super::super::ecs`-adjacent] `FactionRanks` has.

use super::actor::NpcRecord;
use super::index::EsmIndex;
use crate::esm::reader::GameKind;

/// The 7 SPECIAL attributes, in `ATTR`/`ClassRecord::base_attributes`
/// order, paired with their `AVIF` EditorID.
const SPECIAL: [&str; 7] = [
    "Strength",
    "Perception",
    "Endurance",
    "Charisma",
    "Intelligence",
    "Agility",
    "Luck",
];

/// `(AVIF EditorID, governing-SPECIAL index into [`SPECIAL`])` for every
/// FNV + FO3 skill. Indices: 0=Str, 1=Per, 2=End, 3=Cha, 4=Int, 5=Agi.
/// (Luck governs no skill directly — it contributes the `ceil(Luck/2)`
/// term to all of them.)
const SKILLS: [(&str, usize); 15] = [
    ("Barter", 3),        // Charisma
    ("EnergyWeapons", 1), // Perception
    ("Explosives", 1),    // Perception
    ("Guns", 5),          // Agility (FNV)
    ("Lockpick", 1),      // Perception
    ("Medicine", 4),      // Intelligence
    ("MeleeWeapons", 0),  // Strength
    ("Repair", 4),        // Intelligence
    ("Science", 4),       // Intelligence
    ("Sneak", 5),         // Agility
    ("Speech", 3),        // Charisma
    ("Survival", 2),      // Endurance (FNV)
    ("Unarmed", 2),       // Endurance
    ("SmallGuns", 5),     // Agility (FO3)
    ("BigGuns", 2),       // Endurance (FO3)
];

// Derived-skill game-setting defaults (geckwiki Derived Skill Settings).
const SKILL_BASE: f32 = 2.0; // fAVDSkill<name>Base
const SKILL_ATTR_MULT: f32 = 2.0; // fAVDSkillPrimaryBonusMult
const SKILL_LUCK_MULT: f32 = 0.5; // fAVDSkillLuckBonusMult

/// `skill = 2 + 2 × governing + ceil(Luck × 0.5)`.
fn base_skill(governing: u8, luck: u8) -> f32 {
    SKILL_BASE
        + SKILL_ATTR_MULT * f32::from(governing)
        + (SKILL_LUCK_MULT * f32::from(luck)).ceil()
}

/// Derive an FNV/FO3 NPC's `(AVIF FormID, value)` actor-value pairs from
/// its class's base SPECIAL. Returns the 7 SPECIAL plus every skill whose
/// `AVIF` resolves in `index`. Empty when the game isn't FNV/FO3, the NPC
/// has no class, or the class wasn't parsed.
pub fn derive_npc_actor_values(
    npc: &NpcRecord,
    index: &EsmIndex,
    game: GameKind,
) -> Vec<(u32, f32)> {
    if !matches!(game, GameKind::Fallout3NV) {
        return Vec::new();
    }
    let Some(class) = index.classes.get(&npc.class_form_id) else {
        return Vec::new();
    };
    let special = class.base_attributes;
    let luck = special[6];

    let mut out = Vec::with_capacity(SPECIAL.len() + SKILLS.len());
    for (i, editor_id) in SPECIAL.iter().enumerate() {
        if let Some(fid) = index.actor_value_form_id(editor_id) {
            out.push((fid, f32::from(special[i])));
        }
    }
    for (editor_id, gov) in SKILLS {
        if let Some(fid) = index.actor_value_form_id(editor_id) {
            out.push((fid, base_skill(special[gov], luck)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::{AvifRecord, ClassRecord};

    fn avif(form_id: u32, editor_id: &str) -> AvifRecord {
        AvifRecord {
            form_id,
            editor_id: editor_id.to_string(),
            ..Default::default()
        }
    }

    /// Build an index whose AVIF records cover the 7 SPECIAL + 13 FNV
    /// skills, with deterministic FormIDs (0x100 + slot).
    fn fnv_index_with_class(class_form_id: u32, base: [u8; 7]) -> EsmIndex {
        let mut index = EsmIndex::default();
        let mut fid = 0x100u32;
        for name in SPECIAL.iter().chain(
            [
                "Barter",
                "EnergyWeapons",
                "Explosives",
                "Guns",
                "Lockpick",
                "Medicine",
                "MeleeWeapons",
                "Repair",
                "Science",
                "Sneak",
                "Speech",
                "Survival",
                "Unarmed",
            ]
            .iter(),
        ) {
            index.actor_values.insert(fid, avif(fid, name));
            fid += 1;
        }
        index.classes.insert(
            class_form_id,
            ClassRecord {
                form_id: class_form_id,
                base_attributes: base,
                ..Default::default()
            },
        );
        index
    }

    fn npc_with_class(class_form_id: u32) -> NpcRecord {
        NpcRecord {
            class_form_id,
            ..Default::default()
        }
    }

    #[test]
    fn base_skill_matches_documented_example() {
        // geckwiki worked example: END 5, Luck 5 → 2 + 5*2 + ceil(2.5) = 15.
        assert_eq!(base_skill(5, 5), 15.0);
        // Luck rounds UP: Luck 7 → ceil(3.5) = 4 bonus.
        assert_eq!(base_skill(0, 7), 2.0 + 4.0);
        // Zero everything → the flat base of 2.
        assert_eq!(base_skill(0, 0), 2.0);
    }

    #[test]
    fn derives_special_and_skills_from_class() {
        // Str=5 Per=6 End=5 Cha=4 Int=7 Agi=6 Luck=5.
        let base = [5, 6, 5, 4, 7, 6, 5];
        let index = fnv_index_with_class(0x2000, base);
        let npc = npc_with_class(0x2000);
        let pairs = derive_npc_actor_values(&npc, &index, GameKind::Fallout3NV);

        // Helper: value for a named AV via its resolved FormID.
        let val = |name: &str| -> f32 {
            let fid = index.actor_value_form_id(name).unwrap();
            pairs.iter().find(|(f, _)| *f == fid).unwrap().1
        };

        // SPECIAL copied straight through.
        assert_eq!(val("Strength"), 5.0);
        assert_eq!(val("Intelligence"), 7.0);
        assert_eq!(val("Luck"), 5.0);

        // Skills via 2 + 2*gov + ceil(Luck/2); Luck 5 → +3.
        assert_eq!(val("Unarmed"), 2.0 + 2.0 * 5.0 + 3.0, "END 5"); // 15
        assert_eq!(val("Guns"), 2.0 + 2.0 * 6.0 + 3.0, "AGI 6"); // 17
        assert_eq!(val("Science"), 2.0 + 2.0 * 7.0 + 3.0, "INT 7"); // 19
        assert_eq!(val("Barter"), 2.0 + 2.0 * 4.0 + 3.0, "CHA 4"); // 13

        // 7 SPECIAL + 13 FNV skills resolved (SmallGuns/BigGuns absent here).
        assert_eq!(pairs.len(), 20);
    }

    #[test]
    fn empty_without_class_or_wrong_game() {
        let index = fnv_index_with_class(0x2000, [5; 7]);
        // NPC referencing an unparsed class → empty.
        assert!(derive_npc_actor_values(&npc_with_class(0x9999), &index, GameKind::Fallout3NV).is_empty());
        // Right NPC, wrong game → empty (FNV/FO3 model only).
        assert!(derive_npc_actor_values(&npc_with_class(0x2000), &index, GameKind::Skyrim).is_empty());
    }
}
