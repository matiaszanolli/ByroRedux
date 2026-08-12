# #2664: SCR-D7-NEW11-03: Exterior logical-actor stub open-codes stamp_quest_reference and omits Transform/GlobalTransform, excluding those candidates from every distance-ranked alias fill

**Severity**: MEDIUM
**Dimension**: Engine Attach & Trigger Wiring (Dimension 7)
**Untrusted-Input**: No
**Location**: `byroredux/src/cell_loader/exterior.rs:255-289` (`PersistentCellApplyJob::apply`'s logical-stub loop); contrast `byroredux/src/cell_loader/references/mod.rs:1093-1134` (`stamp_quest_reference` / `spawn_logical_quest_reference`); consumed at `crates/scripting/src/scene.rs:832-856`
**Status**: NEW

## Description

The worldspace persistent-cell loader spawns a "logical actor identity" entity for every persistent `ACHR` that has no 3D (remote actors, or local actors whose spawn produced no root).

It builds the `FormIdComponent` + `SceneAliasCandidate` pair **by hand** -- a verbatim copy of `stamp_quest_reference`'s body -- instead of calling it. And unlike `spawn_logical_quest_reference` (the interior / statics-miss equivalent, which is specifically required to stay `Transform`/`GlobalTransform`-bearing so a mesh-less quest REFR is not dropped from alias candidacy), it inserts **no transform at all** -- even though `placed.position` is available and is already converted elsewhere in the same file.

Two consequences:
1. **Alias-fill correctness.** In `resolve_alias_bindings`, when an alias is distance-ranked (`closest_to_alias`, or `ALIAS_FLAG_CLOSEST` anchoring on the player), the candidate loop is `let position = world.get::<GlobalTransform>(entity)?;` inside a `filter_map` -- so a transform-less candidate is **filtered out entirely**, not merely ranked last. If every eligible candidate for such an alias is a persistent logical stub, `chosen` is `None` and the alias silently stays unfilled.
2. **Divergence hazard.** A future field added to `SceneAliasCandidate`, or a change to what `stamp_quest_reference` attaches, will reach the nine gated call sites and miss this tenth one.

## Evidence

`byroredux/src/cell_loader/exterior.rs:266-282` inserts exactly `FormIdComponent` + `SceneAliasCandidate` and nothing else. `byroredux/src/cell_loader/references/mod.rs:1129-1132` inserts `Transform` + `GlobalTransform` *then* stamps.

`crates/scripting/src/scene.rs:852`: `let position = world.get::<GlobalTransform>(entity)?.translation;` inside a `filter_map`, so a `None` drops the candidate from the `min_by`.

The stub path is reached for `remote_actor_refs` (appended unconditionally at `exterior.rs:343`) and for local actors that produced no live candidate (`:344-349`).

Note this site cannot cause the N-candidates-for-one-REFR corruption #2541 guards against -- there is no SCOL/PKIN fan-out here and `prepare_logical_actor_stubs` de-dups against already-live candidates -- which is why it is filed as its own finding rather than as a gate violation.

## Impact

A quest whose Find-Matching / Unique-Actor alias is authored with "Closest" (or anchored on another alias) can silently fail to fill when its only candidates are persistent-cell logical actors -- precisely the population (Skyrim's persistent worldspace `ACHR`s) M47.3 was built around.

No in-game occurrence was reproduced this pass; the failure mode is structural and silent (an unfilled alias produces no log line and no error).

The duplication itself is a standing regression vector for #2541's invariant.

## Related

#2541 (open -- the `is_primary_synth` test gap; this is the un-covered eleventh stamping site); `docs/engine/m47-3-quest-alias-design.md` section "Remaining subsystem boundary" (this is **not** the deferred unloaded-world Find-Matching search -- these stubs are already candidates, they are just positionless)

## Suggested Fix

Replace the hand-rolled block with a call to `spawn_logical_quest_reference` (making it `pub(crate)` in `cell_loader::references`), passing `Vec3::from_array(zup_to_yup_pos(placed.position))` and the REFR's rotation / scale, so the exterior stub is transform-bearing and identical in shape to the interior one.

Add a test asserting a distance-ranked alias whose only candidate is a logical stub still fills.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives, other parsers, other spawn paths)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh scripting-domain pass, 7 dimension agents).*
