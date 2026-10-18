# SCR-D5-2026-10-17-03: A Location Alias Reference fill ignores the bound location alias (and keyword), binding any candidate of the ref type anywhere

Issue: #6004
Labels: medium, scripting, bug, quests

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D5-2026-10-17-03: A Location Alias Reference fill ignores the bound location alias (and keyword), binding any candidate of the ref type anywhere

- **Severity**: MEDIUM
- **Dimension**: Scene/Package/Cinematic
- **Location**: `crates/scripting/src/scene/quest_alias.rs:330-345` (`candidate_matches_fill`), `:424-427` (`fill_positions` bucket by ref type only); same shape in `byroredux/src/asset_provider/script.rs:~1387` (`matches_fill`)
- **Status**: NEW (the m47-3 design table lists the row as "partial", but its stated blocker, an LCTN model, shipped in M47.2)
- **Untrusted-Input**: No
- **Description**: For a *reference* alias the CK fill is "pick a Location Alias from this quest ... and a Location Ref Type. When the quest starts, the Story Manager will attempt to find a matching loc ref type **from that location**" (ck-uesp-wiki `Quest Alias Tab.wiki` line 61). The recognizer keeps only `ref_type` and never consults `alias_id` or `keyword`: any loaded candidate whose own XLRT (or script-set `SetLocRefType`) tags that type matches, from any location. Now that location aliases bind (`LocationAliasBindings`) and `LocationTable::refs_of_type(location, type)` exists, the constraint is decidable; the fill still approximates, so with several locations loaded (exterior streaming: many Boss/Marker-tagged refs) it silently binds the nearest-ordered wrong actor, which then receives the alias's injected factions/packages/inventory. Per the skill's own rule a partial lowering is worse than declining.
- **Evidence**: `AliasFillType::LocationAliasReference { alias_id, keyword, ref_type }` is destructured with `..` in both matchers; only `fill_location_alias` (location aliases) reads `alias_id`/`keyword` (:1338-1349).
- **Impact**: wrong-actor alias bindings on any radiant/location quest using this fill; state corruption via alias injection. Reach is bounded by how many tagged refs are resident at once.
- **Related**: m47-3-quest-alias-design.md row 208 ("partial"), #2661 precedent (collection alias half-working instead of declining).
- **Suggested Fix**: when `alias_id` names a location alias, require the candidate's reference to be in `refs_of_type(bound_location, ref_type)` (or its cell location within it, honouring `keyword`), and decline (leave unbound) while that location alias is unbound.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
