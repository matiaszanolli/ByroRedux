//! M47.1 Phases 2 + 3 — `ConditionFunction` enum + OR-precedence
//! evaluator.
//!
//! Plugin-side ([`byroredux_plugin::esm::records::condition`]) parses
//! CTDA sub-records into [`Condition`] values; scripting-side (here)
//! interprets the function indices against ECS state and combines
//! the per-condition booleans with the OR-precedence rule.
//!
//! ## What "OR-precedence" means
//!
//! Default operator between conditions is AND. Setting `or_next` on
//! a condition combines it with the next via OR. **Consecutive ORs
//! form a block that binds tighter than the surrounding AND chain.**
//!
//! `A AND B OR C AND D` evaluates as `A AND (B OR C) AND D`, NOT
//! `(A AND B) OR (C AND D)` (the standard boolean reading).
//!
//! This is the opposite of standard boolean precedence. Bethesda
//! designers compose complex expressions by exploiting the distributive
//! law (`(A AND B) OR (C AND D) ⇔ (A OR C) AND (A OR D) AND (B OR C) AND (B OR D)`).
//!
//! ## Function catalog status
//!
//! Bethesda ships ~300 condition functions across the four-game
//! lineage. This catalog ships **52 functions** at their canonical CTDA
//! function indices, verified against TES5Edit `wbDefinitions*.pas`
//! (the same value `parse_ctda` reads from CTDA bytes 8–11; FO3 == FNV
//! for every shared function):
//!
//! | Index | Function               | Reads                       |
//! |-------|------------------------|-----------------------------|
//! | 1     | GetDistance            | `GlobalTransform`           |
//! | 14    | GetActorValue          | `ActorValues[param_1]`      |
//! | 46    | GetDead                | `Dead` marker               |
//! | 56    | GetQuestRunning        | `QuestStageState[param_1]`  |
//! | 58    | GetStage               | `QuestStageState[param_1]`  |
//! | 79    | GetQuestVariable       | `ObScriptQuestLocals[param_1][param_2]` (FO3 / FNV / Oblivion) |
//! | 59    | GetStageDone           | `QuestStageState[param_1]`  |
//! | 67    | GetInCell              | `CellRoot` + `CellFormId`   |
//! | 68    | GetIsClass             | `Background.class_form_id`  |
//! | 69    | GetIsRace              | `Background.race_form_id`   |
//! | 71    | GetInFaction           | `FactionRanks` (Skyrim)     |
//! | 72    | GetIsID                | `SceneAliasCandidate` base  |
//! | 73    | GetFactionRank         | `FactionRanks`              |
//! | 74    | GetGlobalValue         | `Globals[param_1]`          |
//! | 77    | GetRandomPercent       | `StoryManagerRng`           |
//! | 80    | GetLevel               | `CharacterLevel`            |
//! | 182   | GetEquipped            | `Inventory` + `EquipmentSlots` |
//! | 449   | HasPerk (448 on Skyrim)| `Perks`                      |
//! | 533   | GetXPForNextLevel      | `CharacterRuleset` leveling |
//! | 550   | IsSceneActionComplete  | `ScenePlayer` history       |
//! | 555   | HasLoaded3D             | `GlobalTransform`           |
//! | 573   | GetReputation          | `FactionReputation` (FNV)   |
//! | 575   | GetReputationThreshold | `FactionReputation` + bands |
//! | 586   | IsHardcore             | `HardcoreMode` (FNV)        |
//! | 629   | GetVMQuestVariable     | `QuestScriptVariables`      |
//! | 630   | GetVMScriptVariable    | `ScriptVariables`           |
//! | 359   | GetInCurrentLoc        | `LocationTable` + cell      |
//! | 360   | GetInCurrentLocAlias   | `LocationAliasBindings`     |
//! | 562   | LocationHasKeyword     | `LocationTable` keywords    |
//! | 565   | GetIsEditorLocation    | `LocationTable` editor refs |
//! | 566   | GetIsAliasRef          | `SceneActorBindings`        |
//! | 567   | GetIsEditorLocAlias    | `LocationAliasBindings`     |
//! | 605   | LocAliasIsLocation     | `LocationAliasBindings`     |
//! | 563   | LocationHasRefType     | `LocationTable` ref types   |
//! | 181   | HasSameEditorLocAsRefAlias | `LocationTable` editor refs |
//! | 576   | GetEventData           | `EventDataSlots`            |
//! | 250   | GetLocationCleared     | `LocationClearState`        |
//! | 598   | GetLocationAliasCleared | `LocationClearState`       |
//! | 606   | GetKeywordDataForLocation | `LocationKeywordData`    |
//! | 608   | GetKeywordDataForAlias | `LocationKeywordData`       |
//! | 651   | GetKeywordDataForCurrentLocation | `LocationKeywordData` |
//! | 354   | IsEssential            | `ActorProtection` + aliases |
//! | 476   | IsProtected            | `ActorProtection` + aliases |
//! | 580   | IsBleedingOut          | `Bleedout`                  |
//! | 453   | GetPlayerTeammate      | `FollowerState`             |
//! | 403   | GetRelationshipRank    | `relationships`             |
//! | 560   | HasKeyword             | `keywords`                  |
//! | 237   | GetIsGhost             | `ActorControlState`         |
//! | 254   | GetIsPlayableRace      | `PlayableRaces` (RACE flag) |
//! | 365   | GetPlayerInSEWorld     | ObScript ledger (Oblivion)  |
//! | 70    | GetIsSex               | `ActorBaseTraits` (ACBS)    |
//! | 310   | GetInWorldspace        | `cell_kind` (ExteriorCell)  |
//!
//! Unknown function indices evaluate to `0.0` (the Bethesda "unknown
//! function → safe-default" contract) and are logged at debug for
//! future-catalog tracking.
//!
//! ## Module layout (#5804)
//!
//! One file per concern under `condition/`, all re-exported from this hub
//! so every `crate::condition::*` (and `byroredux_scripting::condition::*`)
//! path is unchanged: `catalog.rs` (the `ConditionFunction` enum and the
//! `ROWS` table the six former classification `match`es derive from),
//! `context.rs` (`ConditionContext` / `EventDataSlots`), `eval.rs` (the
//! evaluator family — `evaluate_function`, `evaluate_contextual`,
//! `evaluate`), and `requirement.rs` (`subject_requirement`). The
//! source-shape pins in `condition/tests.rs` read the individual files;
//! the doc-table pin below still scans this hub's own module doc.

