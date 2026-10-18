# SCR-D5-2026-10-17-01: Reference-alias fills read last refresh's location-alias bindings, so a reference alias gated on a location alias of the same quest lags (or never fills) on the quest-start refresh

Issue: #6002
Labels: medium, scripting, bug, quests

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D5-2026-10-17-01: Reference-alias fills read last refresh's location-alias bindings, so a reference alias gated on a location alias of the same quest lags (or never fills) on the quest-start refresh

- **Severity**: MEDIUM
- **Dimension**: Scene/Package/Cinematic
- **Location**: `crates/scripting/src/scene/quest_alias.rs:1138-1144` (per-candidate `eligible` ctx), `:1175-1181` (`subject_requirement` ctx), `:1238-1241` (location bindings committed only at the end), vs `:1322-1329` (the location fill does attach them); `crates/scripting/src/condition/context.rs:166-178`; `condition/eval.rs:198-225, 301-305`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: M47.2 location aliases fill inside `refresh_scene_actor_bindings` "in list order with the reference aliases, so either kind can depend on one above it" (comment at ~:925). The location-to-location and location-to-reference-alias directions are handled (`fill_location_alias` attaches `with_pending_alias_bindings(resolved)` and `with_pending_location_bindings(locations)`). The inverse direction is not: when a *reference* alias's match conditions use `GetInCurrentLocAlias`, `GetIsEditorLocAlias`, `LocAliasIsLocation`, `GetLocationAliasCleared` or `GetKeywordDataForAlias` (all read `ctx.alias_location`), the contexts built for the candidate scan and for `subject_requirement` carry only `.with_pending_alias_bindings(&resolved)`. `alias_location` therefore falls through to the committed `LocationAliasBindings`, which `refresh` replaces only after the whole loop (`.replace(locations)` :1238-1241). This is exactly the #2671 "fill lagged a refresh behind its own dependency" class, for locations. The canonical radiant shape (location alias "Find Matching Location" + reference aliases "in that location" conditioned on `GetInCurrentLocAlias`) evaluates the reference aliases against an empty table on the refresh that starts the quest. Nothing re-dirties `SceneActorBindings` afterwards (bindings.dirty is cleared at :1264), so the alias stays unbound until an unrelated cell load / stage change marks it dirty. A sibling lag exists for an `ExternalAlias` location alias whose source quest has a higher FormID than the consumer and starts in the same pass (`locations.get` reads the map before the source's alias is filled; quests are processed in FormID order, and only reference-alias externals get the fixed-point loop at :1196-1214).
- **Evidence**: no test pins it (`rg "InCurrentLocAlias|360" scene/quest_alias_tests.rs` -> none; `location_aliases_fill_by_each_authored_mechanism_and_hold_until_stop` exercises location-only chains).
- **Impact**: Story-Manager-started quests start with their location-scoped reference aliases (actors that receive injected packages/factions/inventory) unbound for an unbounded time; non-optional aliases are not enforced at start (only FromEvent slots are, `event_aliases_fill`), so the quest runs half-populated.
- **Related**: #2671 (same class, reference aliases), M47.2 6f6a9eb3d.
- **Suggested Fix**: attach `with_pending_location_bindings(&locations)` to the reference-alias scan and `subject_requirement` contexts (the map is complete for aliases above in list order at that point), and run the external-location fill in the same fixed-point loop as the reference externals.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
