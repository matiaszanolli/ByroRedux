# SCR-D3-2026-10-17-05: Scene / phase conditions trust the fail-open Unknown-function pass

Issue: #5993
Labels: medium, scripting, bug, quests

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D3-2026-10-17-05: Scene / phase conditions trust the fail-open Unknown-function pass

- **Severity**: MEDIUM
- **Dimension**: Event Runtime (consumer lives in Dim 5)
- **Location**: `crates/scripting/src/scene/playback.rs:385,402` (`evaluate(&scene.conditions ..)`, `evaluate(&phase.start_conditions ..)`);
  same pattern at `quest_stages.rs:1011,1373`, `trigger.rs:541`, `byroredux/src/inventory.rs:1432`
- **Status**: NEW (the fail-closed `is_modeled` gate exists for the five consumers listed in Verified-clean; these were not converted)
- **Untrusted-Input**: No
- **Description**: A scene starts (Begin fragments, actor movement/dialogue) and each phase starts on the bare `evaluate` result; an uncatalogued term
  evaluates 0.0 and satisfies any `== 0`/`<=` comparison. The skill's rule is "a consumer that *acts* on a pass must decline on any `Unknown` term
  first". Skyrim.esm SCEN carries 301 unmodeled CTDA terms out of 2 791 (top: fn 289 x130, fn 18 x78, fn 249 x19, fn 214, 32, 45 ...); QUST 2 184 of
  11 427.
- **Evidence**: census above (python walk over `Skyrim.esm`, SCEN/QUST CTDA function indices minus `CATALOG`); call sites above carry no `is_modeled`.
- **Impact**: scenes whose authored gate cannot be read can start (or phases can advance) on the vacuous 0.0; quest log entries with
  Complete/Fail flags and objective-target highlighting have the same exposure. Bounded to terms outside the catalog.
- **Related**: #5380 (same class, Story Manager), bd85de5ea (dialogue). Scene ownership: Dim 5.
- **Suggested Fix**: route these through one `evaluate_modeled` helper that returns `false` when any term fails `is_modeled` (the same wrapper the SM gate uses).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
