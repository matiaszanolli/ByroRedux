# #3533 — SPT-2026-08-28-D3-02: placement_root_billboard has no producer that can ever yield Some, and its docstring plus two consumer comments still describe the pre-#3076 root-billboard model

**Labels**: low, speedtree, terrain-exterior, tech-debt, doc-rot, documentation
**Filed from**: `docs/audits/AUDIT_SPEEDTREE_2026-08-28.md` (`/audit-publish`, 2026-08-28)

---

**Severity**: LOW
**Dimension**: TREE→Billboard Wiring
**Source**: `docs/audits/AUDIT_SPEEDTREE_2026-08-28.md` — SPT-2026-08-28-D3-02

**Location**:
- `byroredux/src/cell_loader/nif_import_registry.rs:148-155` (the field + docstring)
- `byroredux/src/cell_loader/spawn.rs:783-790` (the dead consumer)
- `byroredux/src/systems/billboard.rs:133-134` (the misattributed skip comment)

## Description

#3076 moved the SpeedTree billboard from the placement root onto the renderable mesh.
`import_spt_scene` now builds its root with `placeholder_root_node(/* billboard */ false)`
(`crates/spt/src/import/mod.rs:164`), so `references/import.rs:364-368`'s

```rust
let placement_root_billboard = imported
    .nodes.first().and_then(|n| n.billboard_mode).map(BillboardMode::from_nif);
```

is structurally always `None` — pinned by `import_tests.rs:68`
(`assert_eq!(cached.placement_root_billboard, None)`). Every *other* `CachedNifImport`
constructor hardcodes `None` (`references/import.rs:184`, `partial.rs:114`,
`precombined.rs:786`). Consequently the field is `None` at every construction site in the
codebase, and its only consumer —

```rust
// byroredux/src/cell_loader/spawn.rs:788-790
if let Some(mode) = cached.placement_root_billboard {
    world.insert(placement_root, Billboard::new(mode));
}
```

— is unreachable.

Three pieces of documentation still describe the removed model:

1. `nif_import_registry.rs:148-155`: *"`Some` for `NiBillboardNode`-rooted content and for
   SpeedTree `.spt` placeholders, which need the placement root to yaw-track the camera"* —
   false for `.spt` since #3076, and no NIF path ever sets it either (the same docstring's next
   sentence concedes this).
2. `spawn.rs:783-787`: *"Without this insertion `.spt` REFRs render as static quads"* — the
   insertion no longer happens and `.spt` REFRs do not render as static quads, because
   `mesh_instance.rs:792-794` attaches the `Billboard` on the mesh.
3. `systems/billboard.rs:133-134`, added by `8e97b4e5`: *"A `SpeedTreeWind` without `Billboard`
   has no orientation this system owns — **the placement root is exactly that**."* The placement
   root is *not* that. `grep -rn "SpeedTreeWind"` shows exactly three production insert sites
   (`mesh_instance.rs:799`, `nif_loader.rs:550`, `nif_loader.rs:1034`) and all three are on mesh
   entities that receive `Billboard` in the same statement group; no site attaches
   `SpeedTreeWind` to a placement root. The new test
   `parked_camera_wind_pass_skips_a_marked_entity_without_billboard` (`billboard.rs:515-566`)
   labels its synthetic entity "The placement root: SpeedTreeWind without Billboard" for a
   configuration nothing builds.

## Evidence

The four call sites and three comments quoted above; `crates/spt/src/import/mod.rs:164` and
`:260` (`billboard_mode: billboard.then_some(…)` with `billboard == false`);
`byroredux/src/cell_loader/references/import_tests.rs:68`.

## Impact

None at runtime — the guard in `billboard.rs` is correct defensive code whichever entity
motivates it, and the `spawn.rs` branch is simply never taken. The cost is that three documents
disagree with the code about which entity owns a `.spt` billboard, which is precisely the
question #3076 and #2206 were filed to settle; the next contributor touching this path reads the
*field's own docstring* (the most local, most specific source) and gets the pre-#3076 answer.

## Related

- #3076 — moved the billboard to the mesh.
- #2206 — the per-mesh attach.
- #3192 / `8e97b4e5` — added the third stale comment.
- #3193 — the prior cycle's identical "no production entity is in this configuration"
  determination; still true.

## Suggested Fix

Either delete `placement_root_billboard` and its `spawn.rs` consumer outright (nothing can set
it), or, if it is being kept as a seam for a future `NiBillboardNode`-rooted NIF producer, say
exactly that in the docstring and note that no producer exists today. Reword
`billboard.rs:133-134` to describe the guard as defensive rather than naming the placement root,
and retitle the test entity in `parked_camera_wind_pass_skips_a_marked_entity_without_billboard`
accordingly.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — the other `CachedNifImport` fields documented as SPT-populated but hardcoded `None` at every constructor
- [ ] **TESTS**: A regression test pins this specific fix (if the field is deleted, `import_tests.rs:68`'s assertion goes with it; if kept, the docstring claim is what needs pinning)