mod catalog;
mod context;
mod eval;
mod requirement;

pub use catalog::ConditionFunction;
pub(crate) use catalog::authored_actor_value_is_enum_code;
pub use context::{ConditionContext, EventDataSlots};
pub use eval::{
    actor_value_view, entity_global_form_id, evaluate, evaluate_condition, evaluate_function,
    evaluate_modeled,
    evaluate_script_getter, is_modeled, resolve_entity_by_global_form_id,
    uses_obscript_quest_variables, ActorValueView,
};
pub use requirement::{subject_requirement, IdentityTest, SubjectRequirement};

#[cfg(test)]
mod tests;

/// #3867 — the CTDA catalog size is published in two engine docs that
/// `_audit-common.md` names as authority ("prefer these docs over
/// re-deriving facts from source"), and both had fallen behind: the
/// feature matrix said 13 (itself the #1818 correction of 7) and the
/// AI-package fail-open paragraph said ~15, against a live catalog of 19.
///
/// An auditor sizing the M47.1 gap reads the doc, not this array, so the
/// drift is load-bearing — and it has now recurred twice. `CATALOG` is a
/// `const` array, so the documented figure can simply be asserted against
/// its length rather than re-checked by hand on the next six additions.
#[cfg(test)]
mod catalog_count_doc_pin_tests {
    use super::ConditionFunction;

