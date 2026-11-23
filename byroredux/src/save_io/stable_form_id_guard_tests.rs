//! LMC-SAV-002..004 (#6676) — every saved FormID is written as a stable identity (`byroredux_core::stable_form_id`),
//! so a save follows its plugins across a changed load order.
//!
//! Rust has no field reflection, so the guard is an audited classification of every registered column, the
//! `MUTABLE_DELTA_COLUMNS` pattern: a new column fails here until it is placed in one of the three lists below, which
//! is the moment to mark its FormIDs. The shift tests check representative columns of each shape end to end.

use super::*;
use byroredux_core::stable_form_id::test_support::{removed, shift};
use std::collections::HashMap;

/// Columns whose FormIDs are written as stable identities (adapters, wire types or codecs).
const STABLE: &[&str] = &[
    "Inventory", "EquippedWeapon", "EquippedAmmo", "Perks", "FactionReputation", "CharacterIdentity", "SpellList",
    "ReadiedSpell", "ScriptPackage", "SpeakingLine", "ActorCinematicState", "HorseTetherState", "ScriptVariables",
    "TimedRestorations", "ActiveMagicEffects", "ActorValues", "ActorVitals", "SkillProgress", "PersistentReferenceStates",
    "QuestStageState", "QuestObjectiveState", "StoryManagerNodeState", "StoryEventAliasFill", "QuestScriptVariables",
    "QuestScriptObjects", "LocationAliasBindings", "LocationClearState", "AliasOverrides", "CreatedReferences",
    "QuestAliasInjectionState", "FragmentExecutionQueue", "ObScriptQuestLocals", "ObScriptLedger", "CrimeKnowledge",
    "KillRecords", "PipBoyNotes", "ForcedWeather", "CellVisits", "OwnerOverrides", "FactionCrimeGold",
    "CrimeFactionOverrides", "SpeechcraftOutcomes", "DialogueSpokenInfoForms", "ChallengeState", "ScriptedRelationships",
    "LinkedRefOverrides", "AliasCollections", "LocationKeywordData", "ScriptedKeywords", "ActorBaseProtectionOverrides",
    "Globals", "KnownWords", "MapMarkers", "FollowerState", "ReferenceFlags", "DestructionStates",
    "AnimArchetypeOverrides", "ScannedReferences", "FormListAdditions", "LocRefTypeOverrides", "ReferenceActivationBlocks",
    "EventRegistrations", "DistanceWatches", "HitRegistrations", "AnimationEventRegistrations", "MenuEventRegistrations",
    "ScriptUpdates", "AliasScriptOwners", "CinematicPresentationState", "FormIdComponent", "FactionRelations",
    "PapyrusProviderContinuationQueue",
];

/// Ledgers the live load restores *before* the cell reload (`PRE_RELOAD_RESOURCES`), while the world still holds the
/// outgoing session's load order. They stay session-global numbers until the load reloads the session's own profile
/// (Phase 7 slice 3), when that order is the one to read them through.
const PRE_RELOAD_RAW: &[&str] = &[
    "ReferenceEnableState", "ReferenceLockState", "ReferenceScriptState", "ScriptedFactionChanges",
    "ReferenceMotionTypeState", "ActorRelocations",
];

/// Columns that hold no FormID (entity ids, counts, flags, strings, poses, clocks).
const NO_FORM_IDS: &[&str] = &[
    "Transform", "Name", "Parent", "Children", "EquipmentSlots", "LightSource", "LightFlicker", "AnimationPlayer",
    "AnimationStack", "ScriptTimer", "TwoStateActivator", "LevelUpPoints", "CharacterLevel", "SkillUseLevel",
    "DismemberRecord", "Dead", "Sneaking", "WanderState", "TravelState", "Traveled", "FollowState", "EscortState",
    "Escorted", "GuardState", "PatrolState", "Seated", "ActorControlState", "Bleedout", "CharacterController", "Material",
    "RumbleOnActivate", "WeaponDrawn", "IgnoreCrime", "ItemInstancePool", "CurrentCellContext", "CurrentExteriorContext",
    "SessionProfile", "PlayerPose", "GameTimeRes", "HardcoreMode", "MiscStats", "Achievements", "InputLayers",
    "PlayerControlState",
];

