# #3854: TD1-2026-09-05-05: `fragment.rs` is 2538 production LOC of 2540 total, with a 519-LOC `apply_effect` and 18 near-identical `populate_*` entry points

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD1-2026-09-05-05) via `/audit-publish`, 2026-09-05. Labels: `low,scripting,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3854 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD1-2026-09-05-05), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/scripting/src/fragment.rs` (2538 production / 2540 total LOC); `apply_effect` at `:797`–`:1315`
- **Status**: NEW
- **Age**: created `9b375200`, 2026-06-23 — 244 total LOC at birth, **2540 today across 49 commits**
- **Description**: Uniquely in this bucket the file is ~100 % production — its tests already live in
  the sibling `crates/scripting/src/fragment/tests.rs` (3015 lines, 0 production). So the split
  target directory exists and is populated with exactly one file; the production side simply never
  followed. Three responsibilities are interleaved.
- **Evidence**:
  1. **Resources / state** — `ReferenceEnableState`, `QuestStageFragments`, `SceneFragments`,
     `SceneFragmentEffects`, `PendingFragmentExecution`, `FragmentResumeCondition`,
     `FragmentExecutionQueue`, `PendingFragmentActivations`, `DeferredFragmentEffects`,
     `DeferredProviderFragmentStep`, `DeferredCinematicPresentationEffect` (≈470 LOC).
  2. **Effect interpreter** — `resolve_quest`, `resolve_quest_logged`, `resolve_property_form_id`,
     `resolve_object`, `resolve_actor`, `actors_3d_loaded`, `update_actor_cinematic_state`,
     `apply_fragment_guard_free`, `poll_fragment_generated_advances`, `apply_effect`,
     `apply_quest_scoped_effect`, `apply_effects` (≈1150 LOC).
  3. **Population from `.pex` / `.psc`** — eighteen `populate_*` functions in six four-variant
     families (`populate_quest_fragments_from_pex[_detailed][_with_providers][_internal]`,
     the `populate_owned_*` twins, and the `..._from_script` and `..._scene_fragments_*` mirrors),
     plus `FragmentPexTranslation`, `OwnedFragmentProviders`, `FragmentProviderScope`,
     `quest_property_names`, `function_body` (≈580 LOC).
  4. **Dispatch systems** — `fragment_activation_flush_system`, `fragment_continuation_system`,
     `scene_fragment_dispatch_system`, `quest_fragment_dispatch_system`, `register`, `MAX_CASCADE`.

  `apply_effect` (519 LOC) is a 23-arm `match effect` where individual arms run to ~70 lines
  (`EquipItem` ≈70, `SetVehicle` ≈38, `TetherToHorse` ≈45, `Disable` ≈33, `StartScene|StopScene` ≈63).
  Under the 50-arm rule it is *not* a lookup-table candidate — the arms are behaviour, not data —
  but they group cleanly by effect family: globals · inventory (`AddItem`/`EquipItem`) ·
  placement & enable (`MoveTo`/`Disable`/`Activate`/`SetOpen`) · scene (`StartScene`/`StopScene`) ·
  player control (`SetPlayerRestrained`/`SetPlayerControls`/`SetPlayerAiDriven`/`SetHudCartMode`/
  `SetSittingRotation`/`RegisterPlayerAnimationEvent`) · vehicle-cinematic (`SetVehicle`/
  `TetherToHorse`/`SetMotionType`/`ExitCart`/`PlayIdle`) · AI (`EvaluatePackage`) · deferred
  (`Wait`/`WaitForActors3DLoaded`/`Conditional`). `apply_quest_scoped_effect` (169 LOC) is the
  second >150-LOC function.
- **Impact**: the eighteen `populate_*` variants are the most edit-prone surface here (every new
  provider/ownership flavour adds four more), and they force a recompile of the interpreter they
  do not touch.
- **Related**: `/audit-scripting` owns correctness here; this finding is size only.
- **Suggested Fix**: `fragment/{mod,state,populate,effects,systems}.rs` beside the existing
  `fragment/tests.rs`. Within `effects.rs`, give `apply_effect` one private helper per family
  above so each arm becomes a one-line delegate — the same treatment #3739/#3738 applied
  in `boot.rs`/`resize.rs`.
- **Effort**: medium

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
