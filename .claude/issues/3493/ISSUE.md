# #3493: SCR-D6-2026-08-27-01: the #3250 fix orphaned apply_effect's nested-lock-safety docstring onto a three-line helper; apply_effect now has no doc comment at all

**Labels**: low, scripting, concurrency, documentation, doc-rot
**Filed**: 2026-08-27 (`/audit-publish` of `docs/audits/AUDIT_SCRIPTING_2026-08-27.md`)

- **Severity**: LOW
- **Dimension**: Scripting Runtime Systems (Dimension 6)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment.rs:603-641`
- **Source**: `docs/audits/AUDIT_SCRIPTING_2026-08-27.md`

## Description

`1d9a5041` (the #3250 fix) inserted the new `copied_transform` helper *between* `apply_effect`'s multi-paragraph doc comment and `fn apply_effect`. Rust attaches a `///` block to the next item, so the entire nested-lock contract — the residual list (`PlayerControlState` ×3, `Globals` ×1, "12 component-storage acquisitions"), the "only safe because every caller is `add_exclusive`" argument, the #2660 `SceneActorBindings` snapshot rationale, and the instruction to re-derive the analysis before adding a lock — is now the documentation for a three-line `Transform` copy helper, and `apply_effect` is undocumented.

This is the exact doc the `/audit-scripting` skill directs auditors to treat as authoritative ("re-read that doc comment rather than this bullet's own count, since it is the thing this bullet is transcribing and will drift again").

## Evidence

```rust
// fragment.rs:634-641
/// Every production caller constructs the batch before taking quest
/// resource guards and applies it only after those guards have dropped
/// (#2269, #2539, #2660).
fn copied_transform(world: &World, entity: EntityId) -> Option<Transform> {
    // #3250 — `World::get` returns an owning read guard. ...
    world.get::<Transform>(entity).map(|transform| *transform)
}

fn apply_effect(          // <- no doc comment
```

Verified against current `crates/scripting/src/fragment.rs`: the doc block beginning "Apply one effect to the canonical stage/objective state…" runs from ~`:604` to `:640` and is immediately followed by `fn copied_transform`, with `fn apply_effect` at `:641` bare.

## Impact

`cargo doc` renders a lock-ordering contract on the wrong item, and a future editor of `apply_effect` no longer sees the "adding a new nested lock here needs the analysis re-derived" instruction adjacent to the code it governs — the same drift the ABBA argument depends on not happening.

## Related

#3250 (CLOSED — the fix itself is correct; only its placement is wrong). Adjacent: #3441 (the live `ActorValues ↔ CharacterRuleset` lock-order cycle in `condition.rs`) shows the cost of a lock-order argument that is not kept adjacent to its code.

## Suggested Fix

Move `copied_transform` above the doc block (or below `apply_effect`), and give it its own one-line doc. Purely mechanical.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other helpers inserted under an existing `///` block by recent fixes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