    const FEATURE_MATRIX: &str = include_str!("../../../docs/feature-matrix.md");
    const NPC_SPAWN: &str = include_str!("../../../docs/engine/npc-spawn-ai-packages.md");
    /// This file's own source — the module doc's function table lives
    /// here, so the pin scans the file itself. The scan counts the
    /// table's rows (each starting `//! | <index> |`), a property of the
    /// doc text, not a call-site needle, so — unlike the #5650 stub
    /// pins — there is no production literal the test's own body could
    /// satisfy.
    const CONDITION_SRC: &str = include_str!("condition.rs");

    /// The first run of ASCII digits at or after `from` in `haystack`.
    fn digits_after(haystack: &str, anchor: &str, file: &str) -> usize {
        let rest = haystack
            .split_once(anchor)
            .unwrap_or_else(|| panic!("{file} must still carry the anchor {anchor:?} — the doc \
                                       was reworded, so re-point this pin rather than deleting it"))
            .1;
        let start = rest
            .find(|c: char| c.is_ascii_digit())
            .unwrap_or_else(|| panic!("{file}: no number follows {anchor:?}"));
        let rest = &rest[start..];
        let end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        rest[..end].parse().expect("a run of ASCII digits parses")
    }

    /// #5648 — the module doc's own table drifted to 19 rows while the
    /// catalog grew to 24, the third recurrence of the size drift this
    /// module already pins in the two engine docs. The table is the
    /// auditor-facing catalog (the enum's variant docs carry the detail),
    /// so its row count is pinned to `CATALOG` directly: adding a variant
    /// without a table row fails here rather than in a future audit.
    #[test]
    fn the_module_doc_table_lists_the_whole_catalog() {
        let module_doc = CONDITION_SRC
            .split_once("## Function catalog status")
            .and_then(|(_, rest)| rest.split_once("Unknown function indices"))
            .expect("the module doc's catalog section must keep its \
                     `## Function catalog status` / `Unknown function indices` framing")
            .0;
        let rows = module_doc
            .lines()
            .filter(|line| {
                // Strip the `//!` doc-comment prefix before matching.
                let t = line.trim().trim_start_matches("//!").trim();
                t.starts_with("| ") && !t.starts_with("| Index") && !t.starts_with("|---")
            })
            .count();
        assert_eq!(
            rows,
            ConditionFunction::CATALOG.len(),
            "the module doc's function table has {rows} rows but `CATALOG` lists {} — every \
             new catalog entry needs its table row (and its documented index re-verified)",
            ConditionFunction::CATALOG.len(),
        );
        assert_eq!(
            digits_after(CONDITION_SRC, "This catalog ships **", "condition.rs module doc"),
            ConditionFunction::CATALOG.len(),
            "the module doc's prose count no longer matches `ConditionFunction::CATALOG`",
        );
    }

    #[test]
    fn both_engine_docs_publish_the_live_catalog_size() {
        let live = ConditionFunction::CATALOG.len();
        assert!(
            live >= 19,
            "the catalog only ever grows; {live} entries means the extraction broke, \
             not the docs",
        );

        assert_eq!(
            digits_after(
                FEATURE_MATRIX,
                "CTDA condition evaluation with OR-precedence (M47.1) |",
                "docs/feature-matrix.md",
            ),
            live,
            "docs/feature-matrix.md's M47.1 row publishes a catalog size that no longer \
             matches `ConditionFunction::CATALOG` (#3867, and #1818 before it)",
        );

        assert_eq!(
            digits_after(
                NPC_SPAWN,
                "the M47.1 catalog covers ",
                "docs/engine/npc-spawn-ai-packages.md",
            ),
            live,
            "docs/engine/npc-spawn-ai-packages.md's fail-open paragraph publishes a catalog \
             size that no longer matches `ConditionFunction::CATALOG` (#3867)",
        );
    }
}