#[test]
fn every_saved_column_is_classified_for_stable_form_ids() {
    let registry = build_save_registry();
    let registered: Vec<&str> = registry.component_names().chain(registry.resource_names()).collect();
    let lists = [("STABLE", STABLE), ("PRE_RELOAD_RAW", PRE_RELOAD_RAW), ("NO_FORM_IDS", NO_FORM_IDS)];
    let unclassified: Vec<&str> =
        registered.iter().copied().filter(|name| !lists.iter().any(|(_, list)| list.contains(name))).collect();
    assert!(
        unclassified.is_empty(),
        "saved columns with no stable-FormID classification: {unclassified:?}. Mark their FormIDs with \
         `byroredux_core::stable_form_id` and list them in STABLE, or list them in NO_FORM_IDS"
    );
    for (label, list) in lists {
        for name in list {
            assert!(registered.contains(name), "{label} lists {name}, which is not a registered column");
            let listed = lists.iter().filter(|(_, other)| other.contains(name)).count();
            assert_eq!(listed, 1, "{name} is in more than one list");
        }
    }
}

/// A quest-keyed map, a pair-keyed map that is normalised `(min, max)`, FormID sets and a FormID-keyed map follow a
/// shifted plugin, and lose an absent one's entries.
#[test]
fn slice_two_shapes_follow_their_plugin_across_a_changed_load_order() {
    use byroredux_scripting::quest_stages::{QuestFormId, QuestStageState};

    let mut stages = QuestStageState::default();
    stages.start_quest(QuestFormId(0x0200_0100), Some(10));
    stages.start_quest(QuestFormId(0x0100_0101), Some(20));
    stages.start_quest(QuestFormId(0x0000_0102), None);
    let (loaded, report) = shift(&stages, &[]);
    assert!(loaded.is_running(QuestFormId(0x0100_0100)), "ModB's quest follows it to slot 1");
    assert!(!loaded.is_running(QuestFormId(0x0100_0101)), "ModA's quest is gone, not read as ModB's");
    assert!(loaded.is_running(QuestFormId(0x0000_0102)));
    assert_eq!(report.absent.get(&removed()), Some(&1));

    // `(min, max)` before, and ModB's form moving below the game's would break the order: it is normalised again.
    let relationships =
        byroredux_scripting::ScriptedRelationships(HashMap::from([((0x0000_0001, 0x0200_0002), 3), ((0x0000_0003, 0x0100_0004), 1)]));
    let (loaded, _) = shift(&relationships, &[]);
    assert_eq!(loaded.0, HashMap::from([((0x0000_0001, 0x0100_0002), 3)]));

    let words = byroredux_scripting::words_of_power::KnownWords([0x0000_0010, 0x0100_0011, 0x0200_0012].into());
    assert_eq!(shift(&words, &[]).0 .0, [0x0000_0010, 0x0100_0012].into());

    let globals = byroredux_scripting::Globals(HashMap::from([(0x0200_0020, 2.5), (0x0100_0021, 1.0)]));
    assert_eq!(shift(&globals, &[]).0 .0, HashMap::from([(0x0100_0020, 2.5)]));
}

/// A reference variable kept as two `f32` halves follows its plugin; one naming an absent plugin's form is cleared;
/// a plain value is unchanged.
#[test]
fn script_variable_references_follow_their_plugin() {
    use byroredux_scripting::ScriptVariables;

    let mut variables = ScriptVariables::default();
    variables.set_reference_by_name("target", 0x0200_0800);
    variables.set_reference_by_name("gone", 0x0100_0801);
    variables.set_by_name("count", 3.0);
    // The halves are numbers below `0x0100_0000` (16 bits each), so the stray-number check cannot see them; the
    // wire type is what carries the reference.
    let (loaded, report) = shift(&variables, &[]);
    assert_eq!(loaded.get_by_name("target#lo"), Some(0x0800 as f32));
    assert_eq!(loaded.get_by_name("target#hi"), Some(0x0100 as f32), "the high half now names slot 1");
    assert_eq!(loaded.get_by_name("gone#lo"), None);
    assert_eq!(loaded.get_by_name("count"), Some(3.0));
    assert_eq!(report.absent.get(&removed()), Some(&1));
}
