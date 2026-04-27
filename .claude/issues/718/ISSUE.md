# NIF-D4-02: walk_node_lights and walk_node_particle_emitters_flat skip NiSwitchNode/NiLODNode subtrees

URL: https://github.com/matiaszanolli/ByroRedux/issues/718
Labels: bug, nif-parser, import-pipeline, medium

---

## Severity: MEDIUM

## Game Affected
All games (Oblivion → Starfield)

## Location
- `crates/nif/src/import/walk.rs:485-571` (`walk_node_lights`)
- `crates/nif/src/import/walk.rs:579-635` (`walk_node_particle_emitters_flat`)

## Description
`walk_node_hierarchical` (line 113-127) and `walk_node_flat` (line 330-343) explicitly call `switch_active_children` BEFORE `as_ni_node`, because `as_ni_node` returns `None` for `NiSwitchNode` and `NiLODNode` (per the comment at walk.rs:57-60: "NiSwitchNode and NiLODNode are NOT unwrapped here — they need child-filtering logic").

But `walk_node_lights` (line 485) and `walk_node_particle_emitters_flat` (line 579) go straight to `as_ni_node` without the switch fallback, so any `NiPointLight`/`NiSpotLight`/`NiAmbientLight`/`NiDirectionalLight` or `NiPSysBlock` parented under a `NiSwitchNode`/`NiLODNode` silently disappears from the import.

## Evidence
```
walk.rs:495:    if let Some(node) = as_ni_node(block) {
walk.rs:590:    if let Some(node) = as_ni_node(block) {
```
Both functions never call `switch_active_children`; compare to `walk_node_flat:343` which does.

Verified at HEAD `09dbcfc` — `grep -n` on the file confirms no `switch_active_children` calls between lines 485-635.

## Impact
Lights and particle emitters that live inside destruction-stage / weapon-sheath / LOD-gated subtrees (every Skyrim destructible structure, every weapon carry-vs-draw rig, every Skyrim/FO4 LOD-aware torch hierarchy) silently drop. Lighting goes dim where the mesh visibly carries a torch; magic effect emitters under LOD nodes vanish past the first LOD switch.

## Suggested Fix
Mirror the `if let Some((node, active_children)) = switch_active_children(block) { ... }` block from `walk_node_flat:343-378` into both `walk_node_lights` and `walk_node_particle_emitters_flat` (with the appropriate transform composition).

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D4-02)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Verify any other walker functions in `walk.rs` that go directly to `as_ni_node` without the switch fallback — there may be more
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Regression test with a NiSwitchNode containing a NiPointLight child — assert the light is reached by `walk_node_lights`
- [ ] **TESTS**: Regression test with a NiLODNode containing a particle emitter — assert it's reached by `walk_node_particle_emitters_flat`
